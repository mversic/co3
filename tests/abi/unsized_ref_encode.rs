use core::ffi::CStr;

use co3::{
    ReprC,
    rust_spec::RustSpec,
    stored::{
        EncodeMove, EncodeMoveUnchecked, EncodeMut, EncodeMutUnchecked, EncodeRef,
        EncodeRefUnchecked, Store,
    },
};

#[test]
fn mutable_slice_decode_writes_changes_back() {
    use co3::{slice::CWideRefMut, stored::DecodeOwned, tuple::ReprCTuple2};

    let mut values = [ReprCTuple2(1_u8, 2_u8)];
    let source = CWideRefMut::from_mut(&mut values[..]);
    let mut store = Default::default();
    let decoded = unsafe { <&mut [(u8, u8)]>::soft_decode(source, &mut store) }.unwrap();
    decoded[0] = (3, 4);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values[0], ReprCTuple2(3, 4));
}

// Neither the element nor its owned slice supports cloning.
#[derive(RustSpec, co3::ReprC, Debug, PartialEq)]
struct Element(Vec<u32>);

#[test]
fn shared_unstable_slice_encodes_without_clone() {
    let values = [Element(vec![3, 5]), Element(vec![7])];
    let original = values[0].0.as_ptr();
    let mut store = Default::default();
    let encoded = values.as_slice().soft_encode(&mut store);
    assert_eq!(encoded.metadata(), 2);
    let field = unsafe { &(*encoded.as_ptr()).0 };
    assert_eq!(field.as_ptr().cast_const(), original);
    assert_eq!(unsafe { *field.as_ptr().add(1) }, 5);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let encoded = unsafe { values.as_slice().soft_encode_unchecked(&mut store) };
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
    let encoded = values.as_slice().soft_encode(&mut store);
    assert_eq!(encoded.metadata(), 0);
    assert!(!encoded.as_ptr().is_null());
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn nul_terminated_reference_uses_explicit_encoder_without_storage() {
    let value = c"borrowed";
    let encoded: <&core::ffi::CStr as ReprC>::CType = value.encode_ref(&mut ());
    assert_eq!(encoded.as_ptr().cast::<core::ffi::c_char>(), value.as_ptr());
    let encoded = unsafe { value.encode_ref_unchecked(&mut ()) };
    assert_eq!(encoded.as_ptr().cast::<core::ffi::c_char>(), value.as_ptr());
    let wrapped: &core::mem::ManuallyDrop<core::ffi::CStr> = unsafe {
        &*(value as *const core::ffi::CStr as *const core::mem::ManuallyDrop<core::ffi::CStr>)
    };
    assert_eq!(wrapped.soft_encode(&mut ()).as_ptr(), encoded.as_ptr());
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
    unsafe fn encode_ref_unchecked(
        &'a self,
        store: &mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType {
        unsafe { self.0.encode_ref_unchecked(store) }
    }
}
unsafe impl<'a> EncodeRef<'a> for SliceView {
    type Store = <[Element] as EncodeRef<'a>>::Store;
    fn encode_ref(&'a self, store: &mut Self::Store) -> <&'a Self as ReprC>::CType {
        self.0.encode_ref(store)
    }
}

#[test]
fn downstream_unsized_type_delegates_through_reference_traits() {
    let mut values = [Element(vec![23])];
    let value = unsafe { &mut *(values.as_mut_slice() as *mut [Element] as *mut SliceView) };
    let mut store = Default::default();
    let encoded = (&*value).soft_encode(&mut store);
    assert_eq!(encoded.metadata(), 1);
    assert_eq!(store.sync(), Some(()));

    let wrapped: &core::mem::ManuallyDrop<SliceView> =
        unsafe { &*(value as *const SliceView as *const core::mem::ManuallyDrop<SliceView>) };
    let mut store = Default::default();
    let encoded = wrapped.encode_ref(&mut store);
    assert_eq!(encoded.metadata(), 1);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let encoded = unsafe { wrapped.encode_ref_unchecked(&mut store) };
    assert_eq!(encoded.metadata(), 1);
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn boxed_custom_wide_type_delegates_to_pointee_encoder() {
    let values: Box<[Element]> = Box::new([Element(vec![23])]);
    let boxed: Box<SliceView> = unsafe { Box::from_raw(Box::into_raw(values) as *mut SliceView) };
    let mut store = Default::default();
    let encoded = boxed.encode_ref(&mut store);
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(header.metadata(), 1);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let encoded = unsafe { boxed.encode_ref_unchecked(&mut store) };
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(header.metadata(), 1);
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn stable_slice_paths_remain_store_free() {
    let mut values = [31_u32, 37];
    let encoded = values.as_slice().soft_encode(&mut ());
    assert_eq!(encoded.as_ptr(), values.as_ptr());
    let original = values.as_mut_ptr();
    let encoded = values.as_mut_slice().soft_encode(&mut ());
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
    unsafe fn encode_mut_unchecked(&'a mut self, _: &mut ()) -> <&'a mut Self as ReprC>::CType {
        co3::reference::CRefMut::from_mut(&mut self.0)
    }
}
unsafe impl<'a> EncodeMut<'a> for Explicit {
    type Store = ();
    fn encode_mut(&'a mut self, _: &mut ()) -> <&'a mut Self as ReprC>::CType {
        co3::reference::CRefMut::from_mut(&mut self.0)
    }
}

#[test]
fn explicit_sized_mutable_reference_encoder_needs_no_owned_decoder() {
    let mut value = Explicit(41);
    let encoded = (&mut value).soft_encode(&mut ());
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
    unsafe fn encode_mut_unchecked(&'a mut self, _: &mut ()) -> <&'a mut Self as ReprC>::CType {
        unsafe { (&mut self.0).soft_encode_unchecked(&mut ()) }
    }
}
unsafe impl<'a> EncodeMut<'a> for MutSliceView {
    type Store = ();
    fn encode_mut(&'a mut self, _: &mut ()) -> <&'a mut Self as ReprC>::CType {
        (&mut self.0).soft_encode(&mut ())
    }
}
#[test]
fn explicit_unsized_mutable_encoder_needs_no_value_helper() {
    let mut values = [23_u32];
    let view = unsafe { &mut *(values.as_mut_slice() as *mut [u32] as *mut MutSliceView) };
    let encoded = view.soft_encode(&mut ());
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
    let encoded = wrapped.encode_mut(&mut store);
    assert_eq!(encoded.metadata(), 1);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values[0], [3]);

    let mut values = [true];
    let wrapped: &mut core::cell::Cell<[bool]> =
        unsafe { &mut *(values.as_mut_slice() as *mut [bool] as *mut core::cell::Cell<[bool]>) };
    let mut store = Default::default();
    let encoded = wrapped.encode_mut(&mut store);
    unsafe { *encoded.as_ptr() = co3::primitives::CBool::FALSE };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values, [false]);

    let mut values = [true];
    let wrapped: &mut core::cell::UnsafeCell<[bool]> = unsafe {
        &mut *(values.as_mut_slice() as *mut [bool] as *mut core::cell::UnsafeCell<[bool]>)
    };
    let mut store = Default::default();
    let encoded = wrapped.encode_mut(&mut store);
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
    static_assertions::assert_impl_all!(WrappedCStr:
        co3::stored::EncodeRef<'static>, co3::stored::EncodeRefUnchecked<'static>);
    static_assertions::assert_not_impl_any!(WrappedCStr:
        co3::stored::EncodeMut<'static>, co3::stored::EncodeMutUnchecked<'static>);
    use co3::stored::EncodeRef;
    let value = c"hello";
    let wrapper = unsafe { &*(value as *const CStr as *const WrappedCStr) };
    let mut store = Default::default();
    let encoded = wrapper.encode_ref(&mut store);
    assert_eq!(unsafe { CStr::from_ptr(encoded.as_ptr().cast()) }, value);
    assert_eq!(store.sync(), Some(()));
    let mut store = Default::default();
    let encoded = unsafe { wrapper.encode_ref_unchecked(&mut store) };
    assert_eq!(unsafe { CStr::from_ptr(encoded.as_ptr().cast()) }, value);
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn nontransparent_nul_tail_has_no_reference_encoder() {
    static_assertions::assert_not_impl_any!(CStrTail:
        co3::stored::EncodeMut<'static>, co3::stored::EncodeMutUnchecked<'static>,
        co3::stored::EncodeRef<'static>, co3::stored::EncodeRefUnchecked<'static>);
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
    static_assertions::assert_impl_all!(WrappedSlice<u32>:
        EncodeRef<'static>, EncodeRefUnchecked<'static>,
        EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_impl_all!(WrappedSlice<core::ffi::CStr>:
        EncodeRef<'static>, EncodeRefUnchecked<'static>);
    static_assertions::assert_not_impl_any!(WrappedSlice<core::ffi::CStr>:
        EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_impl_all!(WrappedSlice<[u32]>:
        EncodeRef<'static>, EncodeRefUnchecked<'static>,
        EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_impl_all!(WrappedSlice<Packet>:
        EncodeRef<'static>, EncodeRefUnchecked<'static>,
        EncodeMut<'static>, EncodeMutUnchecked<'static>);
    let mut values = [1_u32, 2];
    let wrapper = unsafe { &mut *(&mut values[..] as *mut [u32] as *mut WrappedSlice<[u32]>) };
    let mut store = Default::default();
    let encoded = EncodeRef::encode_ref(&*wrapper, &mut store);
    assert_eq!(unsafe { encoded.into_ref().unwrap() }, &[1, 2]);
    assert_eq!(store.sync(), Some(()));
    let mut store = Default::default();
    let encoded = wrapper.encode_mut(&mut store);
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
    static_assertions::assert_not_impl_any!(Packet: EncodeRef<'static>, EncodeRefUnchecked<'static>, EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_impl_all!(GenericPacket<u8>: EncodeRef<'static>, EncodeRefUnchecked<'static>);
    static_assertions::assert_not_impl_any!(GenericPacket<[u8]>: EncodeRef<'static>, EncodeRefUnchecked<'static>, EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_not_impl_any!(GenericPacket<str>: EncodeRef<'static>, EncodeRefUnchecked<'static>, EncodeMut<'static>, EncodeMutUnchecked<'static>);
}
