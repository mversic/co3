use alloc::{boxed::Box, string::String, vec::Vec};
use core::{mem::ManuallyDrop, ptr::NonNull};

use crate::{ReprC, WrapperTypeOf, mineral, option::Niche as _, slice::RefSlice};

macro_rules! non_zero_derive {
    ($($ty:ty => $target:ty),+ $(,)?) => {$(
        mineral! {
            unsafe impl Transparent for $ty {
                type Target = $target;

                const NICHE_VALUE: Self::CType = 0;
                fn is_valid(target: &Self::Target) -> bool {
                    *target != Self::NICHE_VALUE
                }
            }
        })+
    }
}

non_zero_derive! {
    core::num::NonZeroU8 => u8,
    core::num::NonZeroI8 => i8,
    core::num::NonZeroU16 => u16,
    core::num::NonZeroI16 => i16,
    core::num::NonZeroU32 => u32,
    core::num::NonZeroI32 => i32,
    core::num::NonZeroU64 => u64,
    core::num::NonZeroI64 => i64,
    core::num::NonZeroU128 => u128,
    core::num::NonZeroI128 => i128,
}

// WARN: This can be contested as it is nowhere documented that String is
// actually transmutable into Vec<u8>, but implicitly it should be
// SAFETY: String type should be transmutable into Vec<u8>
mineral! {
    unsafe impl Transparent for String {
        type Target = Vec<u8>;

        const NICHE_VALUE: Self::CType = RefSlice::null();
        fn is_valid(target: &Self::Target) -> bool {
            core::str::from_utf8(target).is_ok()
        }
    }
}
// WARN: `core::str::as_bytes` uses transmute internally which means that
// even though it's a string slice it can be transmuted into byte slice.
mineral! {
    unsafe impl Transparent for Box<str> {
        type Target = Box<[u8]>;

        const NICHE_VALUE: Self::CType = RefSlice::null();
        fn is_valid(target: &Self::Target) -> bool {
            core::str::from_utf8(target).is_ok()
        }
    }
}
mineral! {
    unsafe impl<'slice> Transparent for &'slice str {
        type Target = &'slice [u8];

        const NICHE_VALUE: Self::CType = RefSlice::null();
        fn is_valid(target: &Self::Target) -> bool {
            core::str::from_utf8(target).is_ok()
        }
    }
}
#[cfg(feature = "non_robust_ref_mut")]
mineral! {
    unsafe impl<'slice> Transparent for &'slice mut str {
        type Target = &'slice mut [u8];

        const NICHE_VALUE: Self::CType = crate::slice::RefMutSlice::null_mut();
        fn is_valid(target: &Self::Target) -> bool {
            core::str::from_utf8(target).is_ok()
        }
    }
}
mineral! {
    unsafe impl<T> Transparent for core::mem::ManuallyDrop<T> {
        type Target = T;
        const NICHE_VALUE = "DELEGATE";
    }
}
mineral! {
    unsafe impl<T> Transparent for core::ptr::NonNull<T> {
        type Target = *mut T;

        const NICHE_VALUE: Self::CType = core::ptr::null_mut();
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
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
