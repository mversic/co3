use alloc::{boxed::Box, string::String, vec::Vec};
use core::{mem::ManuallyDrop, ptr::NonNull};

use crate::{
    ReprC, WrapperTypeOf, mineral,
    slice::{RefMutSlice, RefSlice},
};

// WARN: This can be contested as it is nowhere documented that String is
// actually transmutable into Vec<u8>, but implicitly it should be
// SAFETY: String type should be transmutable into Vec<u8>
mineral! {
    unsafe impl Transparent for String {
        type Target = Vec<u8>;

        validation_fn={|target| core::str::from_utf8(target).is_ok()},
        niche_value=RefMutSlice::null_mut()
    }
}
// WARN: `core::str::as_bytes` uses transmute internally which means that
// even though it's a string slice it can be transmuted into byte slice.
mineral! {
    unsafe impl Transparent for Box<str> {
        type Target = Box<[u8]>;

        validation_fn={|target| core::str::from_utf8(target).is_ok()},
        niche_value=RefMutSlice::null_mut()
    }
}
mineral! {
    unsafe impl<'slice> Transparent for &'slice str {
        type Target = &'slice [u8];

        validation_fn={|target| core::str::from_utf8(target).is_ok()},
        niche_value=RefSlice::null()
    }
}
#[cfg(feature = "non_robust_ref_mut")]
mineral! {
    unsafe impl<'slice> Transparent for &'slice mut str {
        type Target = &'slice mut [u8];

        validation_fn={|target| core::str::from_utf8(target).is_ok()},
        niche_value=RefMutSlice::null_mut()
    }
}
mineral! {
    unsafe impl<T> Transparent for core::mem::ManuallyDrop<T> {
        type Target = T;
    }
}
mineral! {
    unsafe impl<T> Transparent for core::ptr::NonNull<T> {
        type Target = *mut T;

        validation_fn={|target: &Self::Target| !target.is_null()},
        niche_value=core::ptr::null_mut()
    }
}

impl<T> WrapperTypeOf<NonNull<T>> for *mut T {
    type Type = NonNull<T>;
}
impl WrapperTypeOf<String> for Vec<u8> {
    type Type = String;
}

// SAFETY: Type is `ReprC` if the inner type is
unsafe impl<T: ReprC> ReprC for ManuallyDrop<T> {}
