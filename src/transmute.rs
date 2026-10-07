#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use disjoint_impls::disjoint_impls;
use rust_spec::{RustSpec, Stable, mutability::Exclusive, niche::WithNiche, size::Zero};

#[cfg(feature = "alloc")]
use crate::boxed::CBox;
use crate::reference::CRefMut;
use crate::{CType, ReprC, Thin, assert_arr_has_non_zero_len};

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

    unsafe impl<R: CheckedTransmute<CType: Copy>, E> CheckedTransmute for Result<R, E>
    where
        R: RustSpec<
                Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>,
                Niche = WithNiche<Stable>,
            >,
        E: RustSpec<Size = rust_spec::size::Sized<Zero>, Alignment = rust_spec::One>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            unsafe { R::is_valid(target) }
        }
    }
    unsafe impl<R, E: CheckedTransmute<CType: Copy>> CheckedTransmute for Result<R, E>
    where
        R: RustSpec<Size = rust_spec::size::Sized<Zero>, Alignment = rust_spec::One>,
        E: RustSpec<
                Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>,
                Niche = WithNiche<Stable>,
            >,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            unsafe { E::is_valid(target) }
        }
    }
}

unsafe impl<R: CheckedTransmute + ?Sized> CheckedTransmute for &R
where
    R: RustSpec<Size: Thin, Mutability = Exclusive>,
    Self: ReprC<CType = *const R::CType>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        let ptr = *target;

        if ptr.is_null() {
            return false;
        }

        unsafe { R::is_valid(&*ptr) }
    }
}

// FIXME: Should it be implemented for non-robust R?
// atm we say yes, this is transmutable but don't misuse it.
// Either require `Stable<Robust>` or write this in the documentation
// If layout is Stable<Robust> then also consider how it affects Box<&mut R>
unsafe impl<R: CheckedTransmute + ?Sized> CheckedTransmute for &mut R
where
    R: RustSpec<Size: Thin>,
    Self: ReprC<CType = CRefMut<R::CType>>,
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

unsafe impl<C: CType + ?Sized, const RESTRICTED: bool> CheckedTransmute for CRefMut<C, RESTRICTED>
where
    C: RustSpec<Size: Thin>,
{
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}

#[cfg(feature = "alloc")]
unsafe impl<R: CheckedTransmute<CType: Sized>> CheckedTransmute for Box<R>
where
    R: RustSpec<Size: Thin, Mutability = Exclusive>,
    Self: ReprC<CType = CBox<<R as ReprC>::CType>>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        if target.is_niche() {
            return false;
        }

        unsafe { R::is_valid(&*target.data) }
    }
}

unsafe impl<R: CheckedTransmute<CType: Copy>, const N: usize> CheckedTransmute for [R; N] {
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

unsafe impl<R: CheckedTransmute<CType: Copy>> CheckedTransmute for [R] {
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
        core::str::from_utf8(target).is_ok()
    }
}

unsafe impl<R: CheckedTransmute<CType: Copy>> CheckedTransmute for Option<R>
where
    R: RustSpec<Niche = WithNiche<Stable>>,
    Self: ReprC<CType = R::CType>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { R::is_valid(target) }
    }
}

// TODO: Use this somehow to strengthen CheckedTransmute assumptions
//fn assert_size_and_allignment_match<R: CheckedTransmute<CType: Copy>>() {
//    const {
//        debug_assert!(core::mem::size_of::<R>() == core::mem::size_of::<R::CType>());
//        debug_assert!(core::mem::align_of::<R>() == core::mem::align_of::<R::CType>());
//    };
//}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::vec::Vec;

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::CBoxedSlice;
    use crate::{
        CType, Decode, Encode,
        niche::Niche,
        slice::{CSlice, CSliceMut},
    };

    #[test]
    fn transparent_type() {
        assert_impl_all!(bool:
            Niche<CType = crate::primitives::CBool>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&bool:
            Niche<CType = *const crate::primitives::CBool>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut bool:
            Niche<CType = CRefMut<crate::primitives::CBool>>,
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
            Niche<CType = CSlice<crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [bool]:
            Niche<CType = CSliceMut<crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[bool]>:
            Niche<CType = CBoxedSlice<crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<bool>:
            Niche<CType = CBoxedSlice<crate::primitives::CBool>>,
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
            Niche<CType = *const *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &u8:
            Niche<CType = CRefMut<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            Niche<CType = CBox<*const crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&u8]:
            Niche<CType = CSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&u8]:
            Niche<CType = CSliceMut<*const u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&u8]>:
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&u8>:
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&u8; 2]:
            Niche<CType = [*const u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&u8>:
            ReprC<CType = *const u8>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(Option<&u8>: CType);
    }

    #[test]
    fn transparent_ref() {
        assert_impl_all!(&&bool:
            Niche<CType = *const *const crate::primitives::CBool>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &bool:
            Niche<CType = CRefMut<*const crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            Niche<CType = CBox<*const crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&bool]:
            Niche<CType = CSlice<*const crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [&bool]:
            Niche<CType = CSliceMut<*const crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&bool]>:
            Niche<CType = CBoxedSlice<*const crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&bool>:
            Niche<CType = CBoxedSlice<*const crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&bool; 2]:
            Niche<CType = [*const crate::primitives::CBool; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&bool>:
            ReprC<CType = *const crate::primitives::CBool>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn robust_ref_mut() {
        assert_impl_all!(&&mut u8:
            Niche<CType = *const CRefMut<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &mut u8:
            Niche<CType = CRefMut<CRefMut<u8>>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut u8>:
            Niche<CType = CBox<CRefMut<u8>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&mut u8]:
            Niche<CType = CSlice<CRefMut<u8>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut u8]:
            Niche<CType = CSliceMut<CRefMut<u8>>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut u8]>:
            Niche<CType = CBoxedSlice<CRefMut<u8>>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut u8>:
            Niche<CType = CBoxedSlice<CRefMut<u8>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&mut u8; 2]:
            Niche<CType = [CRefMut<u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&mut u8>:
            ReprC<CType = CRefMut<u8>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(Option<&mut u8>: CType);
    }

    #[test]
    fn transparent_ref_mut() {
        assert_impl_all!(&&mut bool:
            Niche<CType = *const CRefMut<crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &mut bool:
            Niche<CType = CRefMut<CRefMut<crate::primitives::CBool>>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut bool>:
            Niche<CType = CBox<CRefMut<crate::primitives::CBool>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&mut bool]:
            Niche<CType = CSlice<CRefMut<crate::primitives::CBool>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut bool]:
            Niche<CType = CSliceMut<CRefMut<crate::primitives::CBool>>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut bool]>:
            Niche<CType = CBoxedSlice<CRefMut<crate::primitives::CBool>>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut bool>:
            Niche<CType = CBoxedSlice<CRefMut<crate::primitives::CBool>>>,
            Decode<'static>,
            Encode
        );
        assert_impl_all!([&mut bool; 2]:
            Niche<CType = [CRefMut<crate::primitives::CBool>; 2]>,
            Decode<'static>,
            Encode
        );
        assert_impl_all!(Option<&mut bool>:
            ReprC<CType = CRefMut<crate::primitives::CBool>>,
            Decode<'static>,
            Encode
        );
    }
}
