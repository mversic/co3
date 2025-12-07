#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
use alloc::{boxed::Box, string::String, vec::Vec};

#[cfg(feature = "owned_as_ref")]
#[cfg(feature = "owned_types")]
use crate::slice::RefMutSlice;
use crate::{mineral, slice::RefSlice};

macro_rules! non_zero_derive {
    ($($ty:ty => $target:ty),+ $(,)?) => {$(
        mineral! {
            unsafe impl Transparent for $ty {
                type Target = $target;

                const NICHE_VALUE: Self::CType = 0;
                fn is_valid(target: &Self::Target) -> bool {
                    *target != <Self as crate::niche::Niche>::NICHE_VALUE
                }
            }
        }

        unsafe impl crate::niche::StableNiche for $ty {})+
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

#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
// WARN: This can be contested as it is nowhere documented that String is
// actually transmutable into Vec<u8>, but implicitly it should be
mineral! {
    unsafe impl Transparent for String {
        type Target = Vec<u8>;

        const NICHE_VALUE: Self::CType = RefMutSlice::null_mut();
        fn is_valid(target: &Self::Target) -> bool {
            core::str::from_utf8(target).is_ok()
        }
    }
}

#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
// WARN: `core::str::as_bytes` uses transmute internally which means that
// even though it's a string slice it can be transmuted into byte slice.
mineral! {
    unsafe impl Transparent for Box<str> {
        type Target = Box<[u8]>;

        const NICHE_VALUE: Self::CType = RefMutSlice::null_mut();
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

#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
mineral! {
    unsafe impl<T> Transparent for core::mem::ManuallyDrop<T> {
        type Target = T;
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

impl<T> crate::ir::Ir for core::cell::UnsafeCell<T> {
    type Type = crate::ir::Transparent;
}

// SAFETY: `UnsafeCell<T>` is transmutable into `T`
// and `is_valid` doesn't return false positives
unsafe impl<T> crate::transmute::Transmute for core::cell::UnsafeCell<T> {
    type Target = T;

    #[inline(always)]
    fn is_valid(_: &Self::Target) -> bool {
        true
    }
}

// SAFETY: `UnsafeCell<T>` is robust with respect to `T`
unsafe impl<T> crate::transmute::InfallibleTransmute for core::cell::UnsafeCell<T> {}

impl<T> crate::niche::Ir for core::cell::UnsafeCell<T> {
    type Type = crate::ir::Robust;
}

// SAFETY: ZST relation is transitive
unsafe impl<T> crate::out_ptr::Zst for core::cell::UnsafeCell<T> where
    for<'dummy> <Self as crate::transmute::Transmute>::Target: crate::out_ptr::Zst
{
}
