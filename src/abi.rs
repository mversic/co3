//! Module providing ABI compatible functionality

#[cfg(feature = "alloc")]
use alloc::boxed::Box;
use core::{
    cell::{Cell, UnsafeCell},
    mem::ManuallyDrop,
};

use disjoint_impls::disjoint_impls;
#[doc(hidden)]
pub use interior::InteriorTrap;
use rust_spec::{
    Gt, RustSpec, Stable,
    layout::{NonRobust, Robust},
    mutability::{Exclusive, ExclusiveKind, WithInterior, WithoutInterior},
    size::{Sized as RustSpecSized, Zero},
};

#[cfg(feature = "alloc")]
use crate::boxed::abi::Box as AbiBox;
use crate::{
    CType, ReprC,
    niche::Niche,
    option::abi::OptionSome as AbiOptionSome,
    reference::{RefCarrier, RefMutCarrier},
    result::abi::{ResultErr as AbiResultErr, ResultOk as AbiResultOk},
    wide::Wide as _,
};

disjoint_impls! {
    // A 1-ZST is ABI-compatible with (). Other aggregate returns need nonzero size.
    pub(crate) trait CFnReturnLayout {}

    impl<T: RustSpec<Size = RustSpecSized<Gt<Zero>>>> CFnReturnLayout for T {}
    impl<T: RustSpec<Size = RustSpecSized<Zero>, __Alignment = rust_spec::One>> CFnReturnLayout for T {}
}

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

    unsafe impl<R: AbiCompatible<CType: CType + Copy> + Niche, E: ReprC> AbiCompatible for Result<R, E>
        where
            Self: RustSpec<Layout = Stable> + ReprC,
            R: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
            E: RustSpec<Size = RustSpecSized<Zero>>,
        {}

    unsafe impl<R: ReprC, E: AbiCompatible<CType: CType + Copy> + Niche> AbiCompatible for Result<R, E>
        where
            Self: RustSpec<Layout = Stable> + ReprC,
            R: RustSpec<Size = RustSpecSized<Zero>>,
            E: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
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
        /// * Any pointers dereferenced while checking `target` must be valid for reads.
        /// * Inspecting non-robust interior-mutable contents requires exclusive access.
        unsafe fn is_valid(target: &Self::CType) -> bool;
    }

    unsafe impl<R: AbiCompatibleValidate<CType: CType + Copy> + Niche, E: ReprC<CType: CType + Sized>>
        AbiCompatibleValidate for Result<R, E>
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = AbiResultOk<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            R::is_niche(&target.0) || unsafe { R::is_valid(&target.0) }
        }
    }

    unsafe impl<R: ReprC<CType: CType + Sized>, E: AbiCompatibleValidate<CType: CType + Copy> + Niche>
        AbiCompatibleValidate for Result<R, E>
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = AbiResultErr<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            E::is_niche(&target.0) || unsafe { E::is_valid(&target.0) }
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
            // SAFETY: The validation contract requires exclusive access to these contents.
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
            // SAFETY: The validation contract requires exclusive access to these contents.
            unsafe { T::is_valid(&*target.as_ptr()) }
        }
    }
}

unsafe impl<R: AbiCompatible + RustSpec + ?Sized> AbiCompatible for &R
where
    Self: RustSpec<Layout = Stable>,
    <R as RustSpec>::Mutability: InteriorTrap<Trap = Robust>,
    Self: ReprC<CType: RefCarrier<R>>,
{
}

unsafe impl<R: AbiCompatibleValidate + RustSpec + ?Sized> AbiCompatibleValidate for &R
where
    Self: RustSpec<Layout = Stable>,
    <R as RustSpec>::Mutability: InteriorTrap<Trap = Robust>,
    Self: ReprC<CType: RefCarrier<R>>,
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

unsafe impl<R: AbiCompatibleValidate + ?Sized> AbiCompatibleValidate for &mut R
where
    Self: RustSpec<Layout = Stable> + ReprC<CType: RefMutCarrier<R>>,
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
    Self: RustSpec<Layout = Stable> + ReprC<CType: RefMutCarrier<R>>
{
}

#[cfg(feature = "alloc")]
unsafe impl<R: AbiCompatible<CType: Sized>> AbiCompatible for Box<R> where
    Self: RustSpec<Layout = Stable> + ReprC<CType = AbiBox<<R as ReprC>::CType>>
{
}
#[cfg(feature = "alloc")]
unsafe impl<R: AbiCompatibleValidate<CType: Sized>> AbiCompatibleValidate for Box<R>
where
    Self: RustSpec<Layout = Stable> + ReprC<CType = AbiBox<<R as ReprC>::CType>>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        if target.is_null() {
            return false;
        }
        unsafe { R::is_valid(&*target.as_ptr()) }
    }
}

unsafe impl<R: AbiCompatibleValidate<CType: Sized>, const N: usize> AbiCompatibleValidate for [R; N]
where
    Self: RustSpec<Layout = Stable>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
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
        let len = co3::primitive::abi::str::metadata(ptr);
        let bytes = unsafe { core::slice::from_raw_parts(ptr.cast::<u8>(), len) };
        core::str::from_utf8(bytes).is_ok()
    }
}
unsafe impl AbiCompatible for str {}

unsafe impl<R: AbiCompatibleValidate + Niche> AbiCompatibleValidate for Option<R>
where
    Self: RustSpec<Layout = Stable> + ReprC<CType = AbiOptionSome<<R as ReprC>::CType>>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        R::is_niche(&target.0) || unsafe { R::is_valid(&target.0) }
    }
}
unsafe impl<R: AbiCompatible + Niche> AbiCompatible for Option<R> where
    Self: RustSpec<Layout = Stable> + ReprC<CType = AbiOptionSome<<R as ReprC>::CType>>
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
    use core::cell::{Cell, UnsafeCell};

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::wide::abi::WideBox;
    use crate::{
        CType, ReprC,
        convert::{
            AbiCompatibleValidate, Decode, DecodeMove, Encode, EncodeMove, EncodeMoveUnchecked,
            Store,
        },
        marker::{AbiCompatible, NonZero},
        reference::abi::Ref,
        wide::{
            Wide,
            abi::{SoftWideRefMut as AbiSoftWideRefMut, WideRef as AbiWideRef},
        },
    };

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

    #[test]
    fn option_validation_accepts_none_and_checks_some() {
        use core::num::NonZeroU8;

        assert!(unsafe { <Option<NonZeroU8>>::is_valid(&AbiOptionSome(NonZero(0))) });
        assert!(unsafe { <Option<NonZeroU8>>::is_valid(&AbiOptionSome(NonZero(7))) });
        assert!(unsafe { <Option<&bool>>::is_valid(&AbiOptionSome(Ref::null())) });
        assert!(unsafe {
            <Option<&bool>>::is_valid(&AbiOptionSome(Ref::from_ref(&co3::primitive::abi::bool(1))))
        });
        assert!(!unsafe {
            <Option<&bool>>::is_valid(&AbiOptionSome(Ref::from_ref(&co3::primitive::abi::bool(3))))
        });

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

        let storage = UnsafeCell::new([NonZero::<u8>(1_u8), NonZero(2)]);
        let target: &mut core::cell::UnsafeCell<[NonZero<u8>]> =
            unsafe { Wide::from_raw_parts_mut(storage.get().cast(), 2) };
        // SAFETY: The local backing storage remains exclusively accessible.
        unsafe {
            assert!(<UnsafeCell<[NonZeroU8]>>::is_valid(target));
            assert!(<Cell<[NonZeroU8]>>::is_valid({
                core::cell::Cell::<[NonZero<u8>]>::from_raw_parts_mut(target.get().cast(), 2)
            }));
        }
        target.get_mut()[1] = NonZero(0);
        unsafe {
            assert!(!<UnsafeCell<[NonZeroU8]>>::is_valid(target));
            assert!(!<Cell<[NonZeroU8]>>::is_valid({
                core::cell::Cell::<[NonZero<u8>]>::from_raw_parts_mut(target.get().cast(), 2)
            }));
        }
    }

    #[test]
    fn manually_drop_validates_inner_representation() {
        let invalid_bool = <bool as Niche>::NICHE;
        assert!(!unsafe {
            <ManuallyDrop<bool>>::is_valid(&core::mem::ManuallyDrop::new(invalid_bool))
        });

        #[cfg(feature = "alloc")]
        {
            let null_box = core::mem::ManuallyDrop::new(AbiBox::<u8>::null());
            assert!(!unsafe { <ManuallyDrop<Box<u8>>>::is_valid(&null_box) });
        }
    }

    #[test]
    fn robust_shared_interior_reference_checks_only_the_pointer() {
        use core::cell::UnsafeCell;

        let cell = UnsafeCell::new(7_u8);
        let encoded = crate::encode(&cell);
        assert_eq!(
            encoded.as_ptr(),
            core::ptr::from_ref(&cell).cast_mut().cast()
        );
        assert!(unsafe { <&UnsafeCell<u8>>::is_valid(&encoded) });

        let null = Ref::<<UnsafeCell<u8> as crate::ReprC>::CType>::null();
        assert!(!unsafe { <&UnsafeCell<u8>>::is_valid(&null) });
    }

    #[test]
    fn transparent_type() {
        assert_impl_all!(bool:
            Niche<CType = crate::primitive::abi::bool>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&bool:
            Niche<CType = Ref<'static, <bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut bool:
            Niche<CType = crate::reference::abi::SoftRefMut<<bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<bool>:
            Niche<CType = AbiBox<<bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[bool]:
            Niche<CType = AbiWideRef<'static, <[bool] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [bool]:
            Niche<CType = AbiSoftWideRefMut<<[bool] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[bool]>:
            Niche<CType = WideBox<<[bool] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<bool>:
            Niche<CType = crate::vec::abi::Vec<<bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([bool; 2]:
            Niche<CType = [crate::primitive::abi::bool; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<bool>:
            Niche<CType = AbiOptionSome<<bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn robust_ref() {
        assert_impl_all!(&&u8:
            Niche<CType = Ref<'static, <&'static u8 as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &u8:
            Niche<CType = crate::reference::abi::SoftRefMut<<&'static u8 as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            Niche<CType = AbiBox<<&'static bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&u8]:
            Niche<CType = AbiWideRef<'static, <[&'static u8] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&u8]:
            Niche<CType = AbiSoftWideRefMut<<[&'static u8] as crate::ReprC>::CType>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&u8]>:
            Niche<CType = WideBox<<[&'static u8] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&u8>:
            Niche<CType = crate::vec::abi::Vec<<&'static u8 as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&u8; 2]:
            Niche<CType = [Ref<'static, u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&u8>:
            ReprC<CType = AbiOptionSome<<&'static u8 as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(Option<&u8>: CType);
    }

    #[test]
    fn transparent_ref() {
        assert_impl_all!(&&bool:
            Niche<CType = Ref<'static, <&'static bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &bool:
            Niche<CType = crate::reference::abi::SoftRefMut<<&'static bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            Niche<CType = AbiBox<<&'static bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&bool]:
            Niche<CType = AbiWideRef<'static, <[&'static bool] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [&bool]:
            Niche<CType = AbiSoftWideRefMut<<[&'static bool] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&bool]>:
            Niche<CType = WideBox<<[&'static bool] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&bool>:
            Niche<CType = crate::vec::abi::Vec<<&'static bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&bool; 2]:
            Niche<CType = [Ref<'static, <bool as crate::ReprC>::CType>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&bool>:
            ReprC<CType = AbiOptionSome<<&'static bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn robust_ref_mut() {
        assert_impl_all!(&&mut u8:
            Niche<CType = Ref<'static, <&'static mut u8 as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &mut u8:
            Niche<CType = crate::reference::abi::SoftRefMut<<&'static mut u8 as crate::ReprC>::CType>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut u8>:
            Niche<CType = AbiBox<<&'static mut u8 as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&mut u8]:
            Niche<CType = AbiWideRef<'static, <[&'static mut u8] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut u8]:
            Niche<CType = AbiSoftWideRefMut<<[&'static mut u8] as crate::ReprC>::CType>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut u8]>:
            Niche<CType = WideBox<<[&'static mut u8] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut u8>:
            Niche<CType = crate::vec::abi::Vec<<&'static mut u8 as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&mut u8; 2]:
            Niche<CType = [crate::reference::abi::RefMut<'static, u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&mut u8>:
            ReprC<CType = AbiOptionSome<<&'static mut u8 as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(Option<&mut u8>: CType);
    }

    #[test]
    fn transparent_ref_mut() {
        assert_impl_all!(&&mut bool:
            Niche<CType = Ref<'static, <&'static mut bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &mut bool:
            Niche<CType = crate::reference::abi::SoftRefMut<<&'static mut bool as crate::ReprC>::CType>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut bool>:
            Niche<CType = AbiBox<<&'static mut bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&mut bool]:
            Niche<CType = AbiWideRef<'static, <[&'static mut bool] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut bool]:
            Niche<CType = AbiSoftWideRefMut<<[&'static mut bool] as crate::ReprC>::CType>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut bool]>:
            Niche<CType = WideBox<<[&'static mut bool] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut bool>:
            Niche<CType = crate::vec::abi::Vec<<&'static mut bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode
        );
        assert_impl_all!([&mut bool; 2]:
            Niche<CType = [crate::reference::abi::SoftRefMut<<bool as crate::ReprC>::CType>; 2]>,
            Decode<'static>,
            Encode
        );
        assert_impl_all!(Option<&mut bool>:
            ReprC<CType = AbiOptionSome<<&'static mut bool as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode
        );
    }

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

    impl crate::convert::EncodeUnchecked for Direct {}
    impl crate::convert::Encode for Direct {}
    impl crate::convert::Decode<'_> for Direct {}

    #[test]
    fn empty_arrays_convert() {
        static_assertions::assert_impl_all!([core::num::NonZeroU8; 1]: crate::niche::Niche);
        let value: [u8; 0] = [];
        assert_eq!(crate::encode(value), [] as [u8; 0]);
        assert_eq!(unsafe { crate::decode::<[u8; 0]>([]) }, Some([]));
        assert!(unsafe { <[u8; 0]>::is_valid(&[]) });
    }

    #[test]
    fn stable_robust_arrays_encode_elements() {
        assert_eq!(crate::encode([3u8, 4]), [3, 4]);
        assert_eq!(unsafe { crate::encode_unchecked([5u8, 6]) }, [5, 6]);
        let values =
            unsafe { crate::soft_decode::<[Direct; 2]>([7, 8], &mut Default::default()) }.unwrap();
        assert_eq!(values.map(|x| x.0), [7, 8]);
        let values = unsafe {
            crate::soft_decode_unchecked::<[Direct; 2]>([9, 10], &mut Default::default())
        };
        assert_eq!(values.map(|x| x.0), [9, 10]);
    }

    #[test]
    fn owned_cells_move_without_visiting_contents() {
        assert_eq!(crate::encode(Cell::new(Direct(3))).into_inner(), 3);
        assert_eq!(crate::encode(UnsafeCell::new(Direct(4))).into_inner(), 4);
        let cell = unsafe { crate::encode_unchecked(Cell::new(Direct(5))) };
        assert_eq!(cell.into_inner(), 5);
        let cell = unsafe {
            crate::soft_decode::<UnsafeCell<Direct>>(
                core::cell::UnsafeCell::new(6),
                &mut Default::default(),
            )
        }
        .unwrap();
        assert_eq!(cell.into_inner().0, 6);
        let cell = unsafe {
            crate::soft_decode_unchecked::<Cell<Direct>>(
                core::cell::Cell::new(7),
                &mut Default::default(),
            )
        };
        assert_eq!(cell.into_inner().0, 7);
        // Non-Copy carriers are moved, not read through &UnsafeCell.
        let cells = crate::encode([UnsafeCell::new(Direct(8)), UnsafeCell::new(Direct(9))]);
        assert_eq!(cells.map(core::cell::UnsafeCell::into_inner), [8, 9]);
    }

    #[test]
    fn stable_shared_arrays_preserve_the_original_pointer() {
        let values = [Direct(3), Direct(4)];
        let encoded = crate::soft_encode(&values, &mut Default::default());
        assert_eq!(encoded.as_ptr().cast::<u8>(), values.as_ptr().cast::<u8>());
        let encoded = unsafe { crate::soft_encode_unchecked(&values, &mut Default::default()) };
        assert_eq!(encoded.as_ptr().cast::<u8>(), values.as_ptr().cast::<u8>());
    }

    #[test]
    fn result_niche_is_valid_for_error() {
        use core::num::NonZeroU8;
        assert!(unsafe {
            <Result<NonZeroU8, ()>>::is_valid(&crate::result::abi::ResultOk(
                NonZero(0),
                core::marker::PhantomData,
            ))
        });
        assert!(unsafe {
            <Result<NonZeroU8, ()>>::is_valid(&crate::result::abi::ResultOk(
                NonZero(1),
                core::marker::PhantomData,
            ))
        });
    }

    #[test]
    fn result_niche_is_valid_for_success() {
        use core::num::NonZeroU8;
        assert!(unsafe {
            <Result<(), NonZeroU8>>::is_valid(&crate::result::abi::ResultErr(
                NonZero(0),
                core::marker::PhantomData,
            ))
        });
        assert!(unsafe {
            <Result<(), NonZeroU8>>::is_valid(&crate::result::abi::ResultErr(
                NonZero(1),
                core::marker::PhantomData,
            ))
        });
    }

    #[test]
    fn nonrobust_checked_conversions_still_validate() {
        let mut store = Default::default();
        assert!(
            unsafe {
                crate::soft_decode::<[bool; 2]>(
                    [co3::primitive::abi::bool(1), co3::primitive::abi::bool(2)],
                    &mut store,
                )
            }
            .is_none()
        );
        assert!(
            unsafe {
                crate::soft_decode::<Cell<bool>>(
                    core::cell::Cell::new(co3::primitive::abi::bool(2)),
                    &mut Default::default(),
                )
            }
            .is_none()
        );
        let mut store = Default::default();
        let values = unsafe {
            crate::soft_decode::<[bool; 2]>(
                [co3::primitive::abi::bool(1), co3::primitive::abi::bool(0)],
                &mut store,
            )
        }
        .unwrap();
        assert_eq!(values, [true, false]);
    }

    #[test]
    fn checked_arrays_keep_mutable_reference_writeback() {
        let mut value = true;
        let mut store = Default::default();
        let encoded = crate::soft_encode([&mut value], &mut store);
        // SAFETY: The mutable carrier refers to the live conversion store.
        unsafe { encoded[0].as_ptr().write(co3::primitive::abi::bool::FALSE) };
        assert_eq!(store.sync(), Some(()));
        assert!(!value);
    }

    #[test]
    fn manually_drop_keeps_mutable_reference_writeback() {
        let mut value = true;
        let mut store = Default::default();
        let encoded = crate::soft_encode(core::mem::ManuallyDrop::new(&mut value), &mut store);
        unsafe { encoded.as_ptr().write(co3::primitive::abi::bool::FALSE) };
        assert_eq!(store.sync(), Some(()));
        assert!(!value);
    }

    #[test]
    fn unstable_arrays_keep_field_conversion_and_stores() {
        let values = [(true, 3u8), (false, 4u8)];
        let mut store = Default::default();
        let encoded = crate::soft_encode(values, &mut store);
        assert_eq!(encoded[0].0, co3::primitive::abi::bool::TRUE);
        assert_eq!(encoded[1].1, 4);
        let mut decode_store = Default::default();
        let decoded =
            unsafe { crate::soft_decode::<[(bool, u8); 2]>(encoded, &mut decode_store) }.unwrap();
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
        let moved: Owned = unsafe { crate::abi::transmute_owned(Owned(7)) };
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
        let encoded = unsafe { crate::soft_encode_unchecked([&cell], &mut store) };
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
            type CType = core::cell::UnsafeCell<u8>;
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

        impl crate::convert::Decode<'_> for Interior {}

        let decoded = unsafe {
            crate::soft_decode::<ManuallyDrop<Interior>>(
                core::mem::ManuallyDrop::new(core::cell::UnsafeCell::new(3)),
                &mut Default::default(),
            )
        }
        .unwrap();
        assert_eq!(ManuallyDrop::into_inner(decoded).0.into_inner(), 3);
        let decoded = unsafe {
            crate::soft_decode_unchecked::<ManuallyDrop<Interior>>(
                core::mem::ManuallyDrop::new(core::cell::UnsafeCell::new(4)),
                &mut Default::default(),
            )
        };
        assert_eq!(ManuallyDrop::into_inner(decoded).0.into_inner(), 4);

        // Non-robust interior still delegates to the payload's validating decoder.
        assert!(
            unsafe {
                crate::soft_decode::<ManuallyDrop<Cell<bool>>>(
                    core::mem::ManuallyDrop::new(core::cell::Cell::new(co3::primitive::abi::bool(
                        2,
                    ))),
                    &mut Default::default(),
                )
            }
            .is_none()
        );
        let decoded = unsafe {
            crate::soft_decode::<ManuallyDrop<Cell<bool>>>(
                core::mem::ManuallyDrop::new(core::cell::Cell::new(co3::primitive::abi::bool(1))),
                &mut Default::default(),
            )
        }
        .unwrap();
        assert!(ManuallyDrop::into_inner(decoded).into_inner());
    }
}
