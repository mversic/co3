use core::{cell::UnsafeCell, ptr::NonNull};

use co3::{ReprC, lang, marker::AbiCompatible, ops::CFn1};
use static_assertions::{assert_impl_all, assert_not_impl_any, assert_type_eq_all};

assert_type_eq_all!(<*const bool as ReprC>::CType, *const <bool as ReprC>::CType);
assert_type_eq_all!(<*mut bool as ReprC>::CType, *mut <bool as ReprC>::CType);
assert_type_eq_all!(
    <*const [bool] as ReprC>::CType,
    *const [<bool as ReprC>::CType]
);
assert_type_eq_all!(
    <NonNull<bool> as ReprC>::CType,
    co3::marker::NonZero<*mut <bool as ReprC>::CType>
);
assert_type_eq_all!(
    <*const String as ReprC>::CType,
    *const <String as ReprC>::CType
);
assert_not_impl_any!(*const String: co3::convert::Encode, co3::convert::EncodeUnchecked, co3::convert::Decode<'static>, AbiCompatible);
assert_not_impl_any!(*mut String: co3::convert::Encode, co3::convert::EncodeUnchecked, co3::convert::Decode<'static>, AbiCompatible);
assert_not_impl_any!(NonNull<String>: co3::convert::Encode, co3::convert::Decode<'static>);
assert_type_eq_all!(
    <*const core::ffi::CStr as ReprC>::CType,
    *const co3::ffi::c_char
);
assert_not_impl_any!(*const core::ffi::CStr: co3::convert::Encode, co3::convert::Decode<'static>);
assert_impl_all!(*const [bool]: co3::convert::Encode, co3::convert::Decode<'static>, AbiCompatible);
assert_impl_all!(*mut [bool]: co3::convert::Encode, co3::convert::Decode<'static>, AbiCompatible);

fn identity_const(value: *const bool) -> *const bool {
    value
}
fn identity_mut(value: *mut bool) -> *mut bool {
    value
}
co3::raw! {
    fn identity_const(value: *const bool) -> move *const bool;
    fn identity_mut(value: *mut bool) -> move *mut bool;
}

#[test]
fn raw_pointer_companions_lower_pointees_without_moving_them() {
    let mut value = true;
    let source = core::ptr::from_mut(&mut value);
    let encoded: *mut <bool as ReprC>::CType = unsafe { co3::encode::<_, lang::C>(source) };
    assert_eq!(encoded.cast::<bool>(), source);
    let callback: unsafe extern "C" fn(_) -> _ = identity_mut_raw;
    let output: Option<*mut bool> = unsafe { callback.call(source) };
    assert_eq!(output, Some(source));
    let callback: unsafe extern "C" fn(_) -> _ = identity_const_raw;
    let output: Option<*const bool> = unsafe { callback.call(source.cast_const()) };
    assert_eq!(output, Some(source.cast_const()));
    let nonnull = NonNull::new(source).unwrap();
    let encoded = unsafe { co3::encode::<_, lang::C>(nonnull) };
    assert_eq!(encoded.0.cast::<bool>(), source);
    assert_eq!(
        unsafe { co3::decode::<NonNull<bool>, lang::C>(encoded) },
        Some(nonnull)
    );
    assert!(
        unsafe {
            co3::decode::<NonNull<bool>, lang::C>(co3::marker::NonZero(core::ptr::null_mut()))
        }
        .is_none()
    );
}

#[test]
fn wide_pointer_conversion_preserves_address_and_length() {
    let mut values = [true, false, true];
    let source = core::ptr::slice_from_raw_parts_mut(values.as_mut_ptr(), values.len());
    let encoded: *mut [<bool as ReprC>::CType] = unsafe { co3::encode::<_, lang::C>(source) };
    assert_eq!(encoded.cast::<bool>(), source.cast::<bool>());
    assert_eq!(encoded.len(), source.len());
    let decoded = unsafe { co3::decode::<*mut [bool], lang::C>(encoded) }.unwrap();
    assert!(core::ptr::eq(decoded, source));
    let encoded: *const [<bool as ReprC>::CType] =
        unsafe { co3::encode::<_, lang::C>(source.cast_const()) };
    let decoded = unsafe { co3::decode::<*const [bool], lang::C>(encoded) }.unwrap();
    assert!(core::ptr::eq(decoded, source.cast_const()));
}

#[test]
fn raw_pointer_conversion_does_not_validate_or_read_the_pointee() {
    let bytes = UnsafeCell::new(255_u8);
    let source = bytes.get().cast::<bool>();
    let encoded = unsafe { co3::encode::<_, lang::C>(source) };
    assert_eq!(encoded.cast::<u8>(), bytes.get());
    assert_eq!(
        unsafe { co3::decode::<*mut bool, lang::C>(encoded) },
        Some(source)
    );
    let null = core::ptr::slice_from_raw_parts(core::ptr::null::<bool>(), 17);
    let encoded: *const [<bool as ReprC>::CType] = unsafe { co3::encode::<_, lang::C>(null) };
    assert!(encoded.is_null());
    assert_eq!(encoded.len(), 17);
    let decoded = unsafe { co3::decode::<*const [bool], lang::C>(encoded) }.unwrap();
    assert!(core::ptr::eq(decoded, null));
}
