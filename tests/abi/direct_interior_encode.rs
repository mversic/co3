use core::{cell::UnsafeCell, mem::ManuallyDrop, num::NonZeroU8};

use co3::stored::{DecodeOwned, EncodeMove, EncodeMoveUnchecked};
use co3::{Decode, Encode, EncodeUnchecked};
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(transparent)]
struct Wrapped<'a>(&'a UnsafeCell<NonZeroU8>);

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(transparent)]
#[repr_c(transparent)]
struct TransparentVec<T>(Vec<T>);

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(transparent)]
#[repr_c(transparent)]
struct ConcreteTransparentVec(Vec<(u8,)>);

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(transparent)]
#[repr_c(transparent)]
struct ConcreteTransparentByte(u8);

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(transparent)]
#[repr_c(transparent)]
struct TransparentBox<T>(Box<T>);

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(C)]
struct RecordVec<T> {
    value: Vec<T>,
}

#[test]
fn transparent_wrappers_preserve_owned_marker_restrictions() {
    assert_impl_all!(TransparentVec<(u8,)>: EncodeMove, EncodeMoveUnchecked, DecodeOwned<'static>);
    assert_not_impl_any!(TransparentVec<(u8,)>: Encode, EncodeUnchecked, Decode<'static>);
    assert_impl_all!(TransparentVec<u8>: Encode, EncodeUnchecked, Decode<'static>);
    assert_impl_all!(ConcreteTransparentVec: EncodeMove, EncodeMoveUnchecked, DecodeOwned<'static>);
    assert_not_impl_any!(ConcreteTransparentVec: Encode, EncodeUnchecked, Decode<'static>);
    assert_impl_all!(ConcreteTransparentByte: co3::transmute::CheckedTransmute);

    assert_impl_all!(TransparentBox<(u8,)>: EncodeMove, EncodeMoveUnchecked, DecodeOwned<'static>);
    assert_not_impl_any!(TransparentBox<(u8,)>: Encode, EncodeUnchecked, Decode<'static>);
    assert_impl_all!(TransparentBox<u8>: Encode, EncodeUnchecked, Decode<'static>);

    assert_not_impl_any!(RecordVec<(u8,)>: Encode, EncodeUnchecked, Decode<'static>);
    assert_impl_all!(RecordVec<u8>: Encode, EncodeUnchecked, Decode<'static>);
}

#[test]
fn non_robust_interior_references_are_direct_only() {
    assert_impl_all!(&UnsafeCell<NonZeroU8>: EncodeMoveUnchecked);
    assert_impl_all!(&UnsafeCell<NonZeroU8>: EncodeUnchecked);
    assert_not_impl_any!(&UnsafeCell<NonZeroU8>: EncodeMove);
    assert_impl_all!(&[UnsafeCell<NonZeroU8>]: EncodeMoveUnchecked);
    assert_not_impl_any!(&[UnsafeCell<NonZeroU8>]: EncodeMove);
    assert_impl_all!(Option<&UnsafeCell<NonZeroU8>>: EncodeMoveUnchecked);
    assert_impl_all!(Result<&UnsafeCell<NonZeroU8>, ()>: EncodeMoveUnchecked);
    assert_impl_all!((&UnsafeCell<NonZeroU8>, u8): EncodeMoveUnchecked);
    assert_impl_all!([&UnsafeCell<NonZeroU8>; 1]: EncodeMoveUnchecked);
    assert_impl_all!(Box<&UnsafeCell<NonZeroU8>>: EncodeMoveUnchecked);
    assert_impl_all!(Vec<&UnsafeCell<NonZeroU8>>: EncodeMoveUnchecked);
    assert_impl_all!(Vec<Option<&UnsafeCell<NonZeroU8>>>: EncodeMoveUnchecked);
    assert_impl_all!(Wrapped<'static>: EncodeMoveUnchecked);
    assert_impl_all!(Wrapped<'static>: EncodeUnchecked);
    assert_not_impl_any!(Wrapped<'static>: EncodeMove);
    assert_impl_all!(Box<(u8,)>: EncodeMoveUnchecked);
    assert_not_impl_any!(Box<(u8,)>: EncodeUnchecked);
    assert_impl_all!(ManuallyDrop<Box<(u8,)>>: EncodeMoveUnchecked, EncodeMove);
    assert_not_impl_any!(ManuallyDrop<Box<(u8,)>>: EncodeUnchecked, Encode, Decode<'static>);

    let value = UnsafeCell::new(NonZeroU8::new(7).unwrap());
    let encoded = unsafe { co3::encode_unchecked(&value) };
    assert_eq!(
        encoded
            .as_ptr()
            .cast_const()
            .cast::<UnsafeCell<NonZeroU8>>(),
        &value
    );

    let _wrapped = unsafe { co3::encode_unchecked(Wrapped(&value)) };
    let _optional = unsafe { co3::encode_unchecked(Some(&value)) };
    let _result = unsafe { co3::encode_unchecked(Ok::<_, ()>(&value)) };
    let _tuple = unsafe { co3::encode_unchecked((&value, 1_u8)) };
    let _array = unsafe { co3::encode_unchecked([&value]) };
    let _box = unsafe { co3::encode_unchecked(Box::new(&value)) };
    let _vec = unsafe { co3::encode_unchecked(vec![&value]) };

    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(vec![UnsafeCell::new(7_u8)], &mut store) };
    assert_eq!(encoded.metadata(), 1);
    assert_eq!(store.len(), 1);

    let values = [UnsafeCell::new(NonZeroU8::new(8).unwrap())];
    let encoded = unsafe { co3::encode_unchecked(values.as_slice()) };
    assert_eq!(encoded.metadata(), 1);
}
