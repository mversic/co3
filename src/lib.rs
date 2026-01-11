//! Structures and macros related to FFI and generation of FFI bindings. Any type that implements
//! [`ExternC`] can be used in the FFI bindings generated with [`carbonate`]/[`decarbonate`]. It
//! is advisable to implement [`Ir`] and benefit from automatic implementation of [`ExternC`]
#![no_std]

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use core::mem::ManuallyDrop;

#[cfg(feature = "derive")]
pub use co3_derive::*;
use derive_more::Display;
use disjoint_impls::disjoint_impls;

use crate::niche::{WithCustomNiche, WithoutNiche};
#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
use crate::transmute::{
    transmute_from_target_boxed_slice, transmute_from_target_vec,
    transmute_into_target_boxed_slice, transmute_into_target_vec,
};
use crate::{
    ir::{Cloned, Ir, Opaque, Robust, Transparent},
    niche::Niche,
    slice::{CBoxedSlice, CSlice, CSliceMut},
    transmute::{
        CheckedTransmute, transmute_from_target, transmute_from_target_ref_slice,
        transmute_from_target_slice_mut, transmute_into_target, transmute_into_target_ref_slice,
        transmute_into_target_slice_mut,
    },
};

pub mod external;
pub mod handle;
pub mod ir;
pub mod niche;
pub mod option;
pub mod out_ptr;
pub mod primitives;
pub mod result;
pub mod slice;
mod std_impls;
pub mod transmute;
pub mod tuple;

use option::COption;

/// A specialized `Result` type for FFI operations
pub type Result<T> = core::result::Result<T, FfiReturn>;

disjoint_impls! {
    /// Robust type that conforms to C ABI and can be safely shared across FFI boundaries.
    ///
    /// Note that ABI compatibility of referent is not guaranteed. Dereferencing pointers
    /// whose referents don't also implement `ReprC` is very likely to cause UB
    ///
    /// # Safety
    ///
    /// Type implementing the trait must be a robust type with a guaranteed C ABI. Care must be taken
    /// not to dereference pointers whose referents don't implement `ReprC`; they are considered opaque
    // NOTE: Type is `Copy` to indicate that there can be no ownership transfer
    pub unsafe trait ReprC: Copy {}

    unsafe impl<R: CheckedTransmute<Target: Ir<Type = Robust> + ReprC> + Copy> ReprC for Option<R> {}
    unsafe impl<R: CheckedTransmute<Target: Ir<Type = Transparent>> + Copy> ReprC for Option<R>
    where
        Option<<R as CheckedTransmute>::Target>: ReprC,
    {}
}

// TODO: Check https://github.com/mversic/co3/issues/13
const fn assert_arr_has_non_zero_len<const N: usize>() {
    assert!(N != 0, "empty array is a ZST");
}

disjoint_impls! {
    /// A Rust type that has an `extern "C"` ABI
    pub trait ExternC {
        /// The C-compatible representation of this Rust type.
        type CType: ReprC;
    }

    #[cfg(feature = "owned_types")]
    impl<R: CheckedTransmute<Target: ReprC>> ExternC for R
    where
        Self: Ir<Type = Box<Robust>>,
    {
        type CType = R::Target;
    }
    impl<R: Ir<Type = Transparent> + CheckedTransmute<Target: ExternC>> ExternC for R {
        type CType = <R::Target as ExternC>::CType;
    }
    impl<R: Ir<Type = Robust> + ReprC> ExternC for R {
        type CType = Self;
    }
    impl<R: Ir<Type = Opaque>> ExternC for R {
        type CType = *mut Self;
    }

    #[cfg(feature = "cloned_refs")]
    impl<'a, R: ExternC, S: Cloned> ExternC for &'a R
    where
        Self: Ir<Type = &'a S>,
    {
        type CType = *const R::CType;
    }

    #[cfg(feature = "owned_types")]
    impl<R: ExternC, S: Cloned> ExternC for Box<R>
    where
        Self: Ir<Type = Box<S>>,
    {
        type CType = *mut R::CType;
    }

    impl<'slice, R: CheckedTransmute> ExternC for &'slice [R]
    where
        Self: Ir<Type = &'slice [Transparent]>,
        &'slice [<R as CheckedTransmute>::Target]: ExternC,
    {
        type CType = <&'slice [R::Target] as ExternC>::CType;
    }
    impl<'a, R: ReprC> ExternC for &'a [R]
    where
        Self: Ir<Type = &'a [Robust]>,
    {
        type CType = CSlice<R>;
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R> ExternC for &'a [R]
    where
        Self: Ir<Type = &'a [Opaque]>,
    {
        type CType = CSlice<*const R>;
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R: ExternC, S: Cloned> ExternC for &'a [R]
    where
        Self: Ir<Type = &'a [S]>,
    {
        type CType = CSlice<R::CType>;
    }

    impl<'slice, R: CheckedTransmute> ExternC for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Transparent]>,
        &'slice mut [<R as CheckedTransmute>::Target]: ExternC,
    {
        type CType = <&'slice mut [R::Target] as ExternC>::CType;
    }
    impl<'a, R: ReprC> ExternC for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Robust]>,
    {
        type CType = CSliceMut<R>;
    }

    impl<R: CheckedTransmute> ExternC for Box<[R]>
    where
        Self: Ir<Type = Box<[Transparent]>>,
        Box<[<R as CheckedTransmute>::Target]>: ExternC,
    {
        type CType = <Box<[R::Target]> as ExternC>::CType;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> ExternC for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        type CType = CSliceMut<R>;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R> ExternC for Box<[R]>
    where
        Self: Ir<Type = Box<[Opaque]>>,
    {
        type CType = CSliceMut<*mut R>;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ExternC, S: Cloned> ExternC for Box<[R]>
    where
        Self: Ir<Type = Box<[S]>>,
    {
        type CType = CSliceMut<R::CType>;
    }

    #[cfg(feature = "owned_types")]
    impl<R: CheckedTransmute> ExternC for Vec<R>
    where
        Self: Ir<Type = Vec<Transparent>>,
        Vec<<R as CheckedTransmute>::Target>: ExternC,
    {
        type CType = <Vec<R::Target> as ExternC>::CType;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> ExternC for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        type CType = CSliceMut<R>;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R> ExternC for Vec<R>
    where
        Self: Ir<Type = Vec<Opaque>>,
    {
        type CType = CSliceMut<*mut R>;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ExternC, S: Cloned> ExternC for Vec<R>
    where
        Self: Ir<Type = Vec<S>>,
    {
        type CType = CSliceMut<R::CType>;
    }

    impl<R, const N: usize> ExternC for [R; N]
    where
        Self: Ir<Type = [Opaque; N]>,
    {
        type CType = [*mut R; N];
    }
    impl<R: ExternC, S: Cloned, const N: usize> ExternC for [R; N]
    where
        Self: Ir<Type = [S; N]>,
    {
        type CType = [R::CType; N];
    }

    impl<R: ExternC> ExternC for Option<R>
    where
        Self: Ir<Type = Option<WithoutNiche>>,
    {
        type CType = COption<R::CType>;
    }
    impl<R: Niche> ExternC for Option<R>
    where
        Self: Ir<Type = Option<WithCustomNiche>>,
    {
        type CType = <R as ExternC>::CType;
    }
}

disjoint_impls! {
    /// Facilitates conversion from a Rust type into a corresponding C-compatible representation.
    pub trait Encode: ExternC {
        /// Auxiliary storage used during conversion. If storage is not used, set the type to `()`.
        ///
        /// Use cases include:
        /// - Keeping the result of the conversion of references of [`Cloned`] types
        /// - Keeping the reference alive while converting heap-allocated types
        ///
        /// Conceptually, serves a role similar to the "context" captured by a closure.
        type Store: Default;

        /// Convert from [`Self`] into [`Self::CType`]
        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm;
    }

    //#[cfg(feature = "owned_types")]
    //#[cfg(feature = "owned_as_ref")]
    //impl<R: CheckedTransmute<Target: ReprC>> Encode for R
    //where
    //    Self: Ir<Type = Box<Robust>>,
    //    for<'a> &'a mut <R as CheckedTransmute>::Target: Encode,
    //{
    //    type Store = Option<R>;

    //    fn encode<'itm>(self, store: &mut Self::Store) -> Self::CType where Self: 'itm {
    //        *Encode::encode(store.insert(self), &mut ())
    //    }
    //}
    impl<R: Ir<Type = Transparent> + CheckedTransmute<Target: Encode>> Encode for R {
        type Store = <R::Target as Encode>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            transmute_into_target(self).encode(store)
        }
    }
    impl<R: Ir<Type = Robust> + ReprC> Encode for R {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            self
        }
    }
    impl<R: Ir<Type = Opaque>> Encode for R {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            Box::into_raw(Box::new(self))
        }
    }

    #[cfg(feature = "cloned_refs")]
    impl<'a, R: Encode + Clone, S: Cloned> Encode for &'a R
    where
        Self: Ir<Type = &'a S>,
    {
        type Store = (Option<R::CType>, R::Store);

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            store.0.insert(self.clone().encode(&mut store.1))
        }
    }

    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Encode, S: Cloned> Encode for Box<R>
    where
        Self: Ir<Type = Box<S>>,
    {
        type Store = (Option<R::CType>, R::Store);

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            store.0.insert((*self).encode(&mut store.1))
        }
    }

    impl<'slice, R: CheckedTransmute> Encode for &'slice [R]
    where
        Self: Ir<Type = &'slice [Transparent]>,
        &'slice [<R as CheckedTransmute>::Target]: Encode,
    {
        type Store = <&'slice [R::Target] as Encode>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            transmute_into_target_ref_slice(self).encode(store)
        }
    }
    impl<'slice, R: ReprC> Encode for &'slice [R]
    where
        Self: Ir<Type = &'slice [Robust]>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            CSlice::from_slice(Some(self))
        }
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R> Encode for &'slice [R]
    where
        Self: Ir<Type = &'slice [Opaque]>,
    {
        type Store = Box<[*const R]>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            *store = self.iter().map(core::ptr::from_ref).collect();
            CSlice::from_slice(Some(store))
        }
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R: Encode + Clone, S: Cloned> Encode for &'slice [R]
    where
        Self: Ir<Type = &'slice [S]>,
    {
        type Store = (Box<[R::CType]>, Box<[R::Store]>);

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let slice = self.to_vec();

            store.1 = core::iter::repeat_with(Default::default)
                .take(slice.len())
                .collect();

            store.0 = slice
                .into_iter()
                .zip(&mut *store.1)
                .map(|(item, substore)| item.encode(substore))
                .collect();

            CSlice::from_slice(Some(&store.0))
        }
    }

    impl<'slice, R: CheckedTransmute> Encode for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Transparent]>,
        &'slice mut [<R as CheckedTransmute>::Target]: Encode,
    {
        type Store = <&'slice mut [R::Target] as Encode>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            transmute_into_target_slice_mut(self).encode(store)
        }
    }
    impl<'slice, R: ReprC> Encode for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Robust]>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            CSliceMut::from_slice(Some(self))
        }
    }

    #[cfg(feature = "owned_types")]
    impl<R: CheckedTransmute> Encode for Box<[R]>
    where
        Box<[<R as CheckedTransmute>::Target]>: Encode,
        Self: Ir<Type = Box<[Transparent]>>,
    {
        type Store = <Box<[R::Target]> as Encode>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            transmute_into_target_boxed_slice(self).encode(store)
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> Encode for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        type Store = Self;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            *store = self;
            CSliceMut::from_slice(Some(store))
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R> Encode for Box<[R]>
    where
        Self: Ir<Type = Box<[Opaque]>>,
    {
        type Store = Box<[*mut R]>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            *store = Vec::from(self)
                .into_iter()
                .map(Box::new)
                .map(Box::into_raw)
                .collect();

            CSliceMut::from_slice(Some(store))
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Encode, S: Cloned> Encode for Box<[R]>
    where
        Self: Ir<Type = Box<[S]>>,
    {
        type Store = (Box<[R::CType]>, Box<[R::Store]>);

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let boxed_slice = self;

            store.1 = core::iter::repeat_with(Default::default)
                .take(boxed_slice.len())
                .collect();

            store.0 = Vec::from(boxed_slice)
                .into_iter()
                .zip(&mut *store.1)
                .map(|(item, substore)| item.encode(substore))
                .collect();

            CSliceMut::from_slice(Some(&mut store.0))
        }
    }

    #[cfg(feature = "owned_types")]
    impl<R: CheckedTransmute> Encode for Vec<R>
    where
        Vec<<R as CheckedTransmute>::Target>: Encode,
        Self: Ir<Type = Vec<Transparent>>,
    {
        type Store = <Vec<R::Target> as Encode>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            transmute_into_target_vec(self).encode(store)
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> Encode for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        type Store = Box<[R]>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            *store = self.into_boxed_slice();
            CSliceMut::from_slice(Some(store))
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R> Encode for Vec<R>
    where
        Self: Ir<Type = Vec<Opaque>>,
    {
        type Store = Box<[*mut R]>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            *store = self.into_iter().map(Box::new).map(Box::into_raw).collect();
            CSliceMut::from_slice(Some(store))
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Encode, S: Cloned> Encode for Vec<R>
    where
        Self: Ir<Type = Vec<S>>,
    {
        type Store = (Box<[R::CType]>, Box<[R::Store]>);

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let vec = self;

            store.1 = core::iter::repeat_with(Default::default)
                .take(vec.len())
                .collect();

            store.0 = vec
                .into_iter()
                .zip(&mut *store.1)
                .map(|(item, substore)| item.encode(substore))
                .collect();

            CSliceMut::from_slice(Some(&mut store.0))
        }
    }

    impl<R, const N: usize> Encode for [R; N]
    where
        Self: Ir<Type = [Opaque; N]>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            assert_arr_has_non_zero_len::<N>();

            let array = self
                .into_iter()
                .map(Box::new)
                .map(Box::into_raw)
                .collect::<Vec<_>>()
                .try_into();

            // SAFETY: Vec<T> length is N
            unsafe { array.unwrap_unchecked() }
        }
    }
    impl<R: Encode, S: Cloned, const N: usize> Encode for [R; N]
    where
        // FIXME: https://github.com/rust-lang/rust/issues/61415
        [<R as Encode>::Store; N]: Default,
        Self: Ir<Type = [S; N]>,
    {
        type Store = [R::Store; N];

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            fn default_init_arr<R: Default, const N: usize>() -> [R; N] {
                assert_arr_has_non_zero_len::<N>();

                let vec = core::iter::repeat_with(Default::default)
                    .take(N)
                    .collect::<Vec<_>>();

                // SAFETY: Vec<T> length is N
                unsafe { TryFrom::try_from(vec).unwrap_unchecked() }
            }

            assert_arr_has_non_zero_len::<N>();
            *store = default_init_arr();

            let array = self
                .into_iter()
                .zip(store.iter_mut())
                .map(|(item, substore)| item.encode(substore))
                .collect::<Vec<_>>()
                .try_into();

            // SAFETY: Vec<T> length is N
            unsafe { array.unwrap_unchecked() }
        }
    }

    //impl<R> Encode for R
    //where
    //    Self: Ir<Type = Option<Box<Robust>>>,
    //{
    //    type Store = ();

    //    fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
    //        unimplemented!();
    //        //// SAFETY: Guaranteed by [`StableNiche`]
    //        //let inner = unsafe {
    //        //    core::mem::transmute::<R, R::Target>(self)
    //        //};
    //    }
    //}
    impl<R: Encode> Encode for Option<R>
    where
        Self: Ir<Type = Option<WithoutNiche>>,
    {
        type Store = <R as Encode>::Store;

        fn encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                // SAFETY: `ReprC` type is robust and can't have any trap representations
                // TODO: No need to zero the memory because it must never be read. Use MaybeUninit?
                None => COption {
                    tag: 0,
                    payload: unsafe { core::mem::zeroed() },
                },
                Some(value) => COption {
                    tag: 1,
                    payload: value.encode(store),
                },
            }
        }
    }
    impl<R: Niche + Encode> Encode for Option<R>
    where
        Self: Ir<Type = Option<WithCustomNiche>>,
    {
        type Store = <R as Encode>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            if let Some(value) = self {
                return value.encode(store);
            }

            R::NICHE_VALUE
        }
    }
}

disjoint_impls! {
    /// Facilitates conversion into a Rust type from a corresponding C-compatible representation.
    pub trait Decode<'d>: ExternC<CType: 'd> + Sized {
        /// Auxiliary storage used during conversion. If storage is not used, set the type to `()`.
        ///
        /// Use cases include:
        /// - Storing the result of the conversion of references of [`Cloned`] types
        /// - Keeping the reference alive while converting heap-allocated types
        ///
        /// Conceptually, serves a role similar to the "context" captured by a closure.
        type Store: Default;

        /// Perform the conversion from [`Self::CType`] into [`Self`]
        ///
        /// # Errors
        ///
        /// Check [`FfiReturn`]
        ///
        /// # Safety
        ///
        /// - All conversions from a pointer must ensure pointer validity beforehand
        /// - If `type Store = ()`, then the store **must never be dereferenced**
        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self>;
    }

    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: CheckedTransmute<Target: ReprC + 'd> + Clone> Decode<'d> for R
    where
        Self: Ir<Type = Box<Robust>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Result<Self> {
            transmute_from_target::<&R>(&source).cloned()
        }
    }
    impl<'d, R: CheckedTransmute> Decode<'d> for R
    where
        <Self as CheckedTransmute>::Target: Decode<'d>,
        Self: Ir<Type = Transparent>,
    {
        type Store = <R::Target as Decode<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            unsafe { Decode::decode(source, store).and_then(|inner| transmute_from_target(inner)) }
        }
    }
    impl<'d, R: ReprC + 'd> Decode<'d> for R
    where
        Self: Ir<Type = Robust>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Result<Self>{
            Ok(source)
        }
    }
    impl<'d, R: 'd> Decode<'d> for R
    where
        Self: Ir<Type = Opaque>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Result<Self> {
            if source.is_null() {
                return Err(FfiReturn::ArgIsNull);
            }

            Ok(*unsafe { Box::from_raw(source) })
        }
    }

    #[cfg(feature = "cloned_refs")]
    impl<'d, R: Decode<'d> + Clone, S: Cloned> Decode<'d> for &'d R
    where
        Self: Ir<Type = &'d S>,
    {
        type Store = (Option<R>, R::Store);

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            unsafe {
                if source.as_ref().is_none() {
                    return Err(FfiReturn::ArgIsNull);
                }

                Ok(store.0.insert(
                    R::decode(source.read(), &mut store.1)
                        .map(ManuallyDrop::new)
                        .map(|item| (*item).clone())?,
                ))
            }
        }
    }

    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: Decode<'d> + Clone, S: Cloned> Decode<'d> for Box<R>
    where
        Self: Ir<Type = Box<S>>,
    {
        type Store = R::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            if source.is_null() {
                return Err(FfiReturn::ArgIsNull);
            }

            unsafe { R::decode(source.read(), store) }
                    .map(ManuallyDrop::new)
                    .map(|item| (*item).clone())
                    .map(Box::new)
        }
    }

    impl<'slice, R: CheckedTransmute> Decode<'slice> for &'slice [R]
    where
        &'slice [<R as CheckedTransmute>::Target]: Decode<'slice>,
        Self: Ir<Type = &'slice [Transparent]>,
    {
        type Store = <&'slice [R::Target] as Decode<'slice>>::Store;

        unsafe fn decode<'itm: 'slice>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            unsafe {
                let slice = <&[R::Target]>::decode(source, store)?;
                transmute_from_target_ref_slice(slice)
            }
        }
    }
    impl<'slice, R: ReprC> Decode<'slice> for &'slice [R]
    where
        Self: Ir<Type = &'slice [Robust]>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'slice>(source: Self::CType, (): &mut ()) -> Result<Self> {
            unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)
        }
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R: Clone> Decode<'slice> for &'slice [R]
    where
        Self: Ir<Type = &'slice [Opaque]>,
    {
        type Store = Box<[R]>;

        unsafe fn decode<'itm: 'slice>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            let source = unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)?;

            *store = source
                .iter()
                .map(|item| {
                    unsafe { item.as_ref() }
                        // TODO: This function clones every opaque pointer in the slice. This could
                        // be avoided with the entire slice being opaque, if that even makes sense.
                        // If the entire slice is opaque then `ExternC` can also be implemented for
                        // `&mut [Opaque]`
                        .cloned()
                        .ok_or(FfiReturn::ArgIsNull)
                })
                .collect::<core::result::Result<_, _>>()?;

            Ok(store)
        }
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R: Decode<'slice> + Clone, S: Cloned> Decode<'slice> for &'slice [R]
    where
        Self: Ir<Type = &'slice [S]>,
    {
        type Store = (Box<[R]>, Box<[R::Store]>);

        unsafe fn decode<'itm: 'slice>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            let source = unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)?;

            store.1 = core::iter::repeat_with(Default::default)
                .take(source.len())
                .collect();

            let slice: Box<[_]> = source
                .iter()
                .zip(&mut *store.1)
                .map(|(&item, substore)| unsafe { R::decode(item, substore) }.map(ManuallyDrop::new))
                .collect::<core::result::Result<_, _>>()?;

            store.0 = slice
                .iter()
                .cloned()
                .map(ManuallyDrop::into_inner)
                .collect();

            Ok(&store.0)
        }
    }

    impl<'slice, R: CheckedTransmute> Decode<'slice> for &'slice mut [R]
    where
        &'slice mut [<R as CheckedTransmute>::Target]: Decode<'slice>,
        Self: Ir<Type = &'slice mut [Transparent]>,
    {
        type Store = <&'slice mut [R::Target] as Decode<'slice>>::Store;

        unsafe fn decode<'itm: 'slice>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            unsafe {
                <&mut [R::Target]>::decode(source, store)
                    .and_then(|output| transmute_from_target_slice_mut(output))
            }
        }
    }
    impl<'slice, R: ReprC> Decode<'slice> for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Robust]>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'slice>(source: Self::CType, (): &mut ()) -> Result<Self> {
            unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)
        }
    }

    #[cfg(feature = "owned_types")]
    impl<'d, R: CheckedTransmute> Decode<'d> for Box<[R]>
    where
        Box<[<R as CheckedTransmute>::Target]>: Decode<'d>,
        Self: Ir<Type = Box<[Transparent]>>,
    {
        type Store = <Box<[R::Target]> as Decode<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            unsafe {
                <Box<[R::Target]>>::decode(source, store)
                    .and_then(|output| transmute_from_target_boxed_slice(output))
            }
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: ReprC + 'd> Decode<'d> for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Result<Self> {
            unsafe { source.into_rust() }
                .ok_or(FfiReturn::ArgIsNull)
                .map(|slice| slice.into())
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: 'd> Decode<'d> for Box<[R]>
    where
        Self: Ir<Type = Box<[Opaque]>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Result<Self> {
            let slice = unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)?;

            slice
                .iter()
                .map(|&item| unsafe {
                    if let Some(item) = item.as_mut() {
                        return Ok(*Box::from_raw(item));
                    }

                    Err(FfiReturn::ArgIsNull)
                })
                .collect::<core::result::Result<_, _>>()
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: Decode<'d> + Clone, S: Cloned> Decode<'d> for Box<[R]>
    where
        Self: Ir<Type = Box<[S]>>,
    {
        type Store = Box<[R::Store]>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            let slice = unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)?;

            *store = core::iter::repeat_with(Default::default)
                .take(slice.len())
                .collect();

            let vec: Box<[_]> = slice
                .iter()
                .copied()
                .zip(&mut **store)
                .map(|(item, substore)| unsafe { R::decode(item, substore) }.map(ManuallyDrop::new))
                .collect::<core::result::Result<_, _>>()?;

            Ok(vec.iter().cloned().map(ManuallyDrop::into_inner).collect())
        }
    }

    #[cfg(feature = "owned_types")]
    impl<'d, R: CheckedTransmute> Decode<'d> for Vec<R>
    where
        Vec<<R as CheckedTransmute>::Target>: Decode<'d>,
        Self: Ir<Type = Vec<Transparent>>,
    {
        type Store = <Vec<R::Target> as Decode<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            unsafe {
                <Vec<R::Target>>::decode(source, store)
                    .and_then(|output| transmute_from_target_vec(output))
            }
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: ReprC + 'd> Decode<'d> for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Result<Self> {
            unsafe { source.into_rust() }
                .ok_or(FfiReturn::ArgIsNull)
                .map(|slice| slice.to_vec())
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: 'd> Decode<'d> for Vec<R>
    where
        Self: Ir<Type = Vec<Opaque>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Result<Self> {
            let slice = unsafe { source.into_rust() };

            slice
                .ok_or(FfiReturn::ArgIsNull)?
                .iter()
                .map(|&item| unsafe {
                    if let Some(item) = item.as_mut() {
                        return Ok(*Box::from_raw(item));
                    }

                    Err(FfiReturn::ArgIsNull)
                })
                .collect::<core::result::Result<_, _>>()
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: Decode<'d> + Clone, S: Cloned> Decode<'d> for Vec<R>
    where
        Self: Ir<Type = Vec<S>>,
    {
        type Store = Box<[R::Store]>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            let slice = unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)?;

            *store = core::iter::repeat_with(Default::default)
                .take(slice.len())
                .collect();

            let vec: Box<[_]> = slice
                .iter()
                .copied()
                .zip(&mut **store)
                .map(|(item, substore)| unsafe { R::decode(item, substore).map(ManuallyDrop::new) })
                .collect::<core::result::Result<_, _>>()?;

            Ok(vec.iter().cloned().map(ManuallyDrop::into_inner).collect())
        }
    }

    impl<'d, R: 'd, const N: usize> Decode<'d> for [R; N]
    where
        Self: Ir<Type = [Opaque; N]>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Result<Self> {
            assert_arr_has_non_zero_len::<N>();

            let array = source
                .into_iter()
                .map(|item| unsafe {
                    if let Some(item) = item.as_mut() {
                        return Ok(*Box::from_raw(item));
                    }

                    Err(FfiReturn::ArgIsNull)
                })
                .collect::<core::result::Result<Vec<_>, _>>()?
                .try_into();

            Ok(unsafe { array.unwrap_unchecked() })
        }
    }
    impl<'d, R: Decode<'d> + Clone, S: Cloned, const N: usize> Decode<'d> for [R; N]
    where
        // FIXME: https://github.com/rust-lang/rust/issues/61415
        [<R as Decode<'d>>::Store; N]: Default,
        Self: Ir<Type = [S; N]>,
    {
        type Store = [R::Store; N];

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            assert_arr_has_non_zero_len::<N>();

            let vec: core::result::Result<[_; N], _> = source
                .into_iter()
                .zip(store.iter_mut())
                .map(|(item, substore)| unsafe { R::decode(item, substore).map(ManuallyDrop::new) })
                .collect::<core::result::Result<Vec<_>, FfiReturn>>()?
                .try_into();

            let array = unsafe { vec.unwrap_unchecked() }
                .iter()
                .cloned()
                .map(ManuallyDrop::into_inner)
                .collect::<Vec<_>>()
                .try_into();

            Ok(unsafe { array.unwrap_unchecked() })
        }
    }

    impl<'d, R: Decode<'d>> Decode<'d> for Option<R>
    where
        Self: Ir<Type = Option<WithoutNiche>>,
    {
        type Store = <R as Decode<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            match source.tag {
                0 => Ok(None),
                1 => Ok(Some(unsafe { R::decode(source.payload, store) }?)),
                _ => Err(FfiReturn::TrapRepresentation),
            }
        }
    }
    impl<'d, R: Niche<CType: PartialEq> + Decode<'d>> Decode<'d> for Option<R>
    where
        Self: Ir<Type = Option<WithCustomNiche>>,
    {
        type Store = <R as Decode<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            if source == R::NICHE_VALUE {
                return Ok(None);
            }

            Ok(Some(unsafe { R::decode(source, store) }?))
        }
    }
}

/// Result of execution of an FFI function
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum FfiReturn {
    /// The input argument provided to FFI function can't be converted into inner rust representation.
    ConversionFailed = -6,
    /// The input argument provided to FFI function contains a trap representation.
    TrapRepresentation = -5,
    /// FFI function execution panicked.
    UnrecoverableError = -4,
    /// Provided handle id doesn't match any known handles.
    UnknownHandle = -3,
    /// FFI function failed during the execution of the wrapped method on the provided handle.
    ExecutionFail = -2,
    /// The input argument provided to FFI function is a null pointer.
    ArgIsNull = -1,
    /// FFI function executed successfully.
    Ok = 0,
}

/// Macro for defining FFI types of a known category ([`Robust`] or [`CheckedTransmute`]).
///
/// The implementation for an FFI type of one of the categories incurs a lot of bloat that
/// is reduced by the use of this macro
///
/// # Safety
///
/// * If the type is [`Robust`], it derives [`ReprC`]. Check safety invariants for [`ReprC`]
/// * If the type is [`Transparent`], it derives [`CheckedTransmute`]. Check safety invariants for [`CheckedTransmute`]
///
/// # Example
///
/// ```
/// use co3::ReprC;
///
/// // Always use a type alias for inner types of transparent items so that if you make
/// // a change the unsafe code in [`co3::mineral!`] will not compile, thus preventing UB
/// type NonNullInner<T> = *mut T;
/// type WrapperInner = u32;
///
/// #[repr(transparent)]
/// struct NonNull<T>(NonNullInner<T>);
///
/// #[repr(transparent)]
/// struct Wrapper(WrapperInner);
///
/// #[derive(Clone, Copy)]
/// #[repr(C)]
/// struct RobustStruct(u64, i32);
///
/// co3::mineral! {
///     // SAFETY: Type MUST NOT have traps
///     unsafe impl Robust for RobustStruct {}
/// }
///
/// co3::mineral! {
///     // SAFETY: `Self::is_valid` must not return false posives
///     unsafe impl(T) Transparent for NonNull<T> where (T: Copy) {
///         type Target = NonNullInner<T>;
///
///         const NICHE_VALUE: Self::CType = core::ptr::null_mut();
///         fn is_valid(target: &Self::Target) -> bool {
///             !target.is_null()
///         }
///     }
/// }
///
/// // If no validation function or niche value is given,
/// // wrapper type delegates to the inner type
/// co3::mineral! {
///     unsafe impl Transparent for Wrapper {
///         type Target = WrapperInner;
///     }
/// }
/// ```
#[macro_export]
macro_rules! mineral {
    (unsafe impl $(( $($params:tt)* ))? Robust for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        unsafe impl$(<$($params)*>)? $crate::ReprC for $self_ty $(where $($preds)*)? {}

        impl$(<$($params)*>)? $crate::ir::Ir for $self_ty $(where $($preds)*)? {
            type Type = $crate::ir::Robust;
        }
        impl $(<$($params)*>)? $crate::niche::Ir for $self_ty $(where $($preds)*)? {
            type Type = $crate::niche::WithoutNiche;
        }
    };
    (unsafe impl $(( $($params:tt)* ))? Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;
        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        impl $(<$($params)*>)? $crate::ir::Ir for $self_ty $(where $($preds)*)? {
            type Type = $crate::ir::Transparent;
        }

        // SAFETY: `$ty` is transmutable into `$target` and `is_valid` doesn't return false positives
        unsafe impl $(<$($params)*>)? $crate::transmute::CheckedTransmute for $self_ty $(where $($preds)*)? {
            type Target = $target;

            #[inline(always)]
            fn is_valid($target_var: $target_ty) -> bool $block
        }

        impl $(<$($params)*>)? $crate::niche::Niche for $self_ty $(where $($preds)*)? {
            const NICHE_VALUE: $niche_ty = {
                debug_assert!(impls::impls!
                    // TODO: This introduces a dependency, can we do without?
                    // and it also adds checks for internal types like `NonZeroU8`
                    ($target: $crate::niche::Ir<Type = $crate::niche::WithoutNiche>),
                    "Transparent CAN'T define a custom niche if target has a niche"
                );

                $niche_value
            };
        }

        impl $(<$($params)*>)? $crate::niche::Ir for $self_ty $(where $($preds)*)? {
            type Type = $crate::niche::WithCustomNiche;
        }
    };
    (unsafe impl Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;
    }) => {
        $crate::mineral! {
            @transparent [for<'_dummy>] [] $self_ty $([$($preds)*])? {
                type Target = $target;
                // NOTE: When delegating there is no trap representations in the immediate `Self::Target`
                // Whether `Self::Target` itself has trap representations is not to be considered here
                fn is_valid(_target: &Self::Target) -> bool { true }
            }
        }

        unsafe impl $crate::ReprC for $self_ty where for<'_dummy> Self: Copy, for<'_dummy> $target: $crate::ReprC, $($($preds)*)? {}
    };
    (unsafe impl ( $($params:tt)* ) Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;
    }) => {
        $crate::mineral! {
            @transparent [] [<$($params)*>] $self_ty $([$($preds)*])? {
                type Target = $target;
                // NOTE: When delegating there is no trap representations in the immediate `Self::Target`
                // Whether `Self::Target` itself has trap representations is not to be considered here
                fn is_valid(_target: &Self::Target) -> bool { true }
            }
        }

        unsafe impl<$($params)*> $crate::ReprC for $self_ty where Self: Copy, $target: $crate::ReprC, $($($preds)*)? {}
    };
    (unsafe impl Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        $crate::mineral! {
            @transparent [for<'_dummy>] [] $self_ty $([$($preds)*])? {
                type Target = $target;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }
    };
    (unsafe impl ( $($params:tt)* ) Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        $crate::mineral! {
            @transparent [] [<$($params)*>] $self_ty $([$($preds)*])? {
                type Target = $target;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }
    };
    (@transparent [$($for_dummy:tt)?] [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])? {
        type Target = $target:ty;
        fn is_valid($target_var:ident: $target_ty:ty) -> bool $block:block
    }) => {
        impl $($impl_generics)* $crate::ir::Ir for $self_ty $(where $($preds)*)? {
            type Type = $crate::ir::Transparent;
        }

        unsafe impl $($impl_generics)* $crate::transmute::CheckedTransmute for $self_ty $(where $($preds)*)? {
            type Target = $target;

            #[inline(always)]
            fn is_valid($target_var: $target_ty) -> bool $block
        }

        impl $($impl_generics)* $crate::niche::Ir for $self_ty where $($for_dummy)? $target: $crate::niche::Ir, $($($preds)*)? {
            type Type = <$target as $crate::niche::Ir>::Type;
        }

        impl $($impl_generics)* $crate::niche::Niche for $self_ty where $($for_dummy)? $target: $crate::niche::Niche, $($($preds)*)? {
            const NICHE_VALUE: <Self as $crate::ExternC>::CType = <$target as $crate::niche::Niche>::NICHE_VALUE;
        }

        unsafe impl $($impl_generics)* $crate::niche::StableNiche for $self_ty where $($for_dummy)? $target: $crate::niche::StableNiche, $($($preds)*)? {}
    };
}

mineral! {
    unsafe impl(R) Robust for *const R {}
}
mineral! {
    unsafe impl(R) Robust for *mut R {}
}

// SAFETY: Arrays is just a contiguous block of memory
unsafe impl<R: ReprC, const N: usize> ReprC for [R; N] {}

#[cfg(test)]
mod tests {
    use crate::transmute::FlatTransmute;

    use super::*;

    use static_assertions::assert_impl_all;

    #[test]
    fn impls() {
        assert_impl_all!(u8: Ir<Type = Robust>, FlatTransmute<CType = u8>, ReprC);
        assert_impl_all!(&u8: Ir<Type = Transparent>, CheckedTransmute<Target = *const u8>, FlatTransmute<CType = *const u8>);
        assert_impl_all!(&mut u8: Ir<Type = Transparent>, CheckedTransmute<Target = *mut u8>, FlatTransmute<CType = *mut u8>);
        //assert_impl_all!(Box<u8>: Ir<Type = Transparent>, FlatTransmute<CType = *mut u8>);
        assert_impl_all!(&[u8]: Ir<Type = &'static [Robust]>, ExternC<CType = CSlice<u8>>);
        assert_impl_all!(&mut [u8]: Ir<Type = &'static mut [Robust]>, ExternC<CType = CSliceMut<u8>>);
        assert_impl_all!(Box<[u8]>: Ir<Type = Box<[Robust]>>, ExternC<CType = CSliceMut<u8>>);
        assert_impl_all!(Vec<u8>: Ir<Type = Vec<Robust>>, ExternC<CType = CSliceMut<u8>>);
        assert_impl_all!([u8; 2]: Ir<Type = Robust>, ExternC<CType = [u8; 2]>);
        assert_impl_all!(Option<u8>: Ir<Type = Option<WithoutNiche>>, ExternC<CType = COption<u8>>);
    }
}
