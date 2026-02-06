//! Structures and macros related to FFI and generation of FFI bindings. Any type that implements
//! [`ExternC`] can be used in the FFI bindings generated with [`carbonate`]/[`decarbonate`]. It
//! is advisable to implement [`Ir`] and benefit from automatic implementation of [`ExternC`]
#![no_std]

extern crate alloc;

extern crate self as co3;

use alloc::{boxed::Box, vec::Vec};
use core::mem::ManuallyDrop;

#[cfg(feature = "derive")]
pub use co3_derive::*;
use derive_more::Display;
use disjoint_impls::disjoint_impls;

#[cfg(feature = "cloned_refs")]
use crate::out_ptr::NonLocal;
#[cfg(not(feature = "non_robust_ref_mut"))]
use crate::transmute::MutSafe;
#[cfg(feature = "owned_as_ref")]
use crate::transmute::{
    transmute_from_target_boxed_slice, transmute_from_target_vec,
    transmute_into_target_boxed_slice, transmute_into_target_vec,
};
#[cfg(not(feature = "owned_as_ref"))]
use crate::vec::CVec;
use crate::{
    ir::{Cloned, Opaque, ReprFamily, Robust, Transmuted},
    niche::{Niche, StableNiche, WithCustomNiche, WithoutNiche},
    slice::{CBoxedSlice, CSlice, CSliceMut},
    transmute::{
        CheckedTransmute, FlatTransmute, transmute_from_target, transmute_from_target_ref_slice,
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
pub mod vec;

use option::COption;

#[cfg(feature = "owned_as_ref")]
type BoxedSliceCType<C> = CSliceMut<C>;
#[cfg(not(feature = "owned_as_ref"))]
type BoxedSliceCType<C> = CBoxedSlice<C>;

#[cfg(feature = "owned_as_ref")]
type VecCType<C> = CSliceMut<C>;
#[cfg(not(feature = "owned_as_ref"))]
type VecCType<C> = CVec<C>;

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
/// raw pointers whose referents don't also implement `ReprC` is very likely to cause UB
///
/// # Safety
///
/// Type implementing the trait must be a robust type with a guaranteed C ABI. Care must be taken
/// not to dereference pointers whose referents don't implement `ReprC`; they are considered opaque
// NOTE: Type is `Copy` to indicate that there can be no ownership transfer
pub unsafe trait ReprC: Copy {}

disjoint_impls! {
    /// A Rust type that has an `extern "C"` ABI
    pub trait ExternC {
        /// The C-compatible representation of this Rust type.
        type CType: ReprC;
    }

    #[cfg(feature = "owned_types")]
    impl<R: CheckedTransmute<Target: ReprC>> ExternC for R
    where
        Self: ReprFamily<Kind = Box<Robust>>,
    {
        type CType = R::Target;
    }
    impl<R: ReprFamily<Kind = Transmuted> + CheckedTransmute<Target: ExternC>> ExternC for R {
        type CType = <R::Target as ExternC>::CType;
    }
    impl<R: ReprFamily<Kind = Robust> + ReprC> ExternC for R {
        type CType = Self;
    }
    impl<R: ReprFamily<Kind = Opaque>> ExternC for R {
        type CType = *mut Self;
    }

    #[cfg(feature = "cloned_refs")]
    impl<'a, R: ExternC, S: Cloned> ExternC for &'a R
    where
        Self: ReprFamily<Kind = &'a S>,
    {
        type CType = *const R::CType;
    }

    #[cfg(feature = "cloned_refs")]
    impl<'a, R: ExternC, S: Cloned> ExternC for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut S>,
    {
        type CType = *mut R::CType;
    }

    #[cfg(feature = "owned_as_ref")]
    impl<R: ExternC, S: Cloned> ExternC for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
    {
        type CType = *mut R::CType;
    }

    impl<'slice, R: CheckedTransmute> ExternC for &'slice [R]
    where
        Self: ReprFamily<Kind = &'slice [Transmuted]>,
        &'slice [<R as CheckedTransmute>::Target]: ExternC,
    {
        type CType = <&'slice [R::Target] as ExternC>::CType;
    }
    impl<'a, R: ReprC> ExternC for &'a [R]
    where
        Self: ReprFamily<Kind = &'a [Robust]>,
    {
        type CType = CSlice<R>;
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R> ExternC for &'a [R]
    where
        Self: ReprFamily<Kind = &'a [Opaque]>,
    {
        type CType = CSlice<*const R>;
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R: ExternC, S: Cloned> ExternC for &'a [R]
    where
        Self: ReprFamily<Kind = &'a [S]>,
    {
        type CType = CSlice<R::CType>;
    }

    impl<'slice, R: FlatTransmute> ExternC for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [Transmuted]>,
    {
        type CType = CSliceMut<R::CType>;
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R> ExternC for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [Opaque]>,
    {
        type CType = CSliceMut<*mut R>;
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R: ExternC, S: Cloned> ExternC for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [S]>,
    {
        type CType = CSliceMut<R::CType>;
    }

    impl<R: CheckedTransmute> ExternC for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Transmuted]>>,
        Box<[<R as CheckedTransmute>::Target]>: ExternC,
    {
        type CType = <Box<[R::Target]> as ExternC>::CType;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprC> ExternC for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Robust]>>,
    {
        type CType = BoxedSliceCType<R>;
    }
    #[cfg(feature = "owned_types")]
    impl<R> ExternC for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Opaque]>>,
    {
        type CType = BoxedSliceCType<*mut R>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ExternC, S: Cloned> ExternC for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[S]>>,
    {
        type CType = CSliceMut<R::CType>;
    }

    #[cfg(feature = "owned_types")]
    impl<R: CheckedTransmute> ExternC for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Transmuted>>,
        Vec<<R as CheckedTransmute>::Target>: ExternC,
    {
        type CType = <Vec<R::Target> as ExternC>::CType;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprC> ExternC for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Robust>>,
    {
        type CType = VecCType<R>;
    }
    #[cfg(feature = "owned_types")]
    impl<R> ExternC for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Opaque>>,
    {
        type CType = VecCType<*mut R>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ExternC, S: Cloned> ExternC for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>>,
    {
        type CType = CSliceMut<R::CType>;
    }

    impl<R, const N: usize> ExternC for [R; N]
    where
        Self: ReprFamily<Kind = [Opaque; N]>,
    {
        type CType = [*mut R; N];
    }
    impl<R: ExternC, S: Cloned, const N: usize> ExternC for [R; N]
    where
        Self: ReprFamily<Kind = [S; N]>,
    {
        type CType = [R::CType; N];
    }

    impl<R: ExternC> ExternC for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithoutNiche>>,
    {
        type CType = COption<R::CType>;
    }
    impl<R: Niche> ExternC for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithCustomNiche>>,
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
        /// - Storing mutable references that need to be updated in [`Store::sync`]
        ///
        /// Conceptually, serves a role similar to the "context" captured by a closure.
        type Store: Store + Default;

        /// Convert from [`Self`] into [`Self::CType`].
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
    impl<
        #[cfg(feature = "non_robust_ref_mut")] R,
        #[cfg(not(feature = "non_robust_ref_mut"))] R: MutSafe,
    > Encode for R
    where
        R: ReprFamily<Kind = Transmuted> + CheckedTransmute<Target: Encode>,
    {
        type Store = <R::Target as Encode>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            transmute_into_target(self).encode(store)
        }
    }
    impl<R: ReprFamily<Kind = Robust> + ReprC> Encode for R {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            self
        }
    }
    impl<R: ReprFamily<Kind = Opaque>> Encode for R {
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
        Self: ReprFamily<Kind = &'a S>,
    {
        type Store = RefStore<R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = self.clone().encode(&mut store.encode_store);
            store.encoded.insert(encoded)
        }
    }

    #[cfg(feature = "cloned_refs")]
    impl<'a, 'b, R: Encode + Decode<'b> + NonLocal + Clone + 'b, S: Cloned> Encode for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut S>,
    {
        type Store = RefMutStore<'a, R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let original = store.original.insert(self);
            let encoded = original.clone().encode(&mut store.encode_store);
            store.encoded.insert(encoded)
        }
    }

    #[cfg(feature = "owned_as_ref")]
    impl<R: Encode, S: Cloned> Encode for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
    {
        type Store = RefStore<R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = (*self).encode(&mut store.encode_store);
            store.encoded.insert(encoded)
        }
    }

    impl<'slice, R: CheckedTransmute> Encode for &'slice [R]
    where
        Self: ReprFamily<Kind = &'slice [Transmuted]>,
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
        Self: ReprFamily<Kind = &'slice [Robust]>,
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
        Self: ReprFamily<Kind = &'slice [Opaque]>,
    {
        type Store = OwningStore<*const R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let ctypes = self.iter().map(core::ptr::from_ref).collect();
            CSlice::from_slice(Some(store.0.insert(ctypes)))
        }
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R: Encode + Clone, S: Cloned> Encode for &'slice [R]
    where
        Self: ReprFamily<Kind = &'slice [S]>,
    {
        type Store = SliceStore<R::CType, R::Store>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let slice = self.to_vec();

            let stores = store.stores.insert(
                core::iter::repeat_with(Default::default)
                    .take(slice.len())
                    .collect(),
            );

            let ctypes = store.ctypes.insert(
                slice
                    .into_iter()
                    .zip(&mut *stores)
                    .map(|(item, substore)| item.encode(substore))
                    .collect(),
            );

            CSlice::from_slice(Some(ctypes))
        }
    }

    impl<
        'slice,
        #[cfg(not(feature = "non_robust_ref_mut"))] R: ReprC + FlatTransmute,
        #[cfg(feature = "non_robust_ref_mut")] R: FlatTransmute,
    > Encode for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [Transmuted]>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            CSliceMut::from_slice(Some(transmute_into_target_slice_mut(self)))
        }
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R> Encode for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [Opaque]>,
    {
        type Store = OpaqueMutSliceEncodeStore<'slice, R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let original = store.original.insert(self);
            let ctypes = original.iter_mut().map(core::ptr::from_mut).collect();
            CSliceMut::from_slice(Some(store.encoded.insert(ctypes)))
        }
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, 'b, R: Encode + Decode<'b> + NonLocal + Clone + 'b, S: Cloned> Encode for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [S]>,
    {
        type Store = MutSliceStore<'slice, R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let original = store.original.insert(self);
            let cloned_items: Vec<R> = original.iter().cloned().collect();

            let stores = store.stores.insert(
                core::iter::repeat_with(Default::default)
                    .take(original.len())
                    .collect(),
            );

            let ctypes = store.ctypes.insert(
                cloned_items
                    .into_iter()
                    .zip(&mut *stores)
                    .map(|(item, substore)| item.encode(substore))
                    .collect(),
            );

            CSliceMut::from_slice(Some(ctypes))
        }
    }

    #[cfg(feature = "owned_types")]
    impl<R: CheckedTransmute> Encode for Box<[R]>
    where
        Box<[<R as CheckedTransmute>::Target]>: Encode,
        Self: ReprFamily<Kind = Box<[Transmuted]>>,
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
    impl<R: ReprC> Encode for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Robust]>>,
    {
        #[cfg(feature = "owned_as_ref")]
        type Store = OwningStore<R>;
        #[cfg(not(feature = "owned_as_ref"))]
        type Store = ();

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            #[cfg(feature = "owned_as_ref")]
            let encoded = {
                let store = store.0.insert(self);
                CSliceMut::from_slice(Some(store))
            };
            #[cfg(not(feature = "owned_as_ref"))]
            let encoded = CBoxedSlice::from_boxed_slice(Some(self));

            encoded
        }
    }
    #[cfg(feature = "owned_types")]
    impl<R> Encode for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Opaque]>>,
    {
        #[cfg(feature = "owned_as_ref")]
        type Store = OwningStore<*mut R>;
        #[cfg(not(feature = "owned_as_ref"))]
        type Store = ();

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let boxed_ptrs = self.into_iter().map(Box::new).map(Box::into_raw).collect();

            #[cfg(feature = "owned_as_ref")]
            let encoded = {
                let store = store.0.insert(boxed_ptrs);
                CSliceMut::from_slice(Some(store))
            };
            #[cfg(not(feature = "owned_as_ref"))]
            let encoded = CBoxedSlice::from_boxed_slice(Some(boxed_ptrs));

            encoded
        }
    }
    #[cfg(feature = "owned_types")]
    impl<R: Encode, S: Cloned> Encode for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[S]>>,
    {
        type Store = SliceStore<R::CType, R::Store>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let stores = store.stores.insert(
                core::iter::repeat_with(Default::default)
                    .take(self.len())
                    .collect(),
            );

            let ctypes = store.ctypes.insert(
                self.into_iter()
                    .zip(&mut *stores)
                    .map(|(item, substore)| item.encode(substore))
                    .collect(),
            );

            CSliceMut::from_slice(Some(ctypes))
        }
    }

    #[cfg(feature = "owned_types")]
    impl<R: CheckedTransmute> Encode for Vec<R>
    where
        Vec<<R as CheckedTransmute>::Target>: Encode,
        Self: ReprFamily<Kind = Vec<Transmuted>>,
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
    impl<R: ReprC> Encode for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Robust>>,
    {
        #[cfg(feature = "owned_as_ref")]
        type Store = OwningStore<R>;
        #[cfg(not(feature = "owned_as_ref"))]
        type Store = ();

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            #[cfg(feature = "owned_as_ref")]
            let encoded = {
                let store = store.0.insert(self.into_boxed_slice());
                CSliceMut::from_slice(Some(store))
            };
            #[cfg(not(feature = "owned_as_ref"))]
            let encoded = CVec::from_vec(Some(self));

            encoded
        }
    }
    #[cfg(feature = "owned_types")]
    impl<R> Encode for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Opaque>>,
    {
        #[cfg(feature = "owned_as_ref")]
        type Store = OwningStore<*mut R>;
        #[cfg(not(feature = "owned_as_ref"))]
        type Store = ();

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let boxed_ptrs = self.into_iter().map(Box::new).map(Box::into_raw).collect();

            #[cfg(feature = "owned_as_ref")]
            let encoded = {
                let store = store.0.insert(boxed_ptrs);
                CSliceMut::from_slice(Some(store))
            };
            #[cfg(not(feature = "owned_as_ref"))]
            let encoded = CVec::from_vec(Some(boxed_ptrs.into_vec()));

            encoded
        }
    }
    #[cfg(feature = "owned_types")]
    impl<R: Encode, S: Cloned> Encode for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>>,
    {
        type Store = SliceStore<R::CType, R::Store>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let stores = store.stores.insert(
                core::iter::repeat_with(Default::default)
                    .take(self.len())
                    .collect(),
            );

            let ctypes = store.ctypes.insert(
                self.into_iter()
                    .zip(&mut *stores)
                    .map(|(item, substore)| item.encode(substore))
                    .collect(),
            );

            CSliceMut::from_slice(Some(ctypes))
        }
    }

    impl<R, const N: usize> Encode for [R; N]
    where
        Self: ReprFamily<Kind = [Opaque; N]>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            assert_arr_has_non_zero_len::<N>();

            let ctypes = self
                .into_iter()
                .map(Box::new)
                .map(Box::into_raw)
                .collect::<Vec<_>>()
                .try_into();

            // SAFETY: Vec<T> length is N
            unsafe { ctypes.unwrap_unchecked() }
        }
    }
    impl<R: Encode, S: Cloned, const N: usize> Encode for [R; N]
    where
        // FIXME: https://github.com/rust-lang/rust/issues/61415
        [<R as Encode>::Store; N]: Default,
        Self: ReprFamily<Kind = [S; N]>,
    {
        type Store = ArraySyncStore<R::Store, N>;

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
            store.0 = default_init_arr();

            let ctypes: Vec<_> = self
                .into_iter()
                .zip(store.0.iter_mut())
                .map(|(item, substore)| item.encode(substore))
                .collect();

            // SAFETY: Vec<T> length is N
            unsafe { ctypes.try_into().unwrap_unchecked() }
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
        Self: ReprFamily<Kind = Option<WithoutNiche>>,
    {
        type Store = <R as Encode>::Store;

        fn encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.map(|v| v.encode(store)).into()
        }
    }
    impl<R: Niche + Encode> Encode for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithCustomNiche>>,
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
        /// - Storing mutable references that need to be updated in [`Store::sync`]
        ///
        /// Conceptually, serves a role similar to the "context" captured by a closure.
        type Store: Store + Default;

        /// Perform the conversion from [`Self::CType`] into [`Self`]
        ///
        /// # Safety
        ///
        /// - All conversions from a pointer must ensure pointer validity beforehand
        /// - If `type Store = ()`, then the store **must never be dereferenced**
        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self>;
    }

    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: CheckedTransmute<Target: ReprC + 'd> + Clone> Decode<'d> for R
    where
        Self: ReprFamily<Kind = Box<Robust>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            transmute_from_target::<&R>(&source).cloned()
        }
    }
    impl<'d, R: CheckedTransmute> Decode<'d> for R
    where
        <Self as CheckedTransmute>::Target: Decode<'d>,
        Self: ReprFamily<Kind = Transmuted>,
    {
        type Store = <R::Target as Decode<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { Decode::decode(source, store).and_then(|inner| transmute_from_target(inner)) }
        }
    }
    impl<'d, R: ReprC + 'd> Decode<'d> for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            Some(source)
        }
    }
    impl<'d, R: 'd> Decode<'d> for R
    where
        Self: ReprFamily<Kind = Opaque>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            Some(*unsafe { Box::from_raw(source) })
        }
    }

    #[cfg(feature = "cloned_refs")]
    impl<'d, R: Decode<'d> + Clone, S: Cloned> Decode<'d> for &'d R
    where
        Self: ReprFamily<Kind = &'d S>,
    {
        type Store = RefDecodeStore<R, R::Store>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            Some(
                store.value.insert(
                    unsafe { R::decode(source.read(), &mut store.store) }
                        .map(ManuallyDrop::new)
                        .map(|item| (*item).clone())?,
                ),
            )
        }
    }

    #[cfg(feature = "cloned_refs")]
    impl<'d, R: Encode + Decode<'d> + NonLocal + Clone, S: Cloned> Decode<'d> for &'d mut R
    where
        Self: ReprFamily<Kind = &'d mut S>,
    {
        type Store = RefMutDecodeStore<R, <R as Decode<'d>>::Store>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let source = store.source.insert(source);
            let source = unsafe { source.read() };

            Some(
                store.decoded.insert(
                    unsafe { R::decode(source, &mut store.decode_store) }
                        .map(ManuallyDrop::new)
                        .map(|item| (*item).clone())?,
                ),
            )
        }
    }

    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: Decode<'d> + Clone, S: Cloned> Decode<'d> for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
    {
        type Store = R::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source.is_null() {
                return None;
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
        Self: ReprFamily<Kind = &'slice [Transmuted]>,
    {
        type Store = <&'slice [R::Target] as Decode<'slice>>::Store;

        unsafe fn decode<'itm: 'slice>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe {
                let slice = <&[R::Target]>::decode(source, store)?;
                transmute_from_target_ref_slice(slice)
            }
        }
    }
    impl<'slice, R: ReprC> Decode<'slice> for &'slice [R]
    where
        Self: ReprFamily<Kind = &'slice [Robust]>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'slice>(source: Self::CType, (): &mut ()) -> Option<Self> {
            unsafe { source.into_rust() }
        }
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R: Clone> Decode<'slice> for &'slice [R]
    where
        Self: ReprFamily<Kind = &'slice [Opaque]>,
    {
        type Store = OwningStore<R>;

        unsafe fn decode<'itm: 'slice>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.into_rust() }?;

            let store = store.0.insert(
                source
                    .iter()
                    .map(|item| unsafe { item.as_ref() }.cloned())
                    .collect::<Option<Box<_>>>()?,
            );

            Some(store)
        }
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R: Decode<'slice> + Clone, S: Cloned> Decode<'slice> for &'slice [R]
    where
        Self: ReprFamily<Kind = &'slice [S]>,
    {
        type Store = DecodeStoreSlicePair<R, R::Store>;

        unsafe fn decode<'itm: 'slice>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.into_rust() }?;

            let stores = store.stores.insert(
                core::iter::repeat_with(Default::default)
                    .take(source.len())
                    .collect(),
            );

            let slice = source
                .iter()
                .zip(&mut *stores)
                .map(|(&item, substore)| unsafe { R::decode(item, substore) }.map(ManuallyDrop::new))
                .collect::<Option<Vec<_>>>()?;

            let values = store.values.insert(
                slice
                    .iter()
                    .cloned()
                    .map(ManuallyDrop::into_inner)
                    .collect(),
            );

            Some(values)
        }
    }

    impl<'slice, R: FlatTransmute> Decode<'slice> for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [Transmuted]>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'slice>(source: Self::CType, (): &mut ()) -> Option<Self> {
            transmute_from_target_slice_mut(unsafe { source.into_rust()? })
        }
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R: Clone> Decode<'slice> for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [Opaque]>,
    {
        type Store = OpaqueMutSliceDecodeStore<R>;

        unsafe fn decode<'itm: 'slice>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source: &mut CSliceMut<_> = store.source.insert(source);
            let source: &mut [*mut R] = unsafe { source.into_rust()? };

            let values = store.values.insert(
                source
                    .iter()
                    .map(|item| unsafe { item.as_mut() }.cloned())
                    .collect::<Option<_>>()?,
            );

            Some(values)
        }
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R: Encode + Decode<'slice> + NonLocal + Clone, S: Cloned> Decode<'slice> for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [S]>,
    {
        type Store = MutSliceDecodeStore<R, <R as Decode<'slice>>::Store>;

        unsafe fn decode<'itm: 'slice>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            let source: &mut CSliceMut<_> = store.source.insert(source);
            let source: &mut [_] = unsafe { source.into_rust() }?;

            let stores = store.stores.insert(
                core::iter::repeat_with(Default::default)
                    .take(source.len())
                    .collect(),
            );

            let decoded = source
                .iter()
                .zip(&mut *stores)
                .map(|(&item, substore)| unsafe { R::decode(item, substore) }.map(ManuallyDrop::new))
                .collect::<Option<Vec<_>>>()?;

            let values = store
                .values
                .insert(decoded.into_iter().map(ManuallyDrop::into_inner).collect());

            Some(values)
        }
    }

    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: CheckedTransmute> Decode<'d> for Box<[R]>
    where
        Box<[<R as CheckedTransmute>::Target]>: Decode<'d>,
        Self: ReprFamily<Kind = Box<[Transmuted]>>,
    {
        type Store = <Box<[R::Target]> as Decode<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe {
                Decode::decode(source, store)
                    .and_then(|output| transmute_from_target_boxed_slice(output))
            }
        }
    }
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: ReprC + 'd> Decode<'d> for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Robust]>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            unsafe { source.into_rust() }.map(|slice| slice.into())
        }
    }
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: 'd> Decode<'d> for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Opaque]>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let slice = unsafe { source.into_rust() }?;

            slice
                .iter()
                .map(|&item| unsafe { item.as_mut().map(|item| *Box::from_raw(item)) })
                .collect::<Option<_>>()
        }
    }
    #[cfg(feature = "owned_types")]
    impl<'d, R: Decode<'d> + Clone, S: Cloned> Decode<'d> for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[S]>>,
    {
        type Store = DecodeStoreSlice<R::Store>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            let slice = unsafe { source.into_rust() }?;

            let store = store.0.insert(
                core::iter::repeat_with(Default::default)
                    .take(slice.len())
                    .collect(),
            );

            let vec = slice
                .iter()
                .copied()
                .zip(&mut *store)
                .map(|(item, substore)| unsafe { R::decode(item, substore) }.map(ManuallyDrop::new))
                .collect::<Option<Vec<_>>>()?;

            Some(vec.iter().cloned().map(ManuallyDrop::into_inner).collect())
        }
    }

    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: CheckedTransmute> Decode<'d> for Vec<R>
    where
        Vec<<R as CheckedTransmute>::Target>: Decode<'d>,
        Self: ReprFamily<Kind = Vec<Transmuted>>,
    {
        type Store = <Vec<R::Target> as Decode<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe {
                <Vec<R::Target>>::decode(source, store)
                    .and_then(|output| transmute_from_target_vec(output))
            }
        }
    }
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: ReprC + 'd> Decode<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Robust>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            unsafe { source.into_rust() }.map(|slice| slice.to_vec())
        }
    }
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: 'd> Decode<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Opaque>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let slice = unsafe { source.into_rust()? };

            slice
                .iter()
                .map(|&item| unsafe { item.as_mut().map(|item| *Box::from_raw(item)) })
                .collect::<Option<_>>()
        }
    }
    #[cfg(feature = "owned_types")]
    impl<'d, R: Decode<'d> + Clone, S: Cloned> Decode<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>>,
    {
        type Store = DecodeStoreSlice<R::Store>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            let slice = unsafe { source.into_rust() }?;

            let store = store.0.insert(
                core::iter::repeat_with(Default::default)
                    .take(slice.len())
                    .collect(),
            );

            let vec = slice
                .iter()
                .copied()
                .zip(&mut *store)
                .map(|(item, substore)| unsafe { R::decode(item, substore).map(ManuallyDrop::new) })
                .collect::<Option<Vec<_>>>()?;

            Some(vec.iter().cloned().map(ManuallyDrop::into_inner).collect())
        }
    }

    impl<'d, R: 'd, const N: usize> Decode<'d> for [R; N]
    where
        Self: ReprFamily<Kind = [Opaque; N]>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            assert_arr_has_non_zero_len::<N>();

            let array: [R; N] = source
                .into_iter()
                .map(|item| unsafe { item.as_mut().map(|item| *Box::from_raw(item)) })
                .collect::<Option<Vec<_>>>()?
                .try_into()
                .ok()?;

            Some(array)
        }
    }
    impl<'d, R: Decode<'d> + Clone, S: Cloned, const N: usize> Decode<'d> for [R; N]
    where
        // FIXME: https://github.com/rust-lang/rust/issues/61415
        [<R as Decode<'d>>::Store; N]: Default,
        Self: ReprFamily<Kind = [S; N]>,
    {
        type Store = DecodeArrayStore<<R as Decode<'d>>::Store, N>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            assert_arr_has_non_zero_len::<N>();

            let vec: [_; N] = source
                .into_iter()
                .zip(store.0.iter_mut())
                .map(|(item, substore)| unsafe { R::decode(item, substore).map(ManuallyDrop::new) })
                .collect::<Option<Vec<_>>>()?
                .try_into()
                .ok()?;

            let array: [R; N] = vec
                .iter()
                .cloned()
                .map(ManuallyDrop::into_inner)
                .collect::<Vec<_>>()
                .try_into()
                .ok()?;

            Some(array)
        }
    }

    impl<'d, R: Decode<'d>> Decode<'d> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithoutNiche>>,
    {
        type Store = <R as Decode<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            let option = TryInto::<Option<_>>::try_into(source).ok()?;
            match option {
                Some(payload) => unsafe { R::decode(payload, store) }.map(Some),
                None => Some(None),
            }
        }
    }
    impl<'d, R: Niche<CType: PartialEq> + Decode<'d>> Decode<'d> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithCustomNiche>>,
    {
        type Store = <R as Decode<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source == R::NICHE_VALUE {
                return Some(None);
            }

            unsafe { R::decode(source, store) }.map(Some)
        }
    }
}

// TODO: Could the store just be synced on drop?
pub trait Store {
    fn sync(self);
}

impl Store for () {
    fn sync(self) {}
}

pub struct DecodeStoreSlice<D>(Option<Box<[D]>>);

impl<D> Default for DecodeStoreSlice<D> {
    fn default() -> Self {
        Self(None)
    }
}

impl<D: Store> Store for DecodeStoreSlice<D> {
    fn sync(self) {
        for store in self.0.unwrap() {
            store.sync();
        }
    }
}

pub struct RefDecodeStore<R, D> {
    #[allow(dead_code)]
    value: Option<R>,
    store: D,
}

impl<R, D: Default> Default for RefDecodeStore<R, D> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
        }
    }
}

impl<R, D: Store> Store for RefDecodeStore<R, D> {
    fn sync(self) {
        self.store.sync();
    }
}

pub struct OwningStore<T>(Option<Box<[T]>>);

impl<T> Default for OwningStore<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T> Store for OwningStore<T> {
    fn sync(self) {}
}

pub struct DecodeStoreSlicePair<R, D> {
    #[allow(dead_code)]
    values: Option<Box<[R]>>,
    stores: Option<Box<[D]>>,
}

impl<R, D> Default for DecodeStoreSlicePair<R, D> {
    fn default() -> Self {
        Self {
            stores: None,
            values: None,
        }
    }
}

impl<R, D: Store> Store for DecodeStoreSlicePair<R, D> {
    fn sync(self) {
        for store in self.stores.unwrap() {
            store.sync();
        }
    }
}

pub struct DecodeArrayStore<D, const N: usize>(pub [D; N]);

impl<D, const N: usize> Default for DecodeArrayStore<D, N>
where
    [D; N]: Default,
{
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<D: Store, const N: usize> Store for DecodeArrayStore<D, N> {
    fn sync(self) {
        for store in self.0 {
            store.sync();
        }
    }
}

#[cfg(feature = "cloned_refs")]
pub struct OpaqueMutSliceEncodeStore<'a, R> {
    encoded: Option<Box<[*mut R]>>,
    original: Option<&'a mut [R]>,
}

#[cfg(feature = "cloned_refs")]
impl<'a, R> Default for OpaqueMutSliceEncodeStore<'a, R> {
    fn default() -> Self {
        Self {
            encoded: None,
            original: None,
        }
    }
}

#[cfg(feature = "cloned_refs")]
impl<'a, R> Store for OpaqueMutSliceEncodeStore<'a, R> {
    fn sync(self) {
        for (orig, ptr) in self.original.unwrap().iter_mut().zip(self.encoded.unwrap()) {
            if !core::ptr::eq(ptr, orig) {
                *orig = unsafe { ptr.read() };
            }
        }
    }
}

#[cfg(any(feature = "cloned_refs", feature = "owned_as_ref"))]
pub struct RefStore<R: Encode> {
    encoded: Option<R::CType>,
    encode_store: R::Store,
}

#[cfg(any(feature = "cloned_refs", feature = "owned_as_ref"))]
impl<R: Encode> Default for RefStore<R> {
    fn default() -> Self {
        Self {
            encoded: None,
            encode_store: Default::default(),
        }
    }
}

#[cfg(any(feature = "cloned_refs", feature = "owned_as_ref"))]
impl<R: Encode> Store for RefStore<R> {
    fn sync(self) {
        self.encode_store.sync();
    }
}

#[cfg(feature = "cloned_refs")]
pub struct RefMutStore<'a, R: Encode> {
    encoded: Option<R::CType>,
    encode_store: R::Store,
    original: Option<&'a mut R>,
}

#[cfg(feature = "cloned_refs")]
impl<'a, R: Encode> Default for RefMutStore<'a, R> {
    fn default() -> Self {
        Self {
            encoded: None,
            encode_store: Default::default(),
            original: None,
        }
    }
}

#[cfg(feature = "cloned_refs")]
impl<'a, 'b, R: Encode + Decode<'b> + NonLocal + 'b> Store for RefMutStore<'a, R> {
    fn sync(self) {
        if let (Some(encoded), Some(original)) = (self.encoded, self.original) {
            let mut decode_store = Default::default();

            let store_ref = unsafe {
                core::mem::transmute::<&mut <R as Decode>::Store, &'b mut <R as Decode>::Store>(
                    &mut decode_store,
                )
            };

            if let Some(decoded) = unsafe { R::decode(encoded, store_ref) } {
                *original = decoded;
            }
        }
    }
}

pub struct SliceStore<C, D> {
    ctypes: Option<Box<[C]>>,
    stores: Option<Box<[D]>>,
}

impl<C, D> Default for SliceStore<C, D> {
    fn default() -> Self {
        Self {
            ctypes: None,
            stores: None,
        }
    }
}

impl<C, D: Store> Store for SliceStore<C, D> {
    fn sync(self) {
        for store in self.stores.unwrap() {
            store.sync();
        }
    }
}

#[cfg(feature = "cloned_refs")]
pub struct MutSliceStore<'slice, R: Encode> {
    ctypes: Option<Box<[R::CType]>>,
    stores: Option<Box<[R::Store]>>,
    original: Option<&'slice mut [R]>,
}

#[cfg(feature = "cloned_refs")]
impl<'slice, R: Encode> Default for MutSliceStore<'slice, R> {
    fn default() -> Self {
        Self {
            ctypes: None,
            stores: None,
            original: None,
        }
    }
}

#[cfg(feature = "cloned_refs")]
impl<'slice, 'b, R: Encode + Decode<'b> + NonLocal + 'b> Store for MutSliceStore<'slice, R> {
    fn sync(self) {
        if let (Some(borrows), Some(ctypes)) = (self.original, self.ctypes) {
            let mut decode_store = Default::default();

            for (original, encoded) in borrows.iter_mut().zip(ctypes) {
                let store_ref = unsafe {
                    core::mem::transmute::<&mut <R as Decode>::Store, &'b mut <R as Decode>::Store>(
                        &mut decode_store,
                    )
                };

                if let Some(decoded) = unsafe { R::decode(encoded, store_ref) } {
                    *original = decoded;
                }
            }
        }
    }
}

pub struct ArraySyncStore<D, const N: usize>(pub [D; N]);

impl<D: Default, const N: usize> Default for ArraySyncStore<D, N>
where
    [D; N]: Default,
{
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<D: Store, const N: usize> Store for ArraySyncStore<D, N>
where
    [D; N]: Default,
{
    fn sync(self) {
        for store in self.0 {
            store.sync();
        }
    }
}

#[cfg(feature = "cloned_refs")]
pub struct RefMutDecodeStore<R: ExternC, DS> {
    decoded: Option<R>,
    decode_store: DS,
    source: Option<*mut R::CType>,
}

#[cfg(feature = "cloned_refs")]
impl<R: ExternC, DS: Default> Default for RefMutDecodeStore<R, DS> {
    fn default() -> Self {
        Self {
            decoded: None,
            decode_store: Default::default(),
            source: None,
        }
    }
}

#[cfg(feature = "cloned_refs")]
impl<R: Encode + NonLocal + Clone, DS: Store> Store for RefMutDecodeStore<R, DS> {
    fn sync(self) {
        let mut encode_store = Default::default();
        let encoded = self.decoded.unwrap().encode(&mut encode_store);
        unsafe { *self.source.unwrap() = encoded };
    }
}

#[cfg(feature = "cloned_refs")]
pub struct MutSliceDecodeStore<R: ExternC, DS> {
    values: Option<Box<[R]>>,
    stores: Option<Box<[DS]>>,
    source: Option<CSliceMut<R::CType>>,
}

#[cfg(feature = "cloned_refs")]
impl<R: ExternC, DS: Default> Default for MutSliceDecodeStore<R, DS> {
    fn default() -> Self {
        Self {
            values: None,
            stores: None,
            source: None,
        }
    }
}

#[cfg(feature = "cloned_refs")]
impl<R: Encode + NonLocal, DS: Store> Store for MutSliceDecodeStore<R, DS> {
    fn sync(self) {
        let mut encode_store = Default::default();

        let source = unsafe { self.source.unwrap().into_rust().unwrap() };
        for (src, decoded) in source.iter_mut().zip(self.values.unwrap()) {
            let encoded = decoded.encode(&mut encode_store);
            *src = encoded;
        }
    }
}

#[cfg(feature = "cloned_refs")]
pub struct OpaqueMutSliceDecodeStore<R> {
    values: Option<Box<[R]>>,
    source: Option<CSliceMut<*mut R>>,
}

#[cfg(feature = "cloned_refs")]
impl<R> Default for OpaqueMutSliceDecodeStore<R> {
    fn default() -> Self {
        Self {
            values: None,
            source: None,
        }
    }
}

#[cfg(feature = "cloned_refs")]
impl<R> Store for OpaqueMutSliceDecodeStore<R> {
    fn sync(self) {
        let slice = unsafe { self.source.unwrap().into_rust().unwrap() };

        for (src, decoded) in slice.iter_mut().zip(self.values.unwrap()) {
            unsafe { **src = decoded };
        }
    }
}

/// Macro for defining FFI types of a known category ([`Robust`] or [`CheckedTransmute`]).
///
/// The implementation for an FFI type of one of the categories incurs a lot of bloat that
/// is reduced by the use of this macro
///
/// # Safety
///
/// * If the type is [`Robust`], it derives [`ReprC`]. Check safety invariants for [`ReprC`]
/// * If the type is [`Transmuted`], it derives [`CheckedTransmute`]. Check safety invariants for [`CheckedTransmute`]
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

        impl$(<$($params)*>)? $crate::ir::ReprFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::ir::Robust;
        }
        impl $(<$($params)*>)? $crate::niche::NicheFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::niche::WithoutNiche;
        }
    };
    (unsafe impl $(( $($params:tt)* ))? Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;
        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        impl $(<$($params)*>)? $crate::ir::ReprFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::ir::Transmuted;
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
                    ($target: $crate::niche::NicheFamily<Kind = $crate::niche::WithoutNiche>),
                    "Transparent CAN'T define a custom niche if target has a niche"
                );

                $niche_value
            };
        }

        impl $(<$($params)*>)? $crate::niche::NicheFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::niche::WithCustomNiche;
        }

        unsafe impl $(<$($params)*>)? $crate::transmute::MutSafe for $self_ty where $target: $crate::Encode, $($($preds)*)? {}
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
    (@transparent [$($for_dummy:tt)*] [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])? {
        type Target = $target:ty;
        fn is_valid($target_var:ident: $target_ty:ty) -> bool $block:block
    }) => {
        impl $($impl_generics)* $crate::ir::ReprFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::ir::Transmuted;
        }

        unsafe impl $($impl_generics)* $crate::transmute::CheckedTransmute for $self_ty $(where $($preds)*)? {
            type Target = $target;

            #[inline(always)]
            fn is_valid($target_var: $target_ty) -> bool $block
        }

        impl $($impl_generics)* $crate::niche::NicheFamily for $self_ty where $($for_dummy)* $target: $crate::niche::NicheFamily, $($($preds)*)? {
            type Kind = <$target as $crate::niche::NicheFamily>::Kind;
        }

        impl $($impl_generics)* $crate::niche::Niche for $self_ty where $($for_dummy)* $target: $crate::niche::Niche, $($($preds)*)? {
            const NICHE_VALUE: <Self as $crate::ExternC>::CType = <$target as $crate::niche::Niche>::NICHE_VALUE;
        }

        unsafe impl $($impl_generics)* $crate::niche::StableNiche for $self_ty where $($for_dummy)* $target: $crate::niche::StableNiche, $($($preds)*)? {}
        unsafe impl $($impl_generics)* $crate::transmute::MutSafe for $self_ty where $($for_dummy)* $target: $crate::Encode, $($($preds)*)? {}
    };
}

mineral! {
    unsafe impl(R) Robust for *const R {}
}
mineral! {
    unsafe impl(R) Robust for *mut R {}
}

// SAFETY: Array is just a contiguous block of memory
unsafe impl<R: ReprC, const N: usize> ReprC for [R; N] {}

unsafe impl<R: StableNiche + Copy> ReprC for Option<R> where Self: ReprFamily<Kind = Transmuted> {}

// TODO: Check https://github.com/mversic/co3/issues/13
const fn assert_arr_has_non_zero_len<const N: usize>() {
    assert!(N != 0, "empty array is a ZST");
}

#[cfg(test)]
mod tests {
    use super::*;

    use alloc::string::String;
    use static_assertions::{assert_impl_all, assert_not_impl_any};

    #[test]
    fn robust_u8() {
        assert_impl_all!(u8: ReprC, FlatTransmute<CType = u8>);
        assert_impl_all!(&u8: CheckedTransmute<Target = *const u8>, FlatTransmute<CType = *const u8>, StableNiche);
        assert_impl_all!(&mut u8: CheckedTransmute<Target = *mut u8>, FlatTransmute<CType = *mut u8>, StableNiche);
        // FIXME:
        //assert_impl_all!(Box<u8>: CheckedTransmute<Target = *mut u8>, FlatTransmute<CType = *mut u8>, StableNiche);
        assert_impl_all!(&[u8]: Niche<CType = CSlice<u8>>);
        assert_impl_all!(&mut [u8]: Niche<CType = CSliceMut<u8>>);
        assert_impl_all!([u8; 2]: ReprC, FlatTransmute<CType = [u8; 2]>);
        assert_impl_all!(Option<u8>: Niche<CType = COption<u8>>);

        assert_not_impl_any!(u8: CheckedTransmute);
        assert_not_impl_any!(Box<u8>: ReprC);
        assert_not_impl_any!(&mut u8: ReprC);
        assert_not_impl_any!(&u8: ReprC);
        assert_not_impl_any!(&[u8]: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(&mut [u8]: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!([u8; 2]: CheckedTransmute, Niche);
        assert_not_impl_any!(Option<u8>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
    }

    #[test]
    fn robust_ptr() {
        assert_impl_all!(*const String: ReprC, FlatTransmute<CType = *const String>);
        assert_impl_all!(&*const String: CheckedTransmute<Target = *const *const String>, FlatTransmute<CType = *const *const String>, StableNiche);
        assert_impl_all!(&mut *const String: CheckedTransmute<Target = *mut *const String>, FlatTransmute<CType = *mut *const String>, StableNiche);
        // FIXME:
        //assert_impl_all!(Box<*const String>: CheckedTransmute<Target = *mut *const String>, FlatTransmute<CType = *mut *const String>, StableNiche);
        assert_impl_all!(&[*const String]: Niche<CType = CSlice<*const String>>);
        assert_impl_all!(&mut [*const String]: Niche<CType = CSliceMut<*const String>>);
        assert_impl_all!([*const String; 2]: ReprC, FlatTransmute<CType = [*const String; 2]>);
        assert_impl_all!(Option<*const String>: Niche<CType = COption<*const String>>);

        assert_not_impl_any!(*const String: CheckedTransmute);
        assert_not_impl_any!(&*const String: ReprC);
        assert_not_impl_any!(&mut *const String: ReprC);
        assert_not_impl_any!(Box<*const String>: ReprC);
        assert_not_impl_any!(&[*const String]: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(&mut [*const String]: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!([*const String; 2]: CheckedTransmute, Niche);
        assert_not_impl_any!(Option<*const String>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
    }

    #[test]
    #[cfg(feature = "owned_types")]
    fn owned_types() {
        use crate::tuple::CTuple3;

        assert_impl_all!(Box<[u8]>: Niche<CType = BoxedSliceCType<u8>>);
        assert_impl_all!(Vec<u8>: Niche<CType = VecCType<u8>>);
        assert_impl_all!(Box<[*const String]>: Niche<CType = BoxedSliceCType<*const String>>);
        assert_impl_all!(Vec<*const String>: Niche<CType = VecCType<*const String>>);
        assert_impl_all!(Box<[bool]>: Niche<CType = BoxedSliceCType<u8>>);
        assert_impl_all!(Vec<bool>: Niche<CType = VecCType<u8>>);
        assert_impl_all!(Box<[&u8]>: Niche<CType = BoxedSliceCType<*const u8>>);
        assert_impl_all!(Vec<&u8>: Niche<CType = VecCType<*const u8>>);
        assert_impl_all!(Box<[&bool]>: Niche<CType = BoxedSliceCType<*const u8>>);
        assert_impl_all!(Vec<&bool>: Niche<CType = VecCType<*const u8>>);
        assert_impl_all!(Box<[(u8, u8, u8)]>: Niche<CType = BoxedSliceCType<CTuple3<u8, u8, u8>>>);
        assert_impl_all!(Vec<(u8, u8, u8)>: Niche<CType = VecCType<CTuple3<u8, u8, u8>>>);
        assert_impl_all!(Box<[(u8, bool, u8)]>: Niche<CType = BoxedSliceCType<CTuple3<u8, u8, u8>>>);
        assert_impl_all!(Vec<(u8, bool, u8)>: Niche<CType = VecCType<CTuple3<u8, u8, u8>>>);

        assert_not_impl_any!(Box<[u8]>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(Vec<u8>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(Box<[*const String]>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(Vec<*const String>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(Box<[bool]>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(Vec<bool>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(Box<[&u8]>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(Vec<&u8>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(Box<[&bool]>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(Vec<&bool>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(Box<[(u8, u8, u8)]>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(Vec<(u8, u8, u8)>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(Box<[(u8, bool, u8)]>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(Vec<(u8, bool, u8)>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
    }

    #[test]
    #[cfg(feature = "cloned_refs")]
    fn encode_cloned_mut_ref() {
        let inner = 8u8;
        let other = 42u8;
        let mut value = (&inner,);
        let value_mut_ref = &mut value;
        {
            let mut store = Box::new(Default::default());
            let encoded = value_mut_ref.encode(&mut *store);
            unsafe {
                (*encoded).0 = &other;
            }
            store.sync();
        }
        assert_eq!(value, (&42u8,));

        let mut slice = [(&1u8,)];
        let ref_mut: &mut [(&u8,)] = &mut slice;
        {
            let mut store = Box::new(Default::default());
            let encoded = ref_mut.encode(&mut *store);
            let c_slice = unsafe { encoded.into_rust().unwrap() };
            c_slice[0].0 = &other;
            store.sync();
        }
        assert_eq!(slice, [(&42,)]);
    }

    #[test]
    #[cfg(feature = "cloned_refs")]
    fn encode_opaque_mut_ref() {
        #[derive(Clone, PartialEq, Eq)]
        struct OpaqueData {
            value: i32,
        }

        impl ReprFamily for OpaqueData {
            type Kind = crate::ir::Opaque;
        }

        let mut items = [OpaqueData { value: 1 }];
        let slice_ref: &mut [_] = &mut items;
        let mut other = Box::new(OpaqueData { value: 100 });
        {
            let mut store = Box::new(Default::default());
            let encoded = slice_ref.encode(&mut store);
            let c_slice = unsafe { encoded.into_rust().unwrap() };
            c_slice[0] = &mut *other;

            store.sync();
        }
        assert_eq!(items[0].value, 100);
    }

    #[test]
    #[cfg(feature = "cloned_refs")]
    fn decode_opaque_mut_ref() {
        use crate::slice::CSliceMut;

        #[derive(Clone, PartialEq, Eq)]
        struct OpaqueData {
            value: i32,
        }

        impl ReprFamily for OpaqueData {
            type Kind = crate::ir::Opaque;
        }

        let mut item1 = OpaqueData { value: 10 };
        let mut ptrs: [*mut OpaqueData; 1] = [&mut item1];
        let c_slice = CSliceMut::from_slice(Some(&mut ptrs));

        {
            let mut store = Box::new(Default::default());
            let decoded = unsafe { <&mut [OpaqueData]>::decode(c_slice, &mut store) }.unwrap();
            decoded[0].value = 100;
            store.sync();
        }

        assert_eq!(item1.value, 100);
    }

    #[test]
    #[cfg(feature = "cloned_refs")]
    fn decode_cloned_mut_ref() {
        use crate::{slice::CSliceMut, tuple::CTuple1};

        let inner: u8 = 1;
        let inner_ptr: *const u8 = &inner;
        let mut c_tuple = CTuple1(inner_ptr);
        let c_ptr: *mut CTuple1<*const u8> = &mut c_tuple;
        let new_val: u8 = 42;
        {
            let mut store = Box::new(Default::default());
            let decoded = unsafe { <&mut (&u8,)>::decode(c_ptr, &mut *store) }.unwrap();
            decoded.0 = &new_val;
            store.sync();
        }
        assert_eq!(unsafe { *c_tuple.0 }, 42);

        let a: u8 = 1;
        let mut c_tuples = [CTuple1(&a as *const u8)];
        let c_slice = CSliceMut::from_slice(Some(&mut c_tuples));
        let x: u8 = 10;
        {
            let mut store = Box::new(Default::default());
            let decoded = unsafe { <&mut [(&u8,)]>::decode(c_slice, &mut *store) }.unwrap();
            decoded[0].0 = &x;
            store.sync();
        }
        assert_eq!(unsafe { *c_tuples[0].0 }, 10);
    }
}
