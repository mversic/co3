//! Structures and macros related to FFI and generation of FFI bindings. Any type that implements
//! [`ExternC`] can be used in the FFI bindings generated with [`export`]/[`extern_C!`]. It
//! is advisable to implement [`Ir`] and benefit from automatic implementation of [`ExternC`]
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc as alloc_crate;
extern crate self as co3;

#[cfg(feature = "alloc")]
use alloc_crate::{borrow::ToOwned, boxed::Box, vec::Vec};

#[cfg(feature = "derive")]
pub use co3_derive::*;
use derive_more::Display;
use disjoint_impls::disjoint_impls;
// TODO: I don't like having to reexport macros from other crates
#[doc(hidden)]
pub use impls::impls;

#[cfg(feature = "alloc")]
use crate::boxed::{CBox, CBoxedSlice};
use crate::{
    borrow::BorrowCast,
    ir::{ReprFamily, Robust, Transmuted},
    niche::{Niche, NicheFamily, WithCustomNiche, WithoutNiche},
    option::COption,
    out_ptr::Zst,
    result::CResult,
    size::{MetaSized, SizeFamily, SliceLike, Thin, Wide},
    slice::{CSlice, CSliceMut},
    stored::{SoftDecodeOwned, SoftEncodeOwned, Store},
    transmute::{CheckedTransmute, transmute_into_target_dst_mut, transmute_into_target_ref_dst},
};

#[cfg(feature = "alloc")]
pub mod alloc;
pub mod borrow;
#[cfg(feature = "alloc")]
pub mod boxed;
pub mod handle;
pub mod ir;
pub mod niche;
pub mod option;
pub mod out_ptr;
mod primitives;
pub mod result;
pub mod size;
pub mod slice;
mod std_impls;
pub mod stored;
pub mod transmute;
pub mod tuple;

/// Result of execution of an FFI function
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum FfiReturn {
    /// FFI function failed during the execution of the wrapped method on the provided handle.
    ExecutionFail = -4,
    /// FFI function execution panicked.
    UnrecoverableError = -3,
    /// The input argument provided to FFI function contains a trap representation.
    TrapRepresentation = -2,
    /// Provided handle id doesn't match any known handles.
    UnknownHandle = -1,
    /// FFI function executed successfully.
    Ok = 0,
}

/// Robust type that conforms to C ABI and can be safely shared across FFI boundaries.
///
/// Note that, for raw pointers, ABI compatibility of referent is not guaranteed. Dereferencing
/// opaque/extern type pointers which don't also implement `ReprC` is very likely to cause UB.
///
/// # Safety
///
/// Type implementing the trait must be a robust type with a guaranteed C ABI. Care must be taken
/// not to dereference pointers whose referents don't implement `ReprC`; they are considered opaque
pub unsafe trait ReprC {}

/// `ReprC` type that is allowed as a C function argument.
///
/// # Safety
///
/// Type must be allowed as a C function argument type.
pub unsafe trait CFnArg: CFnReturn {}

/// `ReprC` type that is allowed as a C function return value.
///
/// # Safety
///
/// Type must be allowed as a C function return type.
pub unsafe trait CFnReturn: ReprC + Copy {}

unsafe impl<T: CFnArg> CFnReturn for T {}
unsafe impl CFnReturn for () {}

trait RobustOrTransmuted {}
impl RobustOrTransmuted for Robust {}
impl RobustOrTransmuted for Transmuted {}

/// Refer to [`SoftEncode`]
pub trait Encode: SoftEncode {
    fn encode<'itm>(self) -> Self::CType
    where
        Self: 'itm;
}

/// Refer to [`SoftDecode`]
pub trait Decode<'d>: SoftDecode<'d> {
    /// # Safety
    ///
    /// - All conversions from a pointer must ensure pointer validity beforehand
    unsafe fn decode(source: Self::CType) -> Option<Self>;
}

disjoint_impls! {
    /// A Rust type that has an `extern "C"` ABI
    pub trait ExternC {
        /// The C-compatible representation of this Rust type.
        type CType: ReprC + ?Sized;
    }

    impl<R: ReprC> ExternC for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        type CType = Self;
    }
    impl<R: CheckedTransmute<Target: ExternC>> ExternC for R
    where
        Self: ReprFamily<Kind = Transmuted>,
    {
        type CType = <R::Target as ExternC>::CType;
    }

    impl<R: ReprFamily<Kind = Robust> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized> ExternC for &R
    where
        Self: ReprFamily<Kind = Self>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: ReprC,
    {
        type CType = CSlice<R::Data>;
    }
    impl<'a, R: ReprFamily<Kind = Transmuted> + CheckedTransmute + ?Sized> ExternC for &'a R
    where
        &'a <R as CheckedTransmute>::Target: ExternC,
        Self: ReprFamily<Kind = Self>,
    {
        type CType = <&'a R::Target as ExternC>::CType;
    }
    impl<R: ReprFamily<Kind = R> + SizeFamily<Kind: Thin> + ExternC + ?Sized> ExternC for &R
    where
        Self: ReprFamily<Kind = Self>,
    {
        type CType = *const R::CType;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = R> + SizeFamily<Kind = MetaSized<K>> + ?Sized, K> ExternC for &R
    where
        R: ToOwned<Owned: ExternC<CType: BorrowCast>>,
        Self: ReprFamily<Kind = Self>,
    {
        type CType = <<R::Owned as ExternC>::CType as BorrowCast>::AsConst;
    }

    impl<R: ReprFamily<Kind = Robust> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized> ExternC
        for &mut R
    where
        Self: ReprFamily<Kind = Self>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: ReprC,
    {
        type CType = CSliceMut<R::Data>;
    }
    impl<'a, R: ReprFamily<Kind = Transmuted> + CheckedTransmute + ?Sized> ExternC for &'a mut R
    where
        &'a mut <R as CheckedTransmute>::Target: ExternC,
        Self: ReprFamily<Kind = Self>,
    {
        type CType = <&'a mut R::Target as ExternC>::CType;
    }
    impl<R: ReprFamily<Kind = R> + SizeFamily<Kind: Thin> + ExternC + ?Sized> ExternC for &mut R
    where
        Self: ReprFamily<Kind = Self>,
    {
        type CType = *mut R::CType;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = R> + SizeFamily<Kind = MetaSized<K>> + ?Sized, K> ExternC for &mut R
    where
        R: ToOwned<Owned: ExternC<CType: BorrowCast>>,
        Self: ReprFamily<Kind = Self>,
    {
        type CType = <<R::Owned as ExternC>::CType as BorrowCast>::AsMut;
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Robust> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized> ExternC
        for Box<R>
    where
        Self: ReprFamily<Kind = Self>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: ReprC,
    {
        type CType = CBoxedSlice<R::Data>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted> + CheckedTransmute + ?Sized> ExternC for Box<R>
    where
        Box<<R as CheckedTransmute>::Target>: ExternC,
        Self: ReprFamily<Kind = Self>,
    {
        type CType = <Box<R::Target> as ExternC>::CType;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = R> + SizeFamily<Kind: Thin> + ExternC> ExternC for Box<R>
    where
        Self: ReprFamily<Kind = Self>,
        <R as ExternC>::CType: Copy,
    {
        type CType = CBox<R::CType>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = R> + SizeFamily<Kind = MetaSized<K>> + ?Sized, K> ExternC for Box<R>
    where
        R: ToOwned<Owned: ExternC>,
        Self: ReprFamily<Kind = Self>,
    {
        type CType = <R::Owned as ExternC>::CType;
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Robust> + ReprC> ExternC for Vec<R>
    where
        Self: ReprFamily<Kind = Self>,
    {
        type CType = <Box<[R]> as ExternC>::CType;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted>> ExternC for Vec<R>
    where
        Self: ReprFamily<Kind = Self>,
        // TODO: rewrite bound like for ReprC
        Box<[R]>: ExternC,
    {
        type CType = <Box<[R]> as ExternC>::CType;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = R> + ExternC> ExternC for Vec<R>
    where
        Self: ReprFamily<Kind = Self>,
        <R as ExternC>::CType: Copy,
    {
        type CType = CBoxedSlice<R::CType>;
    }

    impl<R: ExternC, const N: usize> ExternC for [R; N]
    where
        Self: ReprFamily<Kind = Self>,
        <R as ExternC>::CType: Copy,
    {
        type CType = [R::CType; N];
    }

    impl<R: NicheFamily<Kind = WithoutNiche> + ExternC> ExternC for Option<R>
    where
        Self: ReprFamily<Kind = Self>,
        <R as ExternC>::CType: Copy,
    {
        type CType = COption<R::CType>;
    }
    impl<R: NicheFamily<Kind = WithCustomNiche> + Niche> ExternC for Option<R>
    where
        Self: ReprFamily<Kind = Self>,
    {
        type CType = R::CType;
    }

    impl<
        R: NicheFamily<Kind = WithoutNiche> + ExternC,
        E: NicheFamily<Kind = WithoutNiche> + ExternC,
    >
        ExternC for Result<R, E>
    where
        Self: ReprFamily<Kind = Self>,
        <R as ExternC>::CType: Copy,
        <E as ExternC>::CType: Copy,
    {
        type CType = CResult<R::CType, E::CType>;
    }
    // TODO: Implement for niche optimized Results
}

disjoint_impls! {
    /// Facilitates conversion from a Rust type into a corresponding C-compatible representation.
    pub trait SoftEncode: SoftEncodeOwned {
        /// Convert from [`Self`] into [`Self::CType`].
        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm
        {
            SoftEncodeOwned::encode(self, store)
        }
    }

    impl<R> SoftEncode for R
    where
        Self: ReprFamily<Kind = Robust> + SoftEncodeOwned,
    {}
    impl<R> SoftEncode for R
    where
        R: ReprFamily<Kind = Transmuted> + SoftEncodeOwned,
    {}

    impl<'a, R: ?Sized> SoftEncode for &'a R
    where
        Self: ReprFamily<Kind = Self> + SoftEncodeOwned,
    {}

    impl<'a, R: ?Sized> SoftEncode for &'a mut R
    where
        Self: ReprFamily<Kind = Self> + SoftEncodeOwned,
    {}

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind: RobustOrTransmuted> + ?Sized> SoftEncode for Box<R>
    where
        Self: ReprFamily<Kind = Self> + SoftEncodeOwned,
    {}

    impl<R, const N: usize> SoftEncode for [R; N]
    where
        Self: ReprFamily<Kind = Self> + SoftEncodeOwned,
    {}

    impl<R> SoftEncode for Option<R>
    where
        Self: ReprFamily<Kind = Self> + SoftEncodeOwned,
    {}

    impl<R, E> SoftEncode for Result<R, E>
    where
        Self: ReprFamily<Kind = Self> + SoftEncodeOwned,
    {}
    // TODO: Implement for niche optimized Results
}

disjoint_impls! {
    /// Facilitates conversion into a Rust type from a corresponding C-compatible representation.
    pub trait SoftDecode<'d>: SoftDecodeOwned<'d> {

        /// Perform the conversion from [`Self::CType`] into [`Self`]
        ///
        /// # Safety
        ///
        /// - All conversions from a pointer must ensure pointer validity beforehand
        /// - If `type Store = ()`, then the store **must never be dereferenced**
        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { SoftDecodeOwned::decode(source, store) }
        }
    }

    impl<'d, R> SoftDecode<'d> for R
    where
        Self: ReprFamily<Kind = Robust> + SoftDecodeOwned<'d>,
    {}
    impl<'d, R> SoftDecode<'d> for R
    where
        R: ReprFamily<Kind = Transmuted> + SoftDecodeOwned<'d>,
    {}

    impl<'d, R: ?Sized> SoftDecode<'d> for &'d R
    where
        Self: ReprFamily<Kind = Self> + SoftDecodeOwned<'d>,
    {}

    impl<'d, R: ?Sized> SoftDecode<'d> for &'d mut R
    where
        Self: ReprFamily<Kind = Self> + SoftDecodeOwned<'d>,
    {}

    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind: RobustOrTransmuted> + ?Sized> SoftDecode<'d> for Box<R>
    where
        Self: ReprFamily<Kind = Self> + SoftDecodeOwned<'d>,
    {}

    impl<'d, R, const N: usize> SoftDecode<'d> for [R; N]
    where
        Self: ReprFamily<Kind = Self> + SoftDecodeOwned<'d>,
    {}

    impl<'d, R> SoftDecode<'d> for Option<R>
    where
        Self: ReprFamily<Kind = Self> + SoftDecodeOwned<'d>,
    {}

    impl<'d, R, E> SoftDecode<'d> for Result<R, E>
    where
        Self: ReprFamily<Kind = Self> + SoftDecodeOwned<'d>,
    {}
    // TODO: Implement for niche optimized Results
}

#[cfg(feature = "alloc")]
impl<R> SoftEncode for Vec<R>
where
    Self: SoftEncodeOwned<Store = <Box<[R]> as SoftEncodeOwned>::Store>,
    Self: ExternC<CType = <Box<[R]> as ExternC>::CType>,
    Box<[R]>: SoftEncode,
{
    fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        SoftEncodeOwned::encode(self.into_boxed_slice(), store)
    }
}

#[cfg(feature = "alloc")]
impl<'d, R> SoftDecode<'d> for Vec<R>
where
    Self: SoftDecodeOwned<'d, Store = <Box<[R]> as SoftDecodeOwned<'d>>::Store>,
    Self: ExternC<CType = <Box<[R]> as ExternC>::CType>,
    Box<[R]>: SoftDecode<'d>,
{
    unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
        unsafe { <Box<[R]> as SoftDecode<'d>>::decode(source, store) }.map(Into::into)
    }
}

impl<R: SoftEncode<Store = Z>, Z: Zst + Default> Encode for R {
    fn encode<'itm>(self) -> Self::CType
    where
        Self: 'itm,
    {
        let mut store = Default::default();
        <Self as SoftEncodeOwned>::encode(self, &mut store)
    }
}

impl<'d, R, Z: Zst + Default + 'd> Decode<'d> for R
where
    R: SoftDecode<'d, Store = Z>,
{
    unsafe fn decode(source: Self::CType) -> Option<Self> {
        let mut store = Default::default();
        // SAFETY: `Decode` is only blanket-implemented for zero-sized stores, so extending the
        // borrow of the local store does not extend the lifetime of any backing data.
        let store = unsafe { core::mem::transmute::<&mut Z, &'d mut Z>(&mut store) };
        unsafe { <Self as SoftDecode>::decode(source, store) }
    }
}

/// Macro for defining FFI types of a known category ([`Robust`], [`Transmuted`] or [`Stored`]).
///
/// The implementation for an FFI type of one of the categories incurs a lot of bloat that
/// is reduced by the use of this macro
///
/// # Safety
///
/// * [`Robust`] derives [`ReprC`]. Check safety invariants for [`ReprC`]
/// * [`Transmuted`] derives [`CheckedTransmute`]. Check safety invariants for [`CheckedTransmute`]
///
/// # Example
///
/// ```
/// use co3::{
///     borrow::{EncodeAsRef, SoftDecodeView},
///     ir::{SizeFamily, SizedType},
///     reprC
/// };
///
/// #[repr(C)]
/// #[derive(Clone, Copy)]
/// struct RobustStruct(u64, i32);
///
/// #[repr(transparent)]
/// struct MyPtr<T>(*mut T);
///
/// #[repr(transparent)]
/// struct Wrapper(u32);
///
/// struct NoRepr<T: ?Sized>(u64, T);
///
/// co3::reprC! {
///     // SAFETY: Type MUST NOT have traps
///     unsafe impl Robust for RobustStruct {}
/// }
///
/// co3::reprC! {
///     // SAFETY: `Self::is_valid` must not return false posives
///     unsafe impl(T) Transmuted for MyPtr<T> where (T: Copy) {
///         type Target = *mut T;
///
///         const NICHE_VALUE: Self::CType = core::ptr::null_mut();
///         fn is_valid(target: &Self::Target) -> bool {
///             !target.is_null()
///         }
///     }
/// }
///
/// // If validation fn or niche value is given,
/// // wrapper type delegates to the inner type
/// co3::reprC! {
///     unsafe impl Transmuted for Wrapper {
///         type Target = u32;
///     }
/// }
///
/// co3::reprC! {
///     // To use this type one still has to implement
///     // a suite of additional conversion traits
///     impl(T: ?Sized) Stored for NoRepr<T> {}
/// }
///
///
/// // Some extra glue that is required:
///
/// impl<T> Drop for MyPtr<T> {
///     fn drop(&mut self) {
///         unimplemented!("Do a cleanup")
///     }
/// }
///
/// impl SizeFamily for RobustStruct {
///     type Kind = SizedType;
/// }
/// impl<T: ?Sized + SizeFamily> SizeFamily for NoRepr<T> {
///     type Kind = T::Kind;
/// }
///
/// ```
#[doc(hidden)]
#[macro_export]
macro_rules! reprC {
    (unsafe impl $(())? Robust for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        $crate::reprC! { @robust_common [for<'_dummy> Self: Copy,] [] $self_ty $([$($preds)*])? }
    };
    (unsafe impl ( $($params:tt)+ ) Robust for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        $crate::reprC! { @robust_common [Self: Copy,] [$($params)+] $self_ty $([$($preds)*])? }
    };

    (unsafe impl $(( $($params:tt)+ ))? SizedRobust for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        $crate::reprC! { @sized_size_family [$($($params)+)?] $self_ty $([$($preds)*])? }
        // TODO: Self: Copy is here just to satisfy for CFnArg. find different approach
        $crate::reprC! { @robust_common [Self: Copy,] [$($($params)+)?] $self_ty $([$($preds)*])? }
    };
    (unsafe impl $(())? SizedRobust for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        $crate::reprC! { @sized_size_family [] $self_ty $([$($preds)*])? }
        $crate::reprC! { @robust_common [] [] $self_ty $([$($preds)*])? }
    };

    (@robust_common [$($copy_bound:tt)*] [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])?) => {
        // TODO: How can Robust types implement Drop if they are Copy? ?Sized can implement Drop
        $crate::reprC! { @assert_no_drop [$($impl_generics)*] $self_ty $([$($preds)*])? }

        const _: () = {
            #[expect(dead_code)]
            trait AssertNonZst {
                fn assert_non_zst();
            }

            impl<$($impl_generics)*> AssertNonZst for $self_ty where $($copy_bound)* $($($preds)*)? {
                fn assert_non_zst() {
                    const {
                        assert!(
                            core::mem::size_of::<Self>() != 0,
                            "custom ZSTs are not supported yet"
                        );
                    }
                }
            }
        };

        unsafe impl<$($impl_generics)*> $crate::ReprC for $self_ty where $($($preds)*)? {}

        unsafe impl<$($impl_generics)*> $crate::CFnArg for $self_ty where
            $($copy_bound)*
            $($($preds)*)?
        {}

        impl<$($impl_generics)*> $crate::ir::ReprFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::ir::Robust;
        }

        impl<$($impl_generics)*> $crate::niche::NicheFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::niche::WithoutNiche;
        }

        impl<$($impl_generics)*> $crate::borrow::Borrow for $self_ty where
            $($copy_bound)*
            $($($preds)*)?
        {
            type Borrowed<'itm>
                = Self
            where
                Self: 'itm;

            type Owner = ();

            #[inline(always)]
            fn borrow<'itm>(self, (): &mut ()) -> <Self as $crate::borrow::Borrow>::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }

        impl<'itm, $($impl_generics)*> $crate::borrow::ToOwned<'itm> for $self_ty where
            $($copy_bound)*
            $($($preds)*)?
        {
            #[inline(always)]
            fn to_owned(source: Self) -> Self {
                source
            }
        }

        unsafe impl<$($impl_generics)*> $crate::handle::Erase for $self_ty $(where $($preds)*)? {
            type Erased = Self;
        }
    };

    (impl $(( $($params:tt)* ))? Stored for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        $crate::reprC! { @stored_common [$($($params)*)?] $self_ty $([$($preds)*])? {} }
    };

    (@stored_common [$($params:tt)*] $self_ty:ty $([$($preds:tt)*])? {}) => {
        impl<$($params)*> $crate::ir::ReprFamily for $self_ty $(where $($preds)*)? {
            type Kind = Self;
        }
    };

    (unsafe impl $(())? Transmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;
    }) => {
        impl $crate::size::SizeFamily for $self_ty where $($($preds)*)? {
            type Kind = <$target as $crate::size::SizeFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_delegate_niche_valid [for<'_dummy>] [for<'_dummy> Self: Sized,] [] $self_ty $([$($preds)*])? {
                type Target = $target;
            }
        }

    };
    (unsafe impl ( $($params:tt)+ ) Transmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;
    }) => {
        impl<$($params)*> $crate::size::SizeFamily for $self_ty where
            $target: $crate::size::SizeFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::size::SizeFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_delegate_niche_valid [] [Self: Sized,] [$($params)+] $self_ty $([$($preds)*])? {
                type Target = $target;
            }
        }

    };

    (unsafe impl $(())? NoDropSizedTransmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;
    }) => {
        $crate::reprC! { @no_drop_sized_transmuted [] $self_ty $([$($preds)*])? {} }

        $crate::reprC! {
            @transmuted_delegate_niche_valid [for<'_dummy>] [] [] $self_ty $([$($preds)*])? {
                type Target = $target;
            }
        }
    };
    (unsafe impl ( $($params:tt)+ ) NoDropSizedTransmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;
    }) => {
        $crate::reprC! { @no_drop_sized_transmuted [$($params)+] $self_ty $([$($preds)*])? {} }

        $crate::reprC! {
            @transmuted_delegate_niche_valid [] [] [$($params)+] $self_ty $([$($preds)*])? {
                type Target = $target;
            }
        }
    };

    (unsafe impl $(())? Transmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        impl $crate::size::SizeFamily for $self_ty where $($($preds)*)? {
            type Kind = <$target as $crate::size::SizeFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_delegate_niche_sized [] [for<'_dummy> Self: Sized,] [] $self_ty $([$($preds)*])? {
                type Target = $target;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

    };
    (unsafe impl ( $($params:tt)+ ) Transmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        impl<$($params)+> $crate::size::SizeFamily for $self_ty where
            $target: $crate::size::SizeFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::size::SizeFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_delegate_niche_sized [] [Self: Sized,] [$($params)+] $self_ty $([$($preds)*])? {
                type Target = $target;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

    };

    (unsafe impl $(())? NoDropSizedTransmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        $crate::reprC! { @no_drop_sized_transmuted [] $self_ty $([$($preds)*])? {} }

        $crate::reprC! {
            @transmuted_delegate_niche_sized [for<'_dummy>] [] [] $self_ty $([$($preds)*])? {
                type Target = $target;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }
    };
    (unsafe impl ( $($params:tt)+ ) NoDropSizedTransmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        $crate::reprC! { @no_drop_sized_transmuted [$($params)+] $self_ty $([$($preds)*])? {} }

        $crate::reprC! {
            @transmuted_delegate_niche_sized [] [] [$($params)+] $self_ty $([$($preds)*])? {
                type Target = $target;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }
    };

    (unsafe impl $(())? Transmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;
        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        impl $crate::size::SizeFamily for $self_ty where $($($preds)*)? {
            type Kind = <$target as $crate::size::SizeFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_explicit_niche [] [] $self_ty $([$($preds)*])? {
                type Target = $target;
                const NICHE_VALUE: $niche_ty = $niche_value;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

    };
    (unsafe impl ( $($params:tt)+ ) Transmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;
        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        impl<$($params)+> $crate::size::SizeFamily for $self_ty where
            $target: $crate::size::SizeFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::size::SizeFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_explicit_niche [] [$($params)+] $self_ty $([$($preds)*])? {
                type Target = $target;
                const NICHE_VALUE: $niche_ty = $niche_value;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

    };

    (unsafe impl $(())? NoDropSizedTransmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;
        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        $crate::reprC! { @no_drop_sized_transmuted [] $self_ty $([$($preds)*])? {} }

        $crate::reprC! {
            @transmuted_explicit_niche [Self: Sized,] [] $self_ty $([$($preds)*])? {
                type Target = $target;
                const NICHE_VALUE: $niche_ty = $niche_value;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }
    };
    (unsafe impl ( $($params:tt)+ ) NoDropSizedTransmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;
        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        $crate::reprC! { @no_drop_sized_transmuted [$($params)+] $self_ty $([$($preds)*])? {} }

        $crate::reprC! {
            @transmuted_explicit_niche [Self: Sized,] [$($params)+] $self_ty $([$($preds)*])? {
                type Target = $target;
                const NICHE_VALUE: $niche_ty = $niche_value;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }
    };

    (@no_drop_sized_transmuted [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])? {}) => {
        $crate::reprC! { @sized_size_family [$($impl_generics)*] $self_ty $([$($preds)*])? }
        $crate::reprC! { @assert_no_drop [$($impl_generics)*] $self_ty $([$($preds)*])? }
    };

    (@transmuted_delegate_niche_valid [$($for_dummy:tt)*] [$($sized_bound:tt)*] [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])? {
        type Target = $target:ty;
    }) => {
        $crate::reprC! {
            @transmuted_delegate_niche_sized [$($for_dummy)*] [$($sized_bound)*] [$($impl_generics)*] $self_ty $([$($preds)*])? {
                type Target = $target;
                // NOTE: When delegating there is no trap representations in the immediate `Self::Target`
                // Whether `Self::Target` itself has trap representations is not to be considered here
                fn is_valid(_target: &Self::Target) -> bool { true }
            }
        }
    };

    (@transmuted_delegate_niche_sized [$($for_dummy:tt)*] [$($sized_bound:tt)*] [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])? {
        type Target = $target:ty;
        fn is_valid($target_var:ident: $target_ty:ty) -> bool $block:block
    }) => {
        $crate::reprC! {
            @transmuted_delegate_niche [$($for_dummy)*] [$($sized_bound)*] [$($impl_generics)*] $self_ty $([$($preds)*])? {
                type Target = $target;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }
    };

    (@transmuted_delegate_niche [$($for_dummy:tt)*] [$($sized_bound:tt)*] [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])? {
        type Target = $target:ty;
        fn is_valid($target_var:ident: $target_ty:ty) -> bool $block:block
    }) => {
        impl<$($impl_generics)*> $crate::ir::ReprFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::ir::Transmuted;
        }

        impl<$($impl_generics)*> $crate::ir::EncodeReprFamily for $self_ty where
            $target: $crate::ir::EncodeReprFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::ir::EncodeReprFamily>::Kind;
        }

        impl<$($impl_generics)*> $crate::niche::NicheFamily for $self_ty where
            $($for_dummy)* $target: $crate::niche::NicheFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::niche::NicheFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_common [$($impl_generics)*] $self_ty $([$($preds)*])? {
                type Target = $target;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

        impl<$($impl_generics)*> $crate::niche::Niche for $self_ty where
            $($for_dummy)* $target: $crate::niche::Niche,
            $($sized_bound)*
            $($($preds)*)?
        {
            const NICHE_VALUE: <Self as $crate::ExternC>::CType = <$target as $crate::niche::Niche>::NICHE_VALUE;
        }

        unsafe impl<$($impl_generics)*> $crate::niche::StableNiche for $self_ty
        where
            $($for_dummy)* $target: $crate::niche::StableNiche,
            $($sized_bound)*
            $($($preds)*)?
        {}
    };

    (@transmuted_explicit_niche [$($sized_bound:tt)*] [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])? {
        type Target = $target:ty;
        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;
        fn is_valid($target_var:ident: $target_ty:ty) -> bool $block:block
    }) => {
        impl<$($impl_generics)*> $crate::ir::ReprFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::ir::Transmuted;
        }

        impl<$($impl_generics)*> $crate::ir::EncodeReprFamily for $self_ty where
            $target: $crate::ir::EncodeReprFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::ir::EncodeReprFamily>::Kind;
        }

        impl<$($impl_generics)*> $crate::niche::NicheFamily for $self_ty where
            $($sized_bound)*
            $($($preds)*)?
        {
            type Kind = $crate::niche::WithCustomNiche;
        }

        $crate::reprC! {
            @transmuted_common [$($impl_generics)*] $self_ty $([$($preds)*])? {
                type Target = $target;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

        impl<$($impl_generics)*> $crate::niche::Niche for $self_ty where
            $($sized_bound)*
            $($($preds)*)?
        {
            const NICHE_VALUE: $niche_ty = {
                assert!($crate::impls!
                    ($target: $crate::niche::NicheFamily<Kind = $crate::niche::WithoutNiche>),
                    "Transmuted CAN'T define a custom niche if target has a niche"
                );

                $niche_value
            };
        }
    };

    (@transmuted_common [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])? {
        type Target = $target:ty;
        fn is_valid($target_var:ident: $target_ty:ty) -> bool $block:block
    }) => {
        unsafe impl<$($impl_generics)*> $crate::transmute::CheckedTransmute for $self_ty $(where $($preds)*)? {
            type Target = $target;

            #[inline(always)]
            fn is_valid($target_var: $target_ty) -> bool $block
        }

        unsafe impl<$($impl_generics)*> $crate::handle::Erase for $self_ty where
            $target: $crate::handle::Erase,
            $($($preds)*)?
        {
            type Erased = <$target as $crate::handle::Erase>::Erased;
        }
    };

    (@assert_no_drop [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])?) => {
        const _: () = {
            #[expect(dead_code)]
            trait AssertNoDrop {
                fn assert_no_drop();
            }

            impl<$($impl_generics)*> AssertNoDrop for $self_ty $(where $($preds)*)? {
                fn assert_no_drop() {
                    const { assert!($crate::impls!(Self: !Drop)); }
                }
            }
        };
    };

    (@assert_no_drop [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])?) => {
        const _: () = {
            #[expect(dead_code)]
            trait AssertNoDrop {
                fn assert_no_drop();
            }

            impl<$($impl_generics)*> AssertNoDrop for $self_ty $(where $($preds)*)? {
                fn assert_no_drop() {
                    const {
                        // TODO: This is heuristic so it might
                        // make sense to reintroduce `DropFamily`
                        assert!(!core::mem::needs_drop::<Self>());
                    }
                }
            }
        };
    };

    (@sized_size_family [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])?) => {
        impl<$($impl_generics)*> $crate::size::SizeFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::size::Sized<$crate::size::SizedType>;
        }
    };
}

reprC! {
    // FIXME: ?Sized often implies fat pointers which an unknown concept in C
    // However, extern types are ?Sized and can't be destructured. We can define
    // a custom wrapper type for extern pointers which would also prevent users to access
    // the pointer or we can disptch here on the ReprFamily
    // FIXME: ReprC should be the bound on other CBox/CBoxedSlice/CSlice/etc
    unsafe impl(R: ReprC + ?Sized) SizedRobust for *const R {}
}
unsafe impl<R: ReprC + ?Sized> BorrowCast for *const R {
    type AsConst = Self;
    type AsMut = Self;
}
reprC! {
    unsafe impl(R: ReprC + ?Sized) SizedRobust for *mut R {}
}
unsafe impl<R: ReprC + ?Sized> BorrowCast for *mut R {
    type AsConst = Self;
    type AsMut = Self;
}

// SAFETY: Array is just a contiguous block of memory
unsafe impl<R: ReprC, const N: usize> ReprC for [R; N] {}

unsafe impl<R: BorrowCast, const N: usize> BorrowCast for [R; N] {
    type AsConst = [R::AsConst; N];
    type AsMut = [R::AsMut; N];
}

// TODO: Check https://github.com/mversic/co3/issues/13
const fn assert_arr_has_non_zero_len<const N: usize>() {
    assert!(N != 0, "empty array is a ZST");
}

#[cfg(test)]
mod tests {
    use static_assertions::assert_impl_all;

    use super::*;
    use crate::niche::{Niche, NicheFamily, StableNiche, WithStableNiche};

    #[test]
    fn robust_u8() {
        assert_impl_all!(u8:
            ReprFamily<Kind = Robust>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = u8>,
            SoftDecode<'static>,
            SoftEncode,
            ReprC,
        );
        assert_impl_all!(&u8:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const u8>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&mut u8:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut u8>,
            SoftDecode<'static>,
            SoftEncode,
        );
        // FIXME:
        //assert_impl_all!(Box<u8>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut u8>,
        //    SoftDecode<'static>,
        //    SoftEncode,
        //);
        assert_impl_all!(&[u8]:
            ReprFamily<Kind = &'static [u8]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&mut [u8]:
            ReprFamily<Kind = &'static mut [u8]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            SoftDecode<'static>,
            SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[u8]>:
            ReprFamily<Kind = Box<[u8]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            // FIXME:
            //SoftDecode<'static>,
            SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<u8>:
            ReprFamily<Kind = Vec<u8>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            // FIXME:
            //SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!([u8; 2]:
            ReprFamily<Kind = Robust>,
            NicheFamily<Kind = WithoutNiche>,
            SoftDecode<'static>,
            SoftEncode,
            ReprC,
        );
        assert_impl_all!(Option<u8>:
            ReprFamily<Kind = Option<u8>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = COption<u8>>,
            SoftDecode<'static>,
            SoftEncode,
        );
    }

    #[test]
    fn encode_stored_mut_ref() {
        let inner = 8u8;
        let other = 42u8;
        let mut value = Some(inner);
        let value_mut_ref: &mut Option<u8> = &mut value;
        {
            let mut store = Box::default();
            let encoded = SoftEncode::encode(value_mut_ref, &mut *store);
            unsafe {
                *encoded = COption::Some(other);
            }
            store.sync().unwrap();
        }
        assert_eq!(value, Some(42u8));

        let mut slice = [Some(1u8)];
        let ref_mut: &mut [_] = &mut slice;
        {
            let mut store = Box::default();
            let encoded = SoftEncode::encode(ref_mut, &mut *store);
            let c_slice = unsafe { encoded.into_rust().unwrap() };
            c_slice[0] = COption::Some(other);
            store.sync().unwrap();
        }
        assert_eq!(slice, [Some(42u8)]);
    }

    #[test]
    fn decode_stored_mut_ref() {
        let mut c_opt = COption::Some(1u8);
        let c_ptr: *mut _ = &mut c_opt;
        let new_val: u8 = 42;
        {
            let mut store = Box::default();
            let decoded =
                unsafe { <&mut Option<u8> as SoftDecode>::decode(c_ptr, &mut *store) }.unwrap();
            *decoded = Some(new_val);
            store.sync().unwrap();
        }
        assert_eq!(c_opt, COption::Some(42u8));

        let mut c_opts = [COption::Some(1u8)];
        let c_slice = CSliceMut::from_slice(Some(&mut c_opts));
        let x: u8 = 10;
        {
            let mut store = Box::default();
            let decoded =
                unsafe { <&mut [Option<u8>] as SoftDecode>::decode(c_slice, &mut *store) }.unwrap();
            decoded[0] = Some(x);
            store.sync().unwrap();
        }
        assert_eq!(c_opts[0], COption::Some(10u8));
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn encode_stored_ref_mut_slice() {
        use crate::tuple::CTuple1;

        let mut tuples = [(1_u32,)];
        let slice_ref: &mut [_] = &mut tuples;

        {
            let mut store = Box::default();

            let encoded = SoftEncode::encode(slice_ref, &mut store);
            let c_slice = unsafe { encoded.into_rust().unwrap() };

            c_slice[0] = CTuple1(100);
            store.sync().unwrap();
        }

        assert_eq!(tuples[0].0, 100);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn decode_stored_ref_mut_slice() {
        use crate::tuple::CTuple1;

        let mut tuples = [CTuple1(10_u32)];
        let c_slice = CSliceMut::from_slice(Some(&mut tuples));

        {
            let mut store = Box::default();
            let decoded =
                unsafe { <&mut [(_,)] as SoftDecode>::decode(c_slice, &mut store) }.unwrap();

            decoded[0].0 = 100;
            store.sync().unwrap();
        }

        assert_eq!(tuples[0].0, 100);
    }
}
