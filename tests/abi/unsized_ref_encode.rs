use core::ffi::CStr;

use co3::{
    ReprC,
    rust_spec::RustSpec,
    stored::{EncodeMut, EncodeMutUnchecked, EncodeRef, EncodeRefUnchecked, Store},
};

#[test]
fn mutable_slice_decode_writes_changes_back() {
    use co3::{tuple::Tuple2, wide::CWideRefMut};

    let mut values = [Tuple2(1_u8, 2_u8)];
    let source = CWideRefMut::from_mut(&mut values[..]);
    let mut store = Default::default();
    let decoded = unsafe { co3::soft_decode::<&mut [(u8, u8)]>(source, &mut store) }.unwrap();
    decoded[0] = (3, 4);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values[0], Tuple2(3, 4));
}

// Neither the element nor its owned slice supports cloning.
#[derive(RustSpec, co3::ReprC, Debug, PartialEq)]
struct Element(Vec<u32>);

#[test]
fn shared_unstable_slice_encodes_without_clone() {
    let values = [Element(vec![3, 5]), Element(vec![7])];
    let original = values[0].0.as_ptr();
    let mut store = Default::default();
    let encoded = co3::soft_encode(values.as_slice(), &mut store);
    assert_eq!(encoded.metadata(), 2);
    let field = unsafe { &(*encoded.as_ptr()).0 };
    assert_eq!(field.as_ptr().cast_const(), original);
    assert_eq!(unsafe { *field.as_ptr().add(1) }, 5);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(values.as_slice(), &mut store) };
    assert_eq!(encoded.metadata(), 2);
    assert_eq!(
        unsafe { (*encoded.as_ptr()).0.as_ptr().cast_const() },
        original
    );
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn empty_unstable_slices_encode_and_sync() {
    let values: [Element; 0] = [];
    let mut store = Default::default();
    let encoded = co3::soft_encode(values.as_slice(), &mut store);
    assert_eq!(encoded.metadata(), 0);
    assert!(!encoded.as_ptr().is_null());
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn nul_terminated_reference_uses_explicit_encoder_without_storage() {
    let value = c"borrowed";
    let encoded: <&core::ffi::CStr as ReprC>::CType =
        co3::soft_encode(value, &mut Default::default());
    assert_eq!(encoded.as_ptr().cast::<core::ffi::c_char>(), value.as_ptr());
    let encoded = unsafe { co3::soft_encode_unchecked(value, &mut Default::default()) };
    assert_eq!(encoded.as_ptr().cast::<core::ffi::c_char>(), value.as_ptr());
    let wrapped: &core::mem::ManuallyDrop<core::ffi::CStr> = unsafe {
        &*(value as *const core::ffi::CStr as *const core::mem::ManuallyDrop<core::ffi::CStr>)
    };
    assert_eq!(
        co3::soft_encode(wrapped, &mut Default::default()).as_ptr(),
        encoded.as_ptr()
    );
}

// A downstream unsized type can implement reference encoding without ToOwned.
#[derive(RustSpec)]
#[repr(transparent)]
struct SliceView([Element]);
impl ReprC for SliceView {
    type CType = <[Element] as ReprC>::CType;
}
unsafe impl<'a> EncodeRefUnchecked<'a> for SliceView {
    type SoftStore = <[Element] as EncodeRefUnchecked<'a>>::SoftStore;
    unsafe fn encode_ref_unchecked<'itm>(
        &'a self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        let inner = unsafe { co3::soft_encode_unchecked(&self.0, store) };
        unsafe { co3::wide::CWideRef::<Self>::from_raw_parts(inner.as_ptr(), inner.metadata()) }
    }
}
unsafe impl<'a> EncodeRef<'a> for SliceView {
    type Store = <[Element] as EncodeRef<'a>>::Store;
    fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        let inner = co3::soft_encode(&self.0, store);
        unsafe { co3::wide::CWideRef::<Self>::from_raw_parts(inner.as_ptr(), inner.metadata()) }
    }
}

#[test]
fn downstream_unsized_type_delegates_through_reference_traits() {
    let mut values = [Element(vec![23])];
    let value = unsafe { &mut *(values.as_mut_slice() as *mut [Element] as *mut SliceView) };
    let mut store = Default::default();
    let encoded = co3::soft_encode(&*value, &mut store);
    assert_eq!(encoded.metadata(), 1);
    assert_eq!(store.sync(), Some(()));

    let wrapped: &core::mem::ManuallyDrop<SliceView> =
        unsafe { &*(value as *const SliceView as *const core::mem::ManuallyDrop<SliceView>) };
    let mut store = Default::default();
    let encoded = co3::soft_encode(wrapped, &mut store);
    assert_eq!(encoded.metadata(), 1);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(wrapped, &mut store) };
    assert_eq!(encoded.metadata(), 1);
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn boxed_custom_wide_type_delegates_to_pointee_encoder() {
    let values: Box<[Element]> = Box::new([Element(vec![23])]);
    let boxed: Box<SliceView> = unsafe { Box::from_raw(Box::into_raw(values) as *mut SliceView) };
    let mut store = Default::default();
    let encoded = co3::soft_encode(&boxed, &mut store);
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(header.metadata(), 1);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(&boxed, &mut store) };
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(header.metadata(), 1);
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn stable_slice_paths_remain_store_free() {
    let mut values = [31_u32, 37];
    let encoded = co3::encode(values.as_slice());
    assert_eq!(encoded.as_ptr(), values.as_ptr());
    let original = values.as_mut_ptr();
    let encoded = co3::encode(values.as_mut_slice());
    assert_eq!(encoded.as_ptr(), original);
}

// Explicit reference encoders do not require an owned decoder or value helper.
#[derive(RustSpec)]
struct Explicit(u32);
impl ReprC for Explicit {
    type CType = u32;
}
unsafe impl<'a> EncodeMutUnchecked<'a> for Explicit {
    type SoftStore = ();
    unsafe fn encode_mut_unchecked<'itm>(
        &'a mut self,
        _: &'itm mut (),
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        co3::reference::CRefMut::from_mut(&mut self.0)
    }
}
unsafe impl<'a> EncodeMut<'a> for Explicit {
    type Store = ();
    fn encode_mut<'itm>(&'a mut self, _: &'itm mut ()) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        co3::reference::CRefMut::from_mut(&mut self.0)
    }
}

#[test]
fn explicit_sized_mutable_reference_encoder_needs_no_owned_decoder() {
    let mut value = Explicit(41);
    let encoded = co3::soft_encode(&mut value, &mut Default::default());
    unsafe { encoded.as_ptr().write(43) };
    assert_eq!(value.0, 43);
}

// A custom unstable DST can still supply its own reference encoder.
#[derive(RustSpec)]
struct MutSliceView([u32]);
impl ReprC for MutSliceView {
    type CType = [u32];
}
unsafe impl<'a> EncodeMutUnchecked<'a> for MutSliceView {
    type SoftStore = ();
    unsafe fn encode_mut_unchecked<'itm>(
        &'a mut self,
        _: &'itm mut (),
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        let inner = unsafe { co3::encode_unchecked(&mut self.0) };
        unsafe {
            co3::wide::CWideRefMut::<Self, true>::from_raw_parts_mut(
                inner.as_ptr(),
                inner.metadata(),
            )
        }
    }
}
unsafe impl<'a> EncodeMut<'a> for MutSliceView {
    type Store = ();
    fn encode_mut<'itm>(&'a mut self, _: &'itm mut ()) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        let inner = co3::encode(&mut self.0);
        unsafe {
            co3::wide::CWideRefMut::<Self, true>::from_raw_parts_mut(
                inner.as_ptr(),
                inner.metadata(),
            )
        }
    }
}
#[test]
fn explicit_unsized_mutable_encoder_needs_no_value_helper() {
    let mut values = [23_u32];
    let view = unsafe { &mut *(values.as_mut_slice() as *mut [u32] as *mut MutSliceView) };
    let encoded = co3::soft_encode(view, &mut Default::default());
    unsafe { encoded.as_ptr().write(29) };
    assert_eq!(values, [29]);
    static_assertions::assert_not_impl_any!([Element]: EncodeMut<'static>, EncodeMutUnchecked<'static>);
}

#[test]
fn wide_mutable_wrappers_delegate_to_inner_encoder() {
    let mut values = [vec![3_u32]];
    let wrapped: &mut core::mem::ManuallyDrop<[Vec<u32>]> = unsafe {
        &mut *(values.as_mut_slice() as *mut [Vec<u32>] as *mut core::mem::ManuallyDrop<[Vec<u32>]>)
    };
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut *wrapped, &mut store);
    assert_eq!(encoded.metadata(), 1);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values[0], [3]);

    let mut values = [true];
    let wrapped: &mut core::cell::Cell<[bool]> =
        unsafe { &mut *(values.as_mut_slice() as *mut [bool] as *mut core::cell::Cell<[bool]>) };
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut *wrapped, &mut store);
    unsafe { *encoded.as_ptr() = co3::primitives::CBool::FALSE };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values, [false]);

    let mut values = [true];
    let wrapped: &mut core::cell::UnsafeCell<[bool]> = unsafe {
        &mut *(values.as_mut_slice() as *mut [bool] as *mut core::cell::UnsafeCell<[bool]>)
    };
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut *wrapped, &mut store);
    unsafe { *encoded.as_ptr() = co3::primitives::CBool::FALSE };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values, [false]);
}

#[repr(C)]
#[derive(RustSpec, ReprC)]
struct CStrTail {
    tag: u32,
    tail: CStr,
}

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct WrappedCStr(CStr);

#[test]
fn transparent_cstr_has_nul_terminated_buffer() {
    static_assertions::assert_impl_all!(WrappedCStr: co3::ffi::NulTerminatedBuf);
    assert_shared_encoder!(WrappedCStr: co3::stored::EncodeRef<'static>, co3::stored::EncodeRefUnchecked<'static>);
    static_assertions::assert_not_impl_any!(WrappedCStr:
        co3::stored::EncodeMut<'static>, co3::stored::EncodeMutUnchecked<'static>);

    let value = c"hello";
    let wrapper = unsafe { &*(value as *const CStr as *const WrappedCStr) };
    let mut store = ();
    let encoded = co3::soft_encode(wrapper, &mut store);
    assert_eq!(unsafe { CStr::from_ptr(encoded.as_ptr().cast()) }, value);
    assert_eq!(store.sync(), Some(()));
    let mut store = ();
    let encoded = unsafe { co3::soft_encode_unchecked(wrapper, &mut store) };
    assert_eq!(unsafe { CStr::from_ptr(encoded.as_ptr().cast()) }, value);
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn nontransparent_nul_tail_has_no_reference_encoder() {
    assert_not_shared_encoder!(CStrTail: co3::stored::EncodeRef<'static>, co3::stored::EncodeRefUnchecked<'static>);
    static_assertions::assert_not_impl_any!(CStrTail: co3::stored::EncodeMut<'static>, co3::stored::EncodeMutUnchecked<'static>);
}

#[repr(C)]
#[derive(RustSpec, ReprC)]
struct Packet {
    tag: u32,
    tail: [u8],
}

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct WrappedSlice<T: ?Sized>(T);

#[test]
fn transparent_wide_wrapper_delegates_reference_encoding() {
    use co3::stored::{EncodeMut, EncodeMutUnchecked, EncodeRefUnchecked};
    assert_shared_encoder!(WrappedSlice<u32>: EncodeRef<'static>, EncodeRefUnchecked<'static>,
        EncodeMut<'static>, EncodeMutUnchecked<'static>);
    assert_shared_encoder!(WrappedSlice<core::ffi::CStr>: EncodeRef<'static>, EncodeRefUnchecked<'static>);
    static_assertions::assert_not_impl_any!(WrappedSlice<core::ffi::CStr>:
        EncodeMut<'static>, EncodeMutUnchecked<'static>);
    assert_shared_encoder!(WrappedSlice<[u32]>: EncodeRef<'static>, EncodeRefUnchecked<'static>,
        EncodeMut<'static>, EncodeMutUnchecked<'static>);
    assert_shared_encoder!(WrappedSlice<Packet>: EncodeRef<'static>, EncodeRefUnchecked<'static>,
        EncodeMut<'static>, EncodeMutUnchecked<'static>);
    let mut values = [1_u32, 2];
    let wrapper = unsafe { &mut *(&mut values[..] as *mut [u32] as *mut WrappedSlice<[u32]>) };
    let mut store = ();
    let encoded = co3::soft_encode(&*wrapper, &mut store);
    assert_eq!(unsafe { encoded.into_ref().unwrap() }, &[1, 2]);
    assert_eq!(store.sync(), Some(()));
    let mut store = ();
    let encoded = co3::soft_encode(&mut *wrapper, &mut store);
    unsafe { encoded.as_ptr().cast::<u32>().write(3) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values, [3, 2]);
}

#[repr(C)]
#[derive(RustSpec, ReprC)]
struct GenericPacket<T: ?Sized> {
    tag: u32,
    tail: T,
}

#[test]
fn derived_wide_aggregate_reference_encoders_are_unsupported() {
    use co3::stored::{EncodeMut, EncodeMutUnchecked, EncodeRefUnchecked};
    assert_not_shared_encoder!(Packet: EncodeRef<'static>, EncodeRefUnchecked<'static>);
    static_assertions::assert_not_impl_any!(Packet: EncodeMut<'static>, EncodeMutUnchecked<'static>);
    assert_shared_encoder!(GenericPacket<u8>: EncodeRef<'static>, EncodeRefUnchecked<'static>);
    assert_not_shared_encoder!(GenericPacket<[u8]>: EncodeRef<'static>, EncodeRefUnchecked<'static>);
    static_assertions::assert_not_impl_any!(GenericPacket<[u8]>: EncodeMut<'static>, EncodeMutUnchecked<'static>);
    assert_not_shared_encoder!(GenericPacket<str>: EncodeRef<'static>, EncodeRefUnchecked<'static>);
    static_assertions::assert_not_impl_any!(GenericPacket<str>: EncodeMut<'static>, EncodeMutUnchecked<'static>);
}
