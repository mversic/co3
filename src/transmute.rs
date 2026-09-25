#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use disjoint_impls::disjoint_impls;
use rust_spec::{RustSpec, Stable, mutability::Exclusive, niche::WithNiche, size::Zero};

#[cfg(feature = "alloc")]
use crate::boxed::CBox;
use crate::{ReprC, assert_arr_has_non_zero_len};

disjoint_impls! {
    /// Type that can be **safely transmuted** into its C representation.
    ///
    /// # Safety
    ///
    /// - `Self` and `Self::CType` must be mutually transmutable (this includes [`Drop`] semantics)
    /// - `Self::is_valid` must not return false negatives, i.e. return `true` for trap representations
    pub unsafe trait CheckedTransmute: ReprC {
        /// Called when transmuting an [`ReprC::CType`] back into [`Self`] to check for trap representations.
        ///
        /// This function must never return false negatives, i.e. return `true` for a trap representation.
        ///
        /// # Safety
        ///
        /// pointers in `Self::Target` must be valid for reads
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

// NOTE: It is UB to transmute between `UnsafeCell<T>` and `T`
unsafe impl<R: CheckedTransmute + RustSpec<Mutability = Exclusive> + ?Sized> CheckedTransmute for &R
where
    Self: ReprC<CType: Copy>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        let ptr = unsafe { core::mem::transmute_copy::<Self::CType, *const R::CType>(target) };

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
    Self: ReprC<CType = *mut R::CType>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        if target.is_null() {
            return false;
        }

        unsafe { R::is_valid(&**target) }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<R: CheckedTransmute<CType: Sized>> CheckedTransmute for Box<R>
where
    Self: ReprC<CType = CBox<R::CType>>,
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
            Niche<CType = u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&bool:
            Niche<CType = *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut bool:
            Niche<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<bool>:
            Niche<CType = CBox<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[bool]:
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [bool]:
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[bool]>:
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<bool>:
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([bool; 2]:
            Niche<CType = [u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<bool>:
            Niche<CType = u8>,
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
            Niche<CType = *mut *const u8>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            Niche<CType = CBox<*const u8>>,
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
            Niche<CType = *const *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &bool:
            Niche<CType = *mut *const u8>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            Niche<CType = CBox<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&bool]:
            Niche<CType = CSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [&bool]:
            Niche<CType = CSliceMut<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&bool]>:
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&bool>:
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&bool; 2]:
            Niche<CType = [*const u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&bool>:
            ReprC<CType = *const u8>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn robust_ref_mut() {
        assert_impl_all!(&&mut u8:
            Niche<CType = *const *mut u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &mut u8:
            Niche<CType = *mut *mut u8>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut u8>:
            Niche<CType = CBox<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&mut u8]:
            Niche<CType = CSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut u8]:
            Niche<CType = CSliceMut<*mut u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut u8]>:
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut u8>:
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&mut u8; 2]:
            Niche<CType = [*mut u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&mut u8>:
            ReprC<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(Option<&mut u8>: CType);
    }

    #[test]
    fn transparent_ref_mut() {
        assert_impl_all!(&&mut bool:
            Niche<CType = *const *mut u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &mut bool:
            Niche<CType = *mut *mut u8>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut bool>:
            Niche<CType = CBox<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&mut bool]:
            Niche<CType = CSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut bool]:
            Niche<CType = CSliceMut<*mut u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut bool]>:
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut bool>:
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            Encode
        );
        assert_impl_all!([&mut bool; 2]:
            Niche<CType = [*mut u8; 2]>,
            Decode<'static>,
            Encode
        );
        assert_impl_all!(Option<&mut bool>:
            ReprC<CType = *mut u8>,
            Decode<'static>,
            Encode
        );
    }
}
