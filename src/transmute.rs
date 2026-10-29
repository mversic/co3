#[cfg(feature = "alloc")]
use alloc::boxed::Box;
use core::{
    cell::{Cell, UnsafeCell},
    mem::ManuallyDrop,
};

use disjoint_impls::disjoint_impls;
use rust_spec::{
    Gt, RustSpec, Stable,
    drop::NoDrop,
    layout::{NonRobust, Robust},
    mutability::{Exclusive, ExclusiveKind, WithInterior, WithoutInterior},
    size::{Sized as RustSpecSized, Zero},
};

#[cfg(feature = "alloc")]
use crate::boxed::{CBox};
use crate::{
    NeedsDrop, ReprC, assert_arr_has_non_zero_len,
    niche::Niche,
    reference::{CRef, CRefMut},
    utf8::Utf8Str,
};
#[doc(hidden)]
pub use interior::InteriorTrap;

// The trait is public inside a private module so associated-type projections
// in conversion impls are legal without exposing the helper to callers.
mod interior {
    use super::{Exclusive, ExclusiveKind, Robust, WithInterior, WithoutInterior};

    /// Robustness of interior-mutable storage reachable from a mutability axis.
    /// Used to gate validation under shared access, independently of the value's traps.
    pub trait InteriorTrap {
        type Trap;
    }

    impl InteriorTrap for WithoutInterior {
        type Trap = Robust;
    }
    impl<R> InteriorTrap for WithInterior<R> {
        type Trap = R;
    }
    impl<I: ExclusiveKind + InteriorTrap> InteriorTrap for Exclusive<I> {
        type Trap = I::Trap;
    }
}

disjoint_impls! {
/// Type that can be safely transmuted into its C representation.
///
/// # Safety
/// `Self` and `Self::CType` must be compatible with the same stable ABI.

    pub unsafe trait AbiCompatible: ReprC {}

    unsafe impl<R: AbiCompatible + ?Sized, __Interior: ExclusiveKind> AbiCompatible for &R
        where
            Self: RustSpec<Layout = Stable>,
            R: RustSpec<Mutability = Exclusive<__Interior>>,
            Exclusive<__Interior>: InteriorTrap<Trap = Robust>,
            Self: ReprC<CType = CRef<<R as ReprC>::CType>>,
        {}

    unsafe impl<R: AbiCompatible + ?Sized, __Trap> AbiCompatible for &R
        where
            Self: RustSpec<Layout = Stable>,
            R: RustSpec<Mutability = WithInterior<__Trap>>,
            WithInterior<__Trap>: InteriorTrap<Trap = Robust>,
            Self: ReprC<CType = CRefMut<<R as ReprC>::CType, false>>,
        {}

    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatible, __Interior: ExclusiveKind> AbiCompatible for Box<R>
        where
            Self: RustSpec<Layout = Stable>,
            R: RustSpec<Mutability = Exclusive<__Interior>>,
            Self: ReprC<CType = CBox<<R as ReprC>::CType>>,
            <R as ReprC>::CType: Sized,
        {}

    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatible, __Trap> AbiCompatible for Box<R>
        where
            Self: RustSpec<Layout = Stable>,
            R: RustSpec<Mutability = WithInterior<__Trap>>,
            Self: ReprC<CType = CBox<<R as ReprC>::CType>>,
            <R as ReprC>::CType: Sized,
        {}

    unsafe impl<R: AbiCompatible<CType: Copy> + ReprC, E: ReprC> AbiCompatible for Result<R, E>
        where
            Self: RustSpec<Layout = Stable> + ReprC<CType = <R as ReprC>::CType>,
            R: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
            E: RustSpec<Size = RustSpecSized<Zero>>,
        {}

    unsafe impl<R: ReprC, E: AbiCompatible<CType: Copy>> AbiCompatible for Result<R, E>
        where
            Self: RustSpec<Layout = Stable> + ReprC<CType = <E as ReprC>::CType>,
            R: RustSpec<Size = RustSpecSized<Zero>>,
            E: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
        {}

    unsafe impl<R: AbiCompatible + ?Sized> AbiCompatible for ManuallyDrop<R>
        where
            Self: RustSpec<Layout = Stable>,
            R: RustSpec<Drop = NoDrop>,
        {}

    unsafe impl<R: AbiCompatible + ?Sized> AbiCompatible for ManuallyDrop<R>
        where
            Self: RustSpec<Layout = Stable>,
            R: RustSpec<Drop: NeedsDrop>,
        {}

    unsafe impl<T: AbiCompatible + ?Sized> AbiCompatible for UnsafeCell<T>
        where
            Self: RustSpec<Layout = Stable, Trap = Robust>
        {}

    unsafe impl<T: AbiCompatible + ?Sized> AbiCompatible for UnsafeCell<T>
        where
            Self: RustSpec<Layout = Stable, Trap = NonRobust>
        {}

    unsafe impl<T: AbiCompatible + ?Sized> AbiCompatible for Cell<T>
        where
            Self: RustSpec<Layout = Stable, Trap = Robust>
        {}

    unsafe impl<T: AbiCompatible + ?Sized> AbiCompatible for Cell<T>
        where
            Self: RustSpec<Layout = Stable, Trap = NonRobust>
        {}
}

disjoint_impls! {
    /// Validates a C representation before converting it to `Self`.
    ///
    /// # Safety
    ///
    /// Validation must not inspect contents of robust types with interior mutability.
    pub unsafe trait AbiCompatibleValidate: AbiCompatible {
        /// Called when transmuting an [`ReprC::CType`] back into [`Self`] to check for trap representations.
        ///
        /// Return `true` only when `target` is a valid representation of `Self`.
        ///
        /// # Safety
        ///
        /// Any pointers dereferenced while checking `target` must be valid for reads.
        unsafe fn is_valid(target: &Self::CType) -> bool;
    }

    unsafe impl<R: AbiCompatibleValidate + ?Sized, __Interior: ExclusiveKind> AbiCompatibleValidate for &R
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Mutability = Exclusive<__Interior>>,
        Exclusive<__Interior>: InteriorTrap<Trap = Robust>,
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

    unsafe impl<R: AbiCompatibleValidate + ?Sized, __Trap> AbiCompatibleValidate for &R
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Mutability = WithInterior<__Trap>>,
        WithInterior<__Trap>: InteriorTrap<Trap = Robust>,
        Self: ReprC<CType = CRefMut<<R as ReprC>::CType, false>>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            let ptr = target.as_ptr();
            !ptr.is_null() && unsafe { R::is_valid(&*ptr) }
        }
    }


    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatibleValidate, __Interior: ExclusiveKind> AbiCompatibleValidate for Box<R>
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Mutability = Exclusive<__Interior>>,
        Self: ReprC<CType = CBox<<R as ReprC>::CType>>,
        <R as ReprC>::CType: Sized,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            if target.is_null() {
                return false;
            }

            unsafe { R::is_valid(&*target.as_ptr()) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatibleValidate, __Trap> AbiCompatibleValidate for Box<R>
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Mutability = WithInterior<__Trap>>,
        Self: ReprC<CType = CBox<<R as ReprC>::CType>>,
        <R as ReprC>::CType: Sized,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            if target.is_null() {
                return false;
            }

            unsafe { R::is_valid(&*target.as_ptr()) }
        }
    }


    unsafe impl<R: AbiCompatibleValidate<CType: Copy> + ReprC, E: ReprC> AbiCompatibleValidate for Result<R, E>
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = <R as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            unsafe { R::is_valid(target) }
        }
    }

    unsafe impl<R: ReprC, E: AbiCompatibleValidate<CType: Copy>> AbiCompatibleValidate for Result<R, E>
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = <E as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            unsafe { E::is_valid(target) }
        }
    }


    unsafe impl<R: AbiCompatibleValidate + ?Sized> AbiCompatibleValidate for ManuallyDrop<R>
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Drop = NoDrop>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            unsafe { R::is_valid(target) }
        }
    }


    unsafe impl<R: AbiCompatibleValidate + ?Sized> AbiCompatibleValidate for ManuallyDrop<R>
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Drop: NeedsDrop>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            unsafe { R::is_valid(target) }
        }
    }


    unsafe impl<T: AbiCompatibleValidate + ?Sized> AbiCompatibleValidate for UnsafeCell<T>
    where
        Self: RustSpec<Layout = Stable, Trap = Robust>
    {
        #[inline(always)]
        unsafe fn is_valid(_: &Self::CType) -> bool {
            true
        }
    }

    unsafe impl<T: AbiCompatibleValidate + ?Sized> AbiCompatibleValidate for UnsafeCell<T>
    where
        Self: RustSpec<Layout = Stable, Trap = NonRobust>
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            unsafe { T::is_valid(&*target.get()) }
        }
    }


    unsafe impl<T: AbiCompatibleValidate + ?Sized> AbiCompatibleValidate for Cell<T>
    where
        Self: RustSpec<Layout = Stable, Trap = Robust>
    {
        #[inline(always)]
        unsafe fn is_valid(_: &Self::CType) -> bool {
            true
        }
    }

    unsafe impl<T: AbiCompatibleValidate + ?Sized> AbiCompatibleValidate for Cell<T>
    where
        Self: RustSpec<Layout = Stable, Trap = NonRobust>
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            unsafe { T::is_valid(&*target.get()) }
        }
    }

}

unsafe impl<R: AbiCompatibleValidate + ?Sized> AbiCompatibleValidate for &mut R
where
    Self: RustSpec<Layout = Stable> + ReprC<CType = CRefMut<R::CType, true>>,
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
unsafe impl<R: AbiCompatible + ?Sized> AbiCompatible for &mut R where
    Self: RustSpec<Layout = Stable> + ReprC<CType = CRefMut<R::CType, true>>
{
}

unsafe impl<R: AbiCompatibleValidate<CType: Sized>, const N: usize> AbiCompatibleValidate for [R; N]
where
    Self: RustSpec<Layout = Stable>,
{
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
unsafe impl<R: AbiCompatible<CType: Sized>, const N: usize> AbiCompatible for [R; N] where
    Self: RustSpec<Layout = Stable>
{
}

unsafe impl<R: AbiCompatibleValidate<CType: Sized>> AbiCompatibleValidate for [R]
where
    Self: RustSpec<Layout = Stable>,
{
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
unsafe impl<R: AbiCompatible<CType: Sized>> AbiCompatible for [R] where
    Self: RustSpec<Layout = Stable>
{
}

unsafe impl AbiCompatibleValidate for str {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        let ptr = core::ptr::from_ref(target);
        let len = <Utf8Str as crate::wide::Wide>::metadata(ptr);
        let bytes = unsafe { core::slice::from_raw_parts(ptr.cast::<u8>(), len) };
        core::str::from_utf8(bytes).is_ok()
    }
}
unsafe impl AbiCompatible for str {}

unsafe impl<R: AbiCompatibleValidate + Niche<CType: PartialEq>> AbiCompatibleValidate for Option<R>
where
    Self: RustSpec<Layout = Stable> + ReprC<CType = R::CType>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        *target == R::NICHE || unsafe { R::is_valid(target) }
    }
}
unsafe impl<R: AbiCompatible + Niche<CType: PartialEq>> AbiCompatible for Option<R> where
    Self: RustSpec<Layout = Stable> + ReprC<CType = R::CType>
{
}

// TODO: Use this somehow to strengthen AbiCompatible assumptions
//fn assert_size_and_allignment_match<R: AbiCompatible<CType: Sized>>() {
//    const {
//        debug_assert!(core::mem::size_of::<R>() == core::mem::size_of::<R::CType>());
//        debug_assert!(core::mem::align_of::<R>() == core::mem::align_of::<R::CType>());
//    };
//}

/// Move a value between layout-compatible representations without borrowing
/// or reading its fields, including any interior-mutable fields.
///
/// # Safety
/// Both types must have compatible layouts and the source must be valid as T.
/// Ownership and any lifetime/aliasing obligations must transfer to the result.
pub(crate) unsafe fn transmute_owned<S, T>(source: S) -> T {
    const {
        assert!(core::mem::size_of::<S>() == core::mem::size_of::<T>());
        assert!(core::mem::align_of::<S>() == core::mem::align_of::<T>());
    }
    union Value<S, T> {
        source: ManuallyDrop<S>,
        target: ManuallyDrop<T>,
    }
    let source = Value {
        source: ManuallyDrop::new(source),
    };
    unsafe { ManuallyDrop::into_inner(source.target) }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::vec::Vec;

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::CWideBox;

    #[repr(transparent)]
    struct LayoutOnly(());

    impl ReprC for LayoutOnly {
        type CType = ();
    }

    // SAFETY: Both representations are zero-sized and use the same ABI.
    unsafe impl AbiCompatible for LayoutOnly {}

    #[test]
    fn layout_compatibility_does_not_require_validation() {
        assert_impl_all!(LayoutOnly: AbiCompatible);
        assert_not_impl_any!(LayoutOnly: AbiCompatibleValidate);
    }
    use crate::{
        CType, Decode, Encode,
        slice::{CWideRef, CWideRefMut},
    };

    #[test]
    fn option_validation_accepts_none_and_checks_some() {
        use crate::primitives::CBool;
        use core::num::NonZeroU8;

        assert!(unsafe { <Option<NonZeroU8>>::is_valid(&0) });
        assert!(unsafe { <Option<NonZeroU8>>::is_valid(&7) });
        assert!(unsafe { <Option<&bool>>::is_valid(&CRef::null()) });
        assert!(unsafe { <Option<&bool>>::is_valid(&CRef::from_ref(&CBool(1))) });
        assert!(!unsafe { <Option<&bool>>::is_valid(&CRef::from_ref(&CBool(3))) });

        let none = None::<NonZeroU8>;
        let encoded = crate::encode(&none);
        assert_eq!(
            unsafe { crate::decode::<&Option<NonZeroU8>>(encoded) },
            Some(&none)
        );
    }

    #[test]
    fn non_robust_unsized_cells_validate_under_exclusive_access() {
        use core::num::NonZeroU8;

        assert_impl_all!(UnsafeCell<[NonZeroU8]>: AbiCompatible);
        assert_impl_all!(Cell<[NonZeroU8]>: AbiCompatible);
        static_assertions::assert_not_impl_any!(
            &UnsafeCell<[NonZeroU8]>: Encode, Decode<'static>
        );
        static_assertions::assert_not_impl_any!(
            &Cell<[NonZeroU8]>: Encode, Decode<'static>
        );

        let mut storage = UnsafeCell::new([1_u8, 2]);
        let target: &mut UnsafeCell<[u8]> = &mut storage;
        // SAFETY: The local backing storage remains exclusively accessible.
        unsafe {
            assert!(<UnsafeCell<[NonZeroU8]>>::is_valid(target));
            assert!(<Cell<[NonZeroU8]>>::is_valid(target));
        }
        target.get_mut()[1] = 0;
        unsafe {
            assert!(!<UnsafeCell<[NonZeroU8]>>::is_valid(target));
            assert!(!<Cell<[NonZeroU8]>>::is_valid(target));
        }
    }

    #[test]
    fn manually_drop_validates_inner_representation() {
        let invalid_bool = <bool as Niche>::NICHE;
        assert!(!unsafe { <ManuallyDrop<bool> as AbiCompatibleValidate>::is_valid(&invalid_bool) });

        #[cfg(feature = "alloc")]
        {
            let null_box = ManuallyDrop::new(CBox::<u8>::null());
            assert!(!unsafe {
                <ManuallyDrop<Box<u8>> as AbiCompatibleValidate>::is_valid(&null_box)
            });
        }
    }

    #[test]
    fn robust_shared_interior_reference_checks_only_the_pointer() {
        use core::cell::UnsafeCell;

        let cell = UnsafeCell::new(7_u8);
        let encoded = crate::encode(&cell);
        assert_eq!(encoded.as_ptr(), core::ptr::from_ref(&cell).cast_mut());
        assert!(unsafe { <&UnsafeCell<u8> as AbiCompatibleValidate>::is_valid(&encoded) });

        let null = CRefMut::<UnsafeCell<u8>, false>::null();
        assert!(!unsafe { <&UnsafeCell<u8> as AbiCompatibleValidate>::is_valid(&null) });
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
            Niche<CType = crate::vec::CVec<crate::primitives::CBool>>,
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
            Niche<CType = crate::vec::CVec<CRef<u8>>>,
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
            Niche<CType = crate::vec::CVec<CRef<crate::primitives::CBool>>>,
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
            Niche<CType = crate::vec::CVec<CRefMut<u8, true>>>,
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
            Niche<CType = crate::vec::CVec<CRefMut<crate::primitives::CBool, true>>>,
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

#[cfg(test)]
mod conversion_tests {
    //! Regression tests for conversion and writeback paths.
    use crate::transmute::AbiCompatibleValidate;
    use crate::{
        ReprC,
        primitives::CBool,
        stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked, EncodeRef, EncodeRefUnchecked, Store},
        transmute::AbiCompatible,
    };
    use core::cell::{Cell, UnsafeCell};

    #[derive(rust_spec::RustSpec)]
    #[repr(transparent)]
    struct Direct(u8);

    impl ReprC for Direct {
        type CType = u8;
    }
    // SAFETY: Direct is a transparent, robust u8.
    unsafe impl AbiCompatibleValidate for Direct {
        unsafe fn is_valid(_: &u8) -> bool {
            true
        }
    }
    unsafe impl AbiCompatible for Direct {}
    // These deliberately panic: aggregate fast paths must not invoke them.
    unsafe impl EncodeMoveUnchecked for Direct {
        type SoftStore = ();
        unsafe fn soft_encode_unchecked<'itm>(self, _: &'itm mut ()) -> u8
        where
            Self: 'itm,
        {
            panic!("visited field")
        }
    }
    unsafe impl EncodeMove for Direct {
        type Store = ();
        fn soft_encode<'itm>(self, _: &mut ()) -> u8
        where
            Self: 'itm,
        {
            panic!("visited field")
        }
    }
    unsafe impl<'d> DecodeMove<'d> for Direct {
        type Store = ();
        unsafe fn soft_decode<'itm: 'd>(_: u8, _: &'itm mut ()) -> Option<Self> {
            panic!("visited field")
        }
        unsafe fn soft_decode_unchecked<'itm: 'd>(_: u8, _: &'itm mut ()) -> Self {
            panic!("visited field")
        }
    }

    #[test]
    fn stable_robust_arrays_encode_elements() {
        assert_eq!([3u8, 4].soft_encode(&mut Default::default()), [3, 4]);
        assert_eq!(
            unsafe { [5u8, 6].soft_encode_unchecked(&mut Default::default()) },
            [5, 6]
        );
        let values = unsafe { <[Direct; 2] as DecodeMove>::soft_decode([7, 8], &mut Default::default()) }.unwrap();
        assert_eq!(values.map(|x| x.0), [7, 8]);
        let values = unsafe { <[Direct; 2] as DecodeMove>::soft_decode_unchecked([9, 10], &mut Default::default()) };
        assert_eq!(values.map(|x| x.0), [9, 10]);
    }

    #[test]
    fn owned_cells_move_without_visiting_contents() {
        assert_eq!(Cell::new(Direct(3)).soft_encode(&mut Default::default()).into_inner(), 3);
        assert_eq!(
            UnsafeCell::new(Direct(4)).soft_encode(&mut Default::default()).into_inner(),
            4
        );
        let cell = unsafe { Cell::new(Direct(5)).soft_encode_unchecked(&mut Default::default()) };
        assert_eq!(cell.into_inner(), 5);
        let cell =
            unsafe { <UnsafeCell<Direct> as DecodeMove>::soft_decode(UnsafeCell::new(6), &mut Default::default()) }
                .unwrap();
        assert_eq!(cell.into_inner().0, 6);
        let cell = unsafe {
            <Cell<Direct> as DecodeMove>::soft_decode_unchecked(UnsafeCell::new(7), &mut Default::default())
        };
        assert_eq!(cell.into_inner().0, 7);
        // Non-Copy carriers are moved, not read through &UnsafeCell.
        let cells = [UnsafeCell::new(Direct(8)), UnsafeCell::new(Direct(9))]
            .soft_encode(&mut Default::default());
        assert_eq!(cells.map(UnsafeCell::into_inner), [8, 9]);
    }

    #[test]
    fn stable_shared_arrays_preserve_the_original_pointer() {
        let values = [Direct(3), Direct(4)];
        let encoded = values.encode_ref(&mut Default::default());
        assert_eq!(encoded.as_ptr().cast::<u8>(), values.as_ptr().cast::<u8>());
        let encoded = unsafe { values.encode_ref_unchecked(&mut Default::default()) };
        assert_eq!(encoded.as_ptr().cast::<u8>(), values.as_ptr().cast::<u8>());
    }

    #[test]
    fn nonrobust_checked_conversions_still_validate() {
        let mut store = Default::default();
        assert!(
            unsafe { <[bool; 2] as DecodeMove>::soft_decode([CBool(1), CBool(2)], &mut store) }
                .is_none()
        );
        assert!(
            unsafe { <Cell<bool> as DecodeMove>::soft_decode(UnsafeCell::new(CBool(2)), &mut Default::default()) }
                .is_none()
        );
        let mut store = Default::default();
        let values =
            unsafe { <[bool; 2] as DecodeMove>::soft_decode([CBool(1), CBool(0)], &mut store) }
                .unwrap();
        assert_eq!(values, [true, false]);
    }

    #[test]
    fn checked_arrays_keep_mutable_reference_writeback() {
        let mut value = true;
        let mut store = Default::default();
        let encoded = [&mut value].soft_encode(&mut store);
        // SAFETY: The mutable carrier refers to the live conversion store.
        unsafe { encoded[0].as_ptr().write(CBool::FALSE) };
        assert_eq!(store.sync(), Some(()));
        assert!(!value);
    }

    #[test]
    fn manually_drop_keeps_mutable_reference_writeback() {
        let mut value = true;
        let mut store = Default::default();
        let encoded = core::mem::ManuallyDrop::new(&mut value).soft_encode(&mut store);
        unsafe { encoded.as_ptr().write(CBool::FALSE) };
        assert_eq!(store.sync(), Some(()));
        assert!(!value);
    }

    #[test]
    fn unstable_arrays_keep_field_conversion_and_stores() {
        let values = [(true, 3u8), (false, 4u8)];
        let mut store = Default::default();
        let encoded = values.soft_encode(&mut store);
        assert_eq!(encoded[0].0, CBool::TRUE);
        assert_eq!(encoded[1].1, 4);
        let mut decode_store = Default::default();
        let decoded =
            unsafe { <[(bool, u8); 2] as DecodeMove>::soft_decode(encoded, &mut decode_store) }
                .unwrap();
        assert_eq!(decoded, values);
    }

    #[test]
    fn moving_values_does_not_drop_the_source_or_duplicate_ownership() {
        use core::sync::atomic::{AtomicUsize, Ordering};
        static DROPS: AtomicUsize = AtomicUsize::new(0);
        #[repr(transparent)]
        struct Owned(u8);
        impl Drop for Owned {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }
        // SAFETY: Identity conversion transfers ownership of the same type.
        let moved: Owned = unsafe { crate::transmute::transmute_owned(Owned(7)) };
        assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        assert_eq!(moved.0, 7);
        drop(moved);
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unchecked_arrays_keep_shared_interior_references_without_validation() {
        use core::num::NonZeroU8;
        let cell = UnsafeCell::new(NonZeroU8::new(3).unwrap());
        let mut store = Default::default();
        let encoded = unsafe { [&cell].soft_encode_unchecked(&mut store) };
        assert_eq!(
            encoded[0].as_ptr().cast::<()>(),
            core::ptr::from_ref(&cell).cast_mut().cast::<()>()
        );
    }

    #[test]
    fn manually_drop_robust_interior_decodes_without_visiting_payload() {
        use core::mem::ManuallyDrop;

        #[derive(rust_spec::RustSpec)]
        #[repr(transparent)]
        struct Interior(UnsafeCell<u8>);

        impl ReprC for Interior {
            type CType = UnsafeCell<u8>;
        }
        // SAFETY: Interior is a transparent wrapper around its carrier.
        unsafe impl AbiCompatibleValidate for Interior {
            unsafe fn is_valid(_: &Self::CType) -> bool {
                panic!("visited robust interior validator")
            }
        }
        unsafe impl AbiCompatible for Interior {}
        unsafe impl<'d> DecodeMove<'d> for Interior {
            type Store = ();
            unsafe fn soft_decode<'itm: 'd>(_: Self::CType, _: &'itm mut ()) -> Option<Self> {
                panic!("visited interior decoder")
            }
            unsafe fn soft_decode_unchecked<'itm: 'd>(_: Self::CType, _: &'itm mut ()) -> Self {
                panic!("visited interior decoder")
            }
        }

        let decoded = unsafe {
            <ManuallyDrop<Interior> as DecodeMove>::soft_decode(UnsafeCell::new(3), &mut Default::default())
        }
        .unwrap();
        assert_eq!(ManuallyDrop::into_inner(decoded).0.into_inner(), 3);
        let decoded = unsafe {
            <ManuallyDrop<Interior> as DecodeMove>::soft_decode_unchecked(UnsafeCell::new(4), &mut Default::default())
        };
        assert_eq!(ManuallyDrop::into_inner(decoded).0.into_inner(), 4);

        // Non-robust interior still delegates to the payload's validating decoder.
        assert!(
            unsafe {
                <ManuallyDrop<Cell<bool>> as DecodeMove>::soft_decode(
                    UnsafeCell::new(CBool(2)),
                    &mut Default::default(),
                )
            }
            .is_none()
        );
        let decoded = unsafe {
            <ManuallyDrop<Cell<bool>> as DecodeMove>::soft_decode(UnsafeCell::new(CBool(1)), &mut Default::default())
        }
        .unwrap();
        assert!(ManuallyDrop::into_inner(decoded).into_inner());
    }
}
