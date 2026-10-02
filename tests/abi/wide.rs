use std::num::NonZeroU8;

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
    let bytes = unsafe { <Bytes as Wide>::from_raw_parts_mut(values.as_mut_ptr(), values.len()) };
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

    assert_impl_all!(<Bytes as Wide>::Data: Sized);
    assert_impl_all!(<Packet as Wide>::Data: Sized);
    assert_impl_all!(<TuplePacket as Wide>::Data: Sized);
}

#[test]
fn struct_wide_raw_parts_round_trip() {
    let values: [u8; 3] = [1, 2, 3];
    let bytes = unsafe {
        <Bytes as Wide>::from_raw_parts(
            values.as_ptr().cast::<<Bytes as Wide>::Data>(),
            values.len(),
        )
    };
    assert_eq!(&bytes.0, &values);

    let mut values: [u8; 3] = [1, 2, 3];
    let bytes = unsafe {
        <Bytes as Wide>::from_raw_parts_mut(
            values.as_mut_ptr().cast::<<Bytes as Wide>::Data>(),
            values.len(),
        )
    };
    bytes.0[1] = 9;
    assert_eq!(values, [1, 9, 3]);

    let values = vec![4u8, 5, 6].into_boxed_slice();
    let len = values.len();
    let data = Box::into_raw(values).cast::<<Bytes as Wide>::Data>();
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
    assert_eq!(data.as_ptr(), ptr.cast_mut());

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
