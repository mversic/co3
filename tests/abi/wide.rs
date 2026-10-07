use std::{
    cell::{Cell, UnsafeCell},
    num::NonZeroU8,
};

use co3::{ReprC, ffi, rust_spec::RustSpec, slice::Unpack2, wide::Wide};
use static_assertions::assert_impl_all;

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
struct Bytes([u8]);

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
#[repr_c(identity)]
struct RawBytes([u8]);

#[test]
fn non_null_wide_unpacks_pointer_metadata() {
    let slice = core::ptr::NonNull::slice_from_raw_parts(core::ptr::NonNull::<u8>::dangling(), 3);
    let ptr = core::ptr::NonNull::new(slice.as_ptr() as *mut RawBytes).unwrap();
    let (data, len): (*mut u8, usize) =
        <core::ptr::NonNull<RawBytes> as Unpack2<*mut u8, usize>>::unpack(ptr.as_ptr()).unwrap();
    assert_eq!(data, slice.as_ptr() as *mut u8);
    assert_eq!(len, 3);
    let (const_data, const_len): (*const u8, usize) =
        <core::ptr::NonNull<RawBytes> as Unpack2<*const u8, usize>>::unpack(ptr.as_ptr()).unwrap();
    assert_eq!(const_data, slice.as_ptr() as *const u8);
    assert_eq!(const_len, 3);
    let (raw_const_data, raw_const_len): (*const u8, usize) =
        <*const RawBytes as Unpack2<*const u8, usize>>::unpack(ptr.as_ptr()).unwrap();
    assert_eq!(raw_const_data, const_data);
    assert_eq!(raw_const_len, 3);
    let (raw_mut_data, raw_mut_len): (*mut u8, usize) =
        <*mut RawBytes as Unpack2<*mut u8, usize>>::unpack(ptr.as_ptr()).unwrap();
    assert_eq!(raw_mut_data, data);
    assert_eq!(raw_mut_len, 3);
}

#[test]
fn cell_wrappers_reconstruct_borrowed_wide_references() {
    let cell = UnsafeCell::new([1u8, 2, 3]);
    let ptr: *const UnsafeCell<[u8]> = &cell;
    let data = <UnsafeCell<[u8]> as Wide>::as_ptr(ptr);
    let len = <UnsafeCell<[u8]> as Wide>::metadata(ptr);
    let rebuilt = unsafe { <UnsafeCell<[u8]> as Wide>::from_raw_parts(data, len) };
    assert!(core::ptr::eq(ptr, rebuilt));

    let mut cell = Cell::new([4u8, 5, 6]);
    let ptr: *mut Cell<[u8]> = &mut cell;
    let data = <Cell<[u8]> as Wide>::as_mut_ptr(ptr);
    let len = <Cell<[u8]> as Wide>::metadata(ptr);
    let rebuilt = unsafe { <Cell<[u8]> as Wide>::from_raw_parts_mut(data, len) };
    rebuilt.get_mut()[1] = 9;
    assert_eq!(cell.into_inner(), [4, 9, 6]);

    let mut text = String::from("hé");
    let cell = Cell::from_mut(text.as_mut_str());
    let ptr = core::ptr::from_ref(cell);
    let data = <Cell<str> as Wide>::as_ptr(ptr);
    let len = <Cell<str> as Wide>::metadata(ptr);
    let rebuilt = unsafe { <Cell<str> as Wide>::from_raw_parts(data, len) };
    assert!(core::ptr::eq(ptr, rebuilt));
    assert_eq!(len, 3);

    let mut text = String::from("hé");
    let cell = UnsafeCell::from_mut(text.as_mut_str());
    let ptr = core::ptr::from_mut(cell);
    let data = <UnsafeCell<str> as Wide>::as_mut_ptr(ptr);
    let len = <UnsafeCell<str> as Wide>::metadata(ptr);
    let rebuilt = unsafe { <UnsafeCell<str> as Wide>::from_raw_parts_mut(data, len) };
    assert!(core::ptr::eq(ptr, rebuilt));
    assert_eq!(rebuilt.get_mut(), "hé");
}

#[test]
fn shared_unsafe_cell_wide_uses_mutable_carrier() {
    let cell = UnsafeCell::new([1u8, 2, 3]);
    let original: &UnsafeCell<[u8]> = &cell;

    let carrier: co3::slice::CSliceMut<u8, false> = co3::encode(original);
    let (data, len) = <&UnsafeCell<[u8]> as Unpack2<*mut u8, usize>>::unpack(carrier).unwrap();
    assert_eq!(data, cell.get().cast());
    assert_eq!(len, 3);
    unsafe { data.add(1).write(9) };

    let decoded = unsafe { co3::decode::<&UnsafeCell<[u8]>>(carrier) }.unwrap();
    assert!(core::ptr::eq(original, decoded));
    assert_eq!(unsafe { &*cell.get() }, &[1, 9, 3]);
}

#[test]
fn boxed_cell_wrappers_keep_their_allocation() {
    let original: Box<UnsafeCell<[u8]>> = Box::new(UnsafeCell::new([1, 2, 3]));
    let ptr = <UnsafeCell<[u8]> as Wide>::as_ptr(core::ptr::from_ref(&*original));
    let len = <UnsafeCell<[u8]> as Wide>::metadata(core::ptr::from_ref(&*original));
    let data = <UnsafeCell<[u8]> as Wide>::into_non_null(original);
    assert_eq!(data.as_ptr(), ptr.cast_mut());
    let mut recovered = unsafe { <UnsafeCell<[u8]> as Wide>::from_non_null(data, len) };
    assert_eq!(recovered.get_mut(), &[1, 2, 3]);

    let original: Box<Cell<[u8]>> = Box::new(Cell::new([4, 5, 6]));
    let ptr = <Cell<[u8]> as Wide>::as_ptr(core::ptr::from_ref(&*original));
    let len = <Cell<[u8]> as Wide>::metadata(core::ptr::from_ref(&*original));
    let data = <Cell<[u8]> as Wide>::into_non_null(original);
    assert_eq!(data.as_ptr(), ptr.cast_mut());
    let mut recovered = unsafe { <Cell<[u8]> as Wide>::from_non_null(data, len) };
    assert_eq!(recovered.get_mut(), &[4, 5, 6]);
}

#[unsafe(no_mangle)]
extern "C" fn unpack_receiver_fill(data: *mut u8, len: usize) -> usize {
    let bytes = unsafe { core::slice::from_raw_parts_mut(data, len) };
    bytes.copy_from_slice(&[4, 5, 6]);
    len
}

#[unsafe(no_mangle)]
extern "C" fn unpack_receiver_sum(data: *const u8, len: usize) -> usize {
    let bytes = unsafe { core::slice::from_raw_parts(data, len) };
    bytes.iter().map(|&byte| byte as usize).sum()
}

ffi! {
    #![unsafe(extern("C"))]

    impl Bytes {
        #[symbol_name = "unpack_receiver_fill"]
        fn fill(#[unpack(_, usize)] &mut self) -> usize;

        #[symbol_name = "unpack_receiver_sum"]
        fn sum(#[unpack(_, usize)] &self) -> usize;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "unpack_receiver_sum"]
    fn unpack_raw_const(#[unpack(_, _)] bytes: *const RawBytes) -> usize;

    #[symbol_name = "unpack_receiver_fill"]
    fn unpack_raw_mut(#[unpack(_, _)] bytes: *mut RawBytes) -> usize;
}

#[test]
fn raw_wide_pointer_placeholders_infer_data_and_metadata() {
    let mut values = [1u8, 2, 3];
    let bytes =
        core::ptr::slice_from_raw_parts_mut(values.as_mut_ptr(), values.len()) as *mut RawBytes;
    assert_eq!(unpack_raw_const(bytes), 6);
    assert_eq!(unpack_raw_mut(bytes), 3);
    assert_eq!(values, [4, 5, 6]);
}

#[test]
fn unpacked_receivers_call_c_with_data_and_length() {
    let mut values = [1u8, 2, 3];
    let bytes =
        unsafe { <Bytes as Wide>::from_raw_parts_mut(values.as_mut_ptr().cast(), values.len()) };
    assert_eq!(bytes.sum(), 6);
    assert_eq!(bytes.fill(), 3);
    assert_eq!(values, [4, 5, 6]);
}

#[repr(C)]
#[derive(RustSpec, ReprC)]
struct Packet {
    tag: NonZeroU8,
    payload: [u8],
}

#[repr(C)]
#[derive(RustSpec, ReprC)]
struct TuplePacket(NonZeroU8, [u8]);

#[test]
fn struct_wide_classification() {
    assert_impl_all!(Bytes: Wide<Metadata = usize>);
    assert_impl_all!(Packet: Wide<Metadata = usize>);
    assert_impl_all!(TuplePacket: Wide<Metadata = usize>);

    assert_impl_all!(<Bytes as Wide>::Header: Sized);
    assert_impl_all!(<Packet as Wide>::Header: Sized);
    assert_impl_all!(<TuplePacket as Wide>::Header: Sized);
}

#[test]
fn struct_wide_raw_parts_preserve_prefix_provenance() {
    #[repr(C)]
    struct Storage {
        tag: NonZeroU8,
        payload: [u8; 3],
    }

    let storage = Storage {
        tag: NonZeroU8::new(1).unwrap(),
        payload: [2, 3, 4],
    };
    let packet = unsafe {
        <Packet as Wide>::from_raw_parts(
            core::ptr::from_ref(&storage).cast::<<Packet as Wide>::Header>(),
            storage.payload.len(),
        )
    };

    assert_eq!(packet.tag, storage.tag);
    assert_eq!(&packet.payload, &storage.payload);

    let mut storage = Storage {
        tag: NonZeroU8::new(5).unwrap(),
        payload: [6, 7, 8],
    };
    let packet = unsafe {
        <Packet as Wide>::from_raw_parts_mut(
            core::ptr::from_mut(&mut storage).cast::<<Packet as Wide>::Header>(),
            3,
        )
    };
    packet.payload[1] = 9;
    assert_eq!(storage.payload, [6, 9, 8]);

    let storage = Box::new(Storage {
        tag: NonZeroU8::new(10).unwrap(),
        payload: [11, 12, 13],
    });
    let data =
        core::ptr::NonNull::new(Box::into_raw(storage).cast::<<Packet as Wide>::Header>()).unwrap();
    let packet = unsafe { <Packet as Wide>::from_non_null(data, 3) };
    assert_eq!(packet.tag.get(), 10);
    assert_eq!(&packet.payload, &[11, 12, 13]);
}

#[test]
fn struct_wide_raw_parts_round_trip() {
    let values: [u8; 3] = [1, 2, 3];
    let bytes = unsafe {
        <Bytes as Wide>::from_raw_parts(
            values.as_ptr().cast::<<Bytes as Wide>::Header>(),
            values.len(),
        )
    };
    assert_eq!(&bytes.0, &values);

    let mut values: [u8; 3] = [1, 2, 3];
    let bytes = unsafe {
        <Bytes as Wide>::from_raw_parts_mut(
            values.as_mut_ptr().cast::<<Bytes as Wide>::Header>(),
            values.len(),
        )
    };
    bytes.0[1] = 9;
    assert_eq!(values, [1, 9, 3]);

    let values = vec![4u8, 5, 6].into_boxed_slice();
    let len = values.len();
    let data = Box::into_raw(values).cast::<<Bytes as Wide>::Header>();
    let bytes =
        unsafe { <Bytes as Wide>::from_non_null(core::ptr::NonNull::new_unchecked(data), len) };
    let metadata = Bytes::metadata(core::ptr::from_ref(&*bytes));
    let data = bytes.into_non_null();
    let bytes = unsafe { <Bytes as Wide>::from_non_null(data, metadata) };
    assert_eq!(&bytes.0, &[4, 5, 6]);
}

#[test]
fn boxed_str_non_null_round_trip() {
    let value: Box<str> = "héllo".into();
    let len = value.len();
    let ptr = value.as_ptr();

    let data = value.into_non_null();
    assert_eq!(data.as_ptr(), ptr.cast_mut().cast());

    let value = unsafe { <str as Wide>::from_non_null(data, len) };
    assert_eq!(&*value, "héllo");
}

#[test]
fn empty_and_zero_sized_boxed_wide_round_trip() {
    let empty: Box<[u8]> = Vec::new().into_boxed_slice();
    let data = empty.into_non_null();
    let empty = unsafe { <[u8] as Wide>::from_non_null(data, 0) };
    assert!(empty.is_empty());

    let zero_sized = vec![(); 3].into_boxed_slice();
    let data = zero_sized.into_non_null();
    let zero_sized = unsafe { <[()] as Wide>::from_non_null(data, 3) };
    assert_eq!(zero_sized.len(), 3);

    let empty: Box<str> = "".into();
    let data = empty.into_non_null();
    let empty = unsafe { <str as Wide>::from_non_null(data, 0) };
    assert!(empty.is_empty());
}
