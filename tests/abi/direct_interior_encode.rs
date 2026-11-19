use core::{cell::UnsafeCell, mem::ManuallyDrop, num::NonZeroU8};

use co3::marker::AbiCompatible;
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(transparent)]
struct Wrapped<'a>(&'a UnsafeCell<NonZeroU8>);

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(transparent)]
struct TransparentVec<T>(Vec<T>);

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(transparent)]
struct ConcreteTransparentVec(Vec<(u8,)>);

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(transparent)]
struct ConcreteTransparentByte(u8);

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(transparent)]
struct TransparentBox<T>(Box<T>);

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(C)]
struct RecordVec<T> {
    value: Vec<T>,
}

#[test]
fn transparent_wrappers_preserve_owned_marker_restrictions() {
    assert_impl_all!(TransparentVec<(u8,)>: co3::convert::EncodeMove, co3::convert::EncodeMoveUnchecked, co3::convert::DecodeMove<'static>);
    assert_impl_all!(TransparentVec<(u8,)>: co3::convert::Encode, co3::convert::EncodeUnchecked, co3::convert::Decode<'static>);
    assert_impl_all!(TransparentVec<u8>: co3::convert::Encode, co3::convert::EncodeUnchecked, co3::convert::Decode<'static>);
    assert_impl_all!(ConcreteTransparentVec: co3::convert::EncodeMove, co3::convert::EncodeMoveUnchecked, co3::convert::DecodeMove<'static>);
    assert_impl_all!(ConcreteTransparentVec: co3::convert::Encode, co3::convert::EncodeUnchecked, co3::convert::Decode<'static>);
    assert_impl_all!(ConcreteTransparentByte: AbiCompatible);

    assert_impl_all!(TransparentBox<(u8,)>: co3::convert::EncodeMove, co3::convert::EncodeMoveUnchecked, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(TransparentBox<(u8,)>: co3::convert::Encode, co3::convert::EncodeUnchecked, co3::convert::Decode<'static>);
    assert_impl_all!(TransparentBox<u8>: co3::convert::Encode, co3::convert::EncodeUnchecked, co3::convert::Decode<'static>);

    assert_impl_all!(RecordVec<(u8,)>: co3::convert::Encode, co3::convert::EncodeUnchecked, co3::convert::Decode<'static>);
    assert_impl_all!(RecordVec<u8>: co3::convert::Encode, co3::convert::EncodeUnchecked, co3::convert::Decode<'static>);
}

#[test]
fn non_robust_interior_references_are_direct_only() {
    assert_impl_all!(&UnsafeCell<NonZeroU8>: co3::convert::EncodeMoveUnchecked);
    assert_impl_all!(&UnsafeCell<NonZeroU8>: co3::convert::EncodeUnchecked);
    assert_not_impl_any!(&UnsafeCell<NonZeroU8>: co3::convert::EncodeMove);
    assert_impl_all!(&[UnsafeCell<NonZeroU8>]: co3::convert::EncodeMoveUnchecked);
    assert_not_impl_any!(&[UnsafeCell<NonZeroU8>]: co3::convert::EncodeMove);
    assert_impl_all!(Option<&UnsafeCell<NonZeroU8>>: co3::convert::EncodeMoveUnchecked);
    assert_impl_all!(Result<&UnsafeCell<NonZeroU8>, ()>: co3::convert::EncodeMoveUnchecked);
    assert_impl_all!((&UnsafeCell<NonZeroU8>, u8): co3::convert::EncodeMoveUnchecked);
    assert_impl_all!([&UnsafeCell<NonZeroU8>; 1]: co3::convert::EncodeMoveUnchecked);
    // TODO: Restore owned-container coverage after separating layout compatibility
    // from validation of shared interior-mutable contents.
    assert_not_impl_any!(Box<&UnsafeCell<NonZeroU8>>: co3::convert::EncodeMoveUnchecked);
    assert_not_impl_any!(Vec<&UnsafeCell<NonZeroU8>>: co3::convert::EncodeMoveUnchecked);
    assert_not_impl_any!(Vec<Option<&UnsafeCell<NonZeroU8>>>: co3::convert::EncodeMoveUnchecked);
    assert_impl_all!(Wrapped<'static>: co3::convert::EncodeMoveUnchecked);
    assert_impl_all!(Wrapped<'static>: co3::convert::EncodeUnchecked);
    assert_not_impl_any!(Wrapped<'static>: co3::convert::EncodeMove);
    assert_impl_all!(Box<(u8,)>: co3::convert::EncodeMoveUnchecked);
    assert_not_impl_any!(Box<(u8,)>: co3::convert::EncodeUnchecked);
    assert_impl_all!(ManuallyDrop<Box<(u8,)>>: co3::convert::EncodeMoveUnchecked, co3::convert::EncodeMove);
    assert_not_impl_any!(ManuallyDrop<Box<(u8,)>>: co3::convert::EncodeUnchecked, co3::convert::Encode, co3::convert::Decode<'static>);

    let value = UnsafeCell::new(NonZeroU8::new(7).unwrap());
    let encoded = unsafe { co3::encode_unchecked(&value) };
    assert_eq!(encoded.as_ptr().cast::<UnsafeCell<NonZeroU8>>(), &value);

    let _wrapped = unsafe { co3::encode_unchecked(Wrapped(&value)) };
    let _optional = unsafe { co3::encode_unchecked(Some(&value)) };
    let _result = unsafe { co3::encode_unchecked(Ok::<_, ()>(&value)) };
    let _tuple = unsafe { co3::encode_unchecked((&value, 1_u8)) };
    let _array = unsafe { co3::encode_unchecked([&value]) };

    let mut store = ();
    let encoded = unsafe { co3::soft_encode_unchecked(vec![UnsafeCell::new(7_u8)], &mut store) };
    assert_eq!(encoded.len(), 1);
    assert_eq!(store, ());
    drop(unsafe { encoded.into_vec() }.unwrap());

    let values = [UnsafeCell::new(NonZeroU8::new(8).unwrap())];
    let encoded = unsafe { co3::encode_unchecked(values.as_slice()) };
    assert_eq!(encoded.metadata(), 1);
}
