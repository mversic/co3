#[cfg(feature = "alloc")]
use alloc::boxed::Box;
use core::mem::ManuallyDrop;

use disjoint_impls::disjoint_impls;
use rust_spec::{
    RustSpec, Stable,
    drop::NoDrop,
    layout::Robust,
    mutability::{Exclusive, WithInterior},
    niche::WithNiche,
    size::Zero,
};

#[cfg(feature = "alloc")]
use crate::boxed::CBox;
use crate::reference::{CRef, CRefMut};
use crate::{ReprC, Thin, WithDrop, assert_arr_has_non_zero_len, utf8::Utf8Str};

disjoint_impls! {
    /// Type that can be **safely transmuted** into its C representation.
    ///
    /// # Safety
    ///
    /// - `Self` and `Self::CType` must have mutually transmutable layouts and value representations.
    /// - `Self::is_valid` must return `false` for trap representations and `true` for valid representations
    pub unsafe trait CheckedTransmute: ReprC {
        /// Called when transmuting an [`ReprC::CType`] back into [`Self`] to check for trap representations.
        ///
        /// Return `true` only when `target` is a valid representation of `Self`.
        ///
        /// # Safety
        ///
        /// Any pointers dereferenced while checking `target` must be valid for reads.
        unsafe fn is_valid(target: &Self::CType) -> bool;
    }

    unsafe impl<R: CheckedTransmute + ?Sized> CheckedTransmute for &R
    where
        R: rust_spec::RustSpec<Size: Thin, Mutability = Exclusive>,
        Self: ReprC<CType = CRef<<R as ReprC>::CType>>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            let ptr = target.as_ptr();

            if ptr.is_null() {
                return false;
            }

            unsafe { R::is_valid(&*ptr) }
        }
    }
    unsafe impl<R: CheckedTransmute + ?Sized> CheckedTransmute for &R
    where
        R: RustSpec<Size: Thin, Mutability = WithInterior, Trap = Robust>,
        Self: ReprC<CType = CRefMut<<R as ReprC>::CType, false>>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            !target.as_ptr().is_null()
        }
    }

    unsafe impl<R: CheckedTransmute<CType: Copy> + ReprC, E: ReprC> CheckedTransmute for Result<R, E>
    where
        R: rust_spec::RustSpec<
                Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>,
                Niche = WithNiche<Stable>,
            >,
        E: rust_spec::RustSpec<Size = rust_spec::size::Sized<Zero>, Alignment = rust_spec::One>,
        Self: ReprC<CType = <R as ReprC>::CType>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            unsafe { R::is_valid(target) }
        }
    }
    unsafe impl<R: ReprC, E: CheckedTransmute<CType: Copy>> CheckedTransmute for Result<R, E>
    where
        R: rust_spec::RustSpec<Size = rust_spec::size::Sized<Zero>, Alignment = rust_spec::One>,
        E: rust_spec::RustSpec<
                Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>,
                Niche = WithNiche<Stable>,
            >,
        Self: ReprC<CType = <E as ReprC>::CType>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            unsafe { E::is_valid(target) }
        }
    }

    unsafe impl<R: CheckedTransmute + ?Sized> CheckedTransmute for ManuallyDrop<R>
    where
        R: RustSpec<Drop = NoDrop>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            unsafe { R::is_valid(target) }
        }
    }

    unsafe impl<R: CheckedTransmute + ?Sized> CheckedTransmute for ManuallyDrop<R>
    where
        R: RustSpec<Drop: WithDrop>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            unsafe { R::is_valid(target) }
        }
    }
}

unsafe impl<R: CheckedTransmute + ?Sized, const EXCLUSIVE: bool> CheckedTransmute for &mut R
where
    R: rust_spec::RustSpec<Size: Thin>,
    Self: ReprC<CType = CRefMut<R::CType, EXCLUSIVE>>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        let ptr = target.as_ptr();

        if ptr.is_null() {
            return false;
        }

        unsafe { R::is_valid(&*ptr) }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<R: CheckedTransmute<CType: Sized>> CheckedTransmute for Box<R>
where
    R: rust_spec::RustSpec<Size: Thin>,
    Self: ReprC<CType = CBox<<R as ReprC>::CType>>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        if target.is_null() {
            return false;
        }

        unsafe { R::is_valid(&*target.as_ptr()) }
    }
}

unsafe impl<R: CheckedTransmute<CType: Sized>, const N: usize> CheckedTransmute for [R; N] {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        assert_arr_has_non_zero_len::<N>();

        for t in target {
            if unsafe { !R::is_valid(t) } {
                return false;
            }
        }

        true
    }
}

unsafe impl<R: CheckedTransmute<CType: Sized>> CheckedTransmute for [R] {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        for item in target {
            if unsafe { !R::is_valid(item) } {
                return false;
            }
        }

        true
    }
}

unsafe impl CheckedTransmute for str {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        let ptr = core::ptr::from_ref(target);
        let len = <Utf8Str as crate::wide::Wide>::metadata(ptr);
        let bytes = unsafe { core::slice::from_raw_parts(ptr.cast::<u8>(), len) };
        core::str::from_utf8(bytes).is_ok()
    }
}

unsafe impl<R: CheckedTransmute<CType: Sized>> CheckedTransmute for Option<R>
where
    R: rust_spec::RustSpec<Niche = WithNiche<Stable>>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { R::is_valid(target) }
    }
}

// TODO: Use this somehow to strengthen CheckedTransmute assumptions
//fn assert_size_and_allignment_match<R: CheckedTransmute<CType: Sized>>() {
//    const {
//        debug_assert!(core::mem::size_of::<R>() == core::mem::size_of::<R::CType>());
//        debug_assert!(core::mem::align_of::<R>() == core::mem::align_of::<R::CType>());
//    };
//}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::vec::Vec;

    use static_assertions::assert_impl_all;

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::CWideBox;
    use crate::{
        CType, Decode, Encode,
        niche::Niche,
        slice::{CWideRef, CWideRefMut},
    };

    #[test]
    fn manually_drop_validates_inner_representation() {
        let invalid_bool = <bool as Niche>::NICHE;
        assert!(!unsafe { <ManuallyDrop<bool> as CheckedTransmute>::is_valid(&invalid_bool) });

        #[cfg(feature = "alloc")]
        {
            let null_box = ManuallyDrop::new(CBox::<u8>::null());
            assert!(!unsafe { <ManuallyDrop<Box<u8>> as CheckedTransmute>::is_valid(&null_box) });
        }
    }

    #[test]
    fn robust_shared_interior_reference_checks_only_the_pointer() {
        use core::cell::UnsafeCell;

        let cell = UnsafeCell::new(7_u8);
        let encoded = crate::encode(&cell);
        assert_eq!(encoded.as_ptr(), core::ptr::from_ref(&cell).cast_mut());
        assert!(unsafe { <&UnsafeCell<u8> as CheckedTransmute>::is_valid(&encoded) });

        let null = CRefMut::<UnsafeCell<u8>, false>::null();
        assert!(!unsafe { <&UnsafeCell<u8> as CheckedTransmute>::is_valid(&null) });
    }

    #[test]
    fn transparent_type() {
        assert_impl_all!(bool:
            Niche<CType = crate::primitives::CBool>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&bool:
            Niche<CType = CRef<crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut bool:
            Niche<CType = CRefMut<crate::primitives::CBool, true>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<bool>:
            Niche<CType = CBox<crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[bool]:
            Niche<CType = CWideRef<[crate::primitives::CBool]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [bool]:
            Niche<CType = CWideRefMut<[crate::primitives::CBool], true>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[bool]>:
            Niche<CType = CWideBox<[crate::primitives::CBool]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<bool>:
            Niche<CType = CWideBox<[crate::primitives::CBool]>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([bool; 2]:
            Niche<CType = [crate::primitives::CBool; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<bool>:
            Niche<CType = crate::primitives::CBool>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn robust_ref() {
        assert_impl_all!(&&u8:
            Niche<CType = CRef<CRef<u8>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &u8:
            Niche<CType = CRefMut<CRef<u8>, true>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            Niche<CType = CBox<CRef<crate::primitives::CBool>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&u8]:
            Niche<CType = CWideRef<[CRef<u8>]>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&u8]:
            Niche<CType = CWideRefMut<[CRef<u8>], true>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&u8]>:
            Niche<CType = CWideBox<[CRef<u8>]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&u8>:
            Niche<CType = CWideBox<[CRef<u8>]>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&u8; 2]:
            Niche<CType = [CRef<u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&u8>:
            ReprC<CType = CRef<u8>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(Option<&u8>: CType);
    }

    #[test]
    fn transparent_ref() {
        assert_impl_all!(&&bool:
            Niche<CType = CRef<CRef<crate::primitives::CBool>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &bool:
            Niche<CType = CRefMut<CRef<crate::primitives::CBool>, true>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            Niche<CType = CBox<CRef<crate::primitives::CBool>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&bool]:
            Niche<CType = CWideRef<[CRef<crate::primitives::CBool>]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [&bool]:
            Niche<CType = CWideRefMut<[CRef<crate::primitives::CBool>], true>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&bool]>:
            Niche<CType = CWideBox<[CRef<crate::primitives::CBool>]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&bool>:
            Niche<CType = CWideBox<[CRef<crate::primitives::CBool>]>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&bool; 2]:
            Niche<CType = [CRef<crate::primitives::CBool>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&bool>:
            ReprC<CType = CRef<crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn robust_ref_mut() {
        assert_impl_all!(&&mut u8:
            Niche<CType = CRef<CRefMut<u8, true>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &mut u8:
            Niche<CType = CRefMut<CRefMut<u8, true>, true>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut u8>:
            Niche<CType = CBox<CRefMut<u8, true>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&mut u8]:
            Niche<CType = CWideRef<[CRefMut<u8, true>]>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut u8]:
            Niche<CType = CWideRefMut<[CRefMut<u8, true>], true>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut u8]>:
            Niche<CType = CWideBox<[CRefMut<u8, true>]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut u8>:
            Niche<CType = CWideBox<[CRefMut<u8, true>]>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&mut u8; 2]:
            Niche<CType = [CRefMut<u8, true>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&mut u8>:
            ReprC<CType = CRefMut<u8, true>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(Option<&mut u8>: CType);
    }

    #[test]
    fn transparent_ref_mut() {
        assert_impl_all!(&&mut bool:
            Niche<CType = CRef<CRefMut<crate::primitives::CBool, true>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &mut bool:
            Niche<CType = CRefMut<CRefMut<crate::primitives::CBool, true>, true>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut bool>:
            Niche<CType = CBox<CRefMut<crate::primitives::CBool, true>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&mut bool]:
            Niche<CType = CWideRef<[CRefMut<crate::primitives::CBool, true>]>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut bool]:
            Niche<CType = CWideRefMut<[CRefMut<crate::primitives::CBool, true>], true>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut bool]>:
            Niche<CType = CWideBox<[CRefMut<crate::primitives::CBool, true>]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut bool>:
            Niche<CType = CWideBox<[CRefMut<crate::primitives::CBool, true>]>>,
            Decode<'static>,
            Encode
        );
        assert_impl_all!([&mut bool; 2]:
            Niche<CType = [CRefMut<crate::primitives::CBool, true>; 2]>,
            Decode<'static>,
            Encode
        );
        assert_impl_all!(Option<&mut bool>:
            ReprC<CType = CRefMut<crate::primitives::CBool, true>>,
            Decode<'static>,
            Encode
        );
    }
}
