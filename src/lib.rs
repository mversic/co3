//! Structures and macros related to FFI and generation of FFI bindings. Any type that implements
//! [`ExternC`] can be used in the FFI bindings generated with [`export`]/[`extern_C!`]. It
//! is advisable to implement [`Ir`] and benefit from automatic implementation of [`ExternC`]
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

extern crate self as co3;

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};

#[cfg(feature = "derive")]
pub use co3_derive::*;
use derive_more::Display;
use disjoint_impls::disjoint_impls;

#[cfg(feature = "unstable-refs")]
use crate::cloned::DecodeCloned;
#[cfg(not(feature = "owned-as-ref"))]
use crate::out_ptr::co3_dealloc;
#[cfg(not(feature = "unsafe-optimizations"))]
use crate::transmute::EncodeTransmuted;
use crate::{
    cloned::{
        decode_cloned_array, decode_cloned_option_with_custom_niche,
        decode_cloned_option_without_niche,
    },
    ir::{Cloned, NonRobust, Opaque, ReprFamily, Robust, Transmuted},
    niche::{Niche, NicheFamily, WithCustomNiche, WithNiche, WithoutNiche},
    option::COption,
    slice::{CSlice, CSliceMut},
    transmute::{
        CheckedTransmute, transmute_from_target, transmute_from_target_ref_slice,
        transmute_from_target_slice_mut, transmute_into_target, transmute_into_target_ref_slice,
        transmute_into_target_slice_mut,
    },
};
#[cfg(feature = "alloc")]
use crate::{
    cloned::{decode_cloned_box_ptr, decode_cloned_collection},
    transmute::{
        transmute_from_target_boxed_slice, transmute_from_target_vec,
        transmute_into_target_boxed_slice, transmute_into_target_vec,
    },
};
#[cfg(feature = "alloc")]
#[cfg(not(feature = "owned-as-ref"))]
use crate::{slice::CBoxedSlice, vec::CVec};

pub mod cloned;
pub mod external;
pub mod handle;
pub mod ir;
pub mod niche;
pub mod option;
pub mod out_ptr;
mod primitives;
pub mod result;
pub mod slice;
mod std_impls;
pub mod transmute;
pub mod tuple;
pub mod vec;

#[cfg(feature = "alloc")]
type BoxedSliceCType<C> = CSliceMut<C>;
#[cfg(feature = "alloc")]
type VecCType<C> = CSliceMut<C>;

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

    impl<R: ReprFamily<Kind = Robust> + ReprC> ExternC for R {
        type CType = Self;
    }
    impl<R: ReprFamily<Kind = Opaque>> ExternC for R {
        type CType = *mut Self;
    }
    impl<R: ReprFamily<Kind = Transmuted> + CheckedTransmute<Target: ExternC>> ExternC for R {
        type CType = <R::Target as ExternC>::CType;
    }

    impl<'a, R: ExternC, S: Cloned> ExternC for &'a R
    where
        Self: ReprFamily<Kind = &'a S>,
    {
        type CType = *const R::CType;
    }

    impl<'a, R: ExternC, S: Cloned> ExternC for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut S>,
    {
        type CType = *mut R::CType;
    }

    #[cfg(feature = "alloc")]
    impl<R: ExternC, S: Cloned> ExternC for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
    {
        type CType = *mut R::CType;
    }

    impl<'a, R: ReprC> ExternC for &'a [R]
    where
        Self: ReprFamily<Kind = &'a [Robust]>,
    {
        type CType = CSlice<R>;
    }
    impl<'a, R> ExternC for &'a [R]
    where
        Self: ReprFamily<Kind = &'a [Opaque]>,
    {
        type CType = CSlice<*const R>;
    }
    impl<'slice, R: CheckedTransmute> ExternC for &'slice [R]
    where
        Self: ReprFamily<Kind = &'slice [Transmuted]>,
        &'slice [<R as CheckedTransmute>::Target]: ExternC,
    {
        type CType = <&'slice [R::Target] as ExternC>::CType;
    }
    impl<'a, R: ExternC, S: Cloned> ExternC for &'a [R]
    where
        Self: ReprFamily<Kind = &'a [S]>,
    {
        type CType = CSlice<R::CType>;
    }

    impl<'a, R: ReprC> ExternC for &'a mut [R]
    where
        Self: ReprFamily<Kind = &'a mut [Robust]>,
    {
        type CType = CSliceMut<R>;
    }
    impl<'slice, R> ExternC for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [Opaque]>,
    {
        type CType = CSliceMut<*mut R>;
    }
    impl<'slice, R: CheckedTransmute> ExternC for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [Transmuted]>,
        &'slice mut [<R as CheckedTransmute>::Target]: ExternC,
    {
        type CType = <&'slice mut [R::Target] as ExternC>::CType;
    }
    impl<'slice, R: ExternC, S: Cloned> ExternC for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [S]>,
    {
        type CType = CSliceMut<R::CType>;
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprC> ExternC for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Robust]>>,
    {
        type CType = BoxedSliceCType<R>;
    }
    #[cfg(feature = "alloc")]
    impl<R> ExternC for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Opaque]>>,
    {
        type CType = BoxedSliceCType<*mut R>;
    }
    #[cfg(feature = "alloc")]
    impl<R: CheckedTransmute> ExternC for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Transmuted]>>,
        Box<[<R as CheckedTransmute>::Target]>: ExternC,
    {
        type CType = <Box<[R::Target]> as ExternC>::CType;
    }
    #[cfg(feature = "alloc")]
    impl<R: ExternC, S: Cloned> ExternC for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[S]>>,
    {
        type CType = CSliceMut<R::CType>;
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprC> ExternC for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Robust>>,
    {
        type CType = VecCType<R>;
    }
    #[cfg(feature = "alloc")]
    impl<R> ExternC for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Opaque>>,
    {
        type CType = VecCType<*mut R>;
    }
    #[cfg(feature = "alloc")]
    impl<R: CheckedTransmute> ExternC for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Transmuted>>,
        Vec<<R as CheckedTransmute>::Target>: ExternC,
    {
        type CType = <Vec<R::Target> as ExternC>::CType;
    }
    #[cfg(feature = "alloc")]
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

    impl<R: ReprFamily<Kind = Robust> + ReprC> Encode for R {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            self
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Opaque>> Encode for R {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            Box::into_raw(Box::new(self))
        }
    }
    impl<
        #[cfg(not(feature = "unsafe-optimizations"))] R: EncodeTransmuted,
        #[cfg(feature = "unsafe-optimizations")] R,
    > Encode for R
    where
        R: ReprFamily<Kind = Transmuted> + CheckedTransmute<Target: Encode>,
    {
        #[cfg(not(feature = "unsafe-optimizations"))]
        type Store = R::Store;
        #[cfg(feature = "unsafe-optimizations")]
        type Store = <R::Target as Encode>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            #[cfg(not(feature = "unsafe-optimizations"))]
            {
                let target = R::encode_transmuted(self, store);
                let store =
                    unsafe { &mut *(store as *mut Self::Store as *mut <R::Target as Encode>::Store) };
                Encode::encode(target, store)
            }
            #[cfg(feature = "unsafe-optimizations")]
            {
                transmute_into_target(self).encode(store)
            }
        }
    }

    #[cfg(feature = "unstable-refs")]
    impl<'a, R: Encode + Clone, S: Cloned> Encode for &'a R
    where
        Self: ReprFamily<Kind = &'a S>,
    {
        type Store = RefStore<R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let value = self.clone().encode(&mut store.encode_store);
            store.ctype.insert(value)
        }
    }

    #[cfg(feature = "unstable-refs")]
    impl<'a, 'b, R: Encode + Decode<'b> + Clone + 'b, S: Cloned> Encode for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut S>,
    {
        type Store = RefMutStore<'a, R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let original = store.original.insert(self);
            let ctype = original.clone().encode(&mut store.encode_store);
            store.ctype.insert(ctype)
        }
    }

    #[cfg(feature = "alloc")]
    impl<R: Encode, S: Cloned> Encode for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
    {
        type Store = RefStore<R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let ctype = (*self).encode(&mut store.encode_store);
            store.ctype.insert(ctype)
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
    #[cfg(all(feature = "alloc", feature = "unstable-refs"))]
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
    #[cfg(all(feature = "alloc", feature = "unstable-refs"))]
    impl<'slice, R: Encode + Clone, S: Cloned> Encode for &'slice [R]
    where
        Self: ReprFamily<Kind = &'slice [S]>,
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
                self.iter()
                    .cloned()
                    .zip(stores)
                    .map(|(item, store)| item.encode(store))
                    .collect(),
            );

            CSlice::from_slice(Some(ctypes))
        }
    }

    impl<'slice, R: ReprC> Encode for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [Robust]>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            CSliceMut::from_slice(Some(self))
        }
    }
    #[cfg(all(feature = "alloc", feature = "unstable-refs"))]
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
            CSliceMut::from_slice(Some(store.ctypes.insert(ctypes)))
        }
    }
    impl<'slice, R: CheckedTransmute<Target: ReprFamily<Kind: NonRobust> + 'slice>> Encode
        for &'slice mut [R]
    where
        &'slice mut [<R as CheckedTransmute>::Target]: Encode,
        Self: ReprFamily<Kind = &'slice mut [Transmuted]>,
    {
        type Store = <&'slice mut [R::Target] as Encode>::Store;

        fn encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            transmute_into_target_slice_mut(self).encode(store)
        }
    }
    impl<
        'slice,
        R: CheckedTransmute<Target: ReprFamily<Kind = Robust> + ReprC + 'slice>
            + NicheFamily<Kind = WithoutNiche>,
    > Encode for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [Transmuted]>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            Encode::encode(transmute_into_target_slice_mut(self), &mut ())
        }
    }
    #[cfg(any(
        feature = "unsafe-optimizations",
        all(feature = "alloc", feature = "unstable-refs")
    ))]
    impl<
        'slice,
        R: CheckedTransmute<Target: ReprFamily<Kind = Robust> + ReprC + 'slice>
            + NicheFamily<Kind: WithNiche>,
    > Encode for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [Transmuted]>,
    {
        #[cfg(all(
            feature = "alloc",
            feature = "unstable-refs",
            not(feature = "unsafe-optimizations")
        ))]
        type Store = SliceMutTransmuteStore<'slice, R>;
        #[cfg(feature = "unsafe-optimizations")]
        type Store = ();

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            #[cfg(all(
                feature = "alloc",
                feature = "unstable-refs",
                not(feature = "unsafe-optimizations")
            ))]
            let ctypes: &mut [_] = {
                let original: &mut [R] = store.original.insert(self);
                let robust = transmute_into_target_slice_mut(original);
                store.target.insert(robust.to_vec().into_boxed_slice())
            };
            #[cfg(feature = "unsafe-optimizations")]
            let ctypes = transmute_into_target_slice_mut(self);

            Encode::encode(ctypes, &mut ())
        }
    }
    #[cfg(all(feature = "alloc", feature = "unstable-refs"))]
    impl<'slice, 'b, R: Encode + Decode<'b> + Clone + 'b, S: Cloned> Encode for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [S]>,
    {
        type Store = MutSliceStore<'slice, R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let original = store.original.insert(self);

            let stores = store.stores.insert(
                core::iter::repeat_with(Default::default)
                    .take(original.len())
                    .collect(),
            );

            let ctypes = store.ctypes.insert(
                original
                    .iter()
                    .cloned()
                    .zip(stores)
                    .map(|(item, store)| item.encode(store))
                    .collect(),
            );

            CSliceMut::from_slice(Some(ctypes))
        }
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprC> Encode for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Robust]>>,
    {
        #[cfg(feature = "owned-as-ref")]
        type Store = OwningStore<R>;
        #[cfg(not(feature = "owned-as-ref"))]
        type Store = ();

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            #[cfg(feature = "owned-as-ref")]
            let ctypes = {
                let store = store.0.insert(self);
                CSliceMut::from_slice(Some(store))
            };
            #[cfg(not(feature = "owned-as-ref"))]
            let ctypes = CBoxedSlice::from_boxed_slice(Some(self), co3_dealloc);

            ctypes
        }
    }
    #[cfg(feature = "alloc")]
    impl<R> Encode for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Opaque]>>,
    {
        type Store = OwningStore<*mut R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let boxed_ptrs = self.into_iter().map(Box::new).map(Box::into_raw).collect();

            let store = store.0.insert(boxed_ptrs);
            CSliceMut::from_slice(Some(store))
        }
    }
    #[cfg(feature = "alloc")]
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
    #[cfg(feature = "alloc")]
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
                    .zip(stores)
                    .map(|(item, store)| item.encode(store))
                    .collect(),
            );

            CSliceMut::from_slice(Some(ctypes))
        }
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprC> Encode for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Robust>>,
    {
        #[cfg(feature = "owned-as-ref")]
        type Store = OwningStore<R>;
        #[cfg(not(feature = "owned-as-ref"))]
        type Store = ();

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            #[cfg(feature = "owned-as-ref")]
            let ctypes = {
                let store = store.0.insert(self.into_boxed_slice());
                CSliceMut::from_slice(Some(store))
            };
            #[cfg(not(feature = "owned-as-ref"))]
            let ctypes = CVec::from_vec(Some(self), co3_dealloc);

            ctypes
        }
    }
    #[cfg(feature = "alloc")]
    impl<R> Encode for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Opaque>>,
    {
        type Store = OwningStore<*mut R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let boxed_ptrs = self.into_iter().map(Box::new).map(Box::into_raw).collect();

            let store = store.0.insert(boxed_ptrs);
            CSliceMut::from_slice(Some(store))
        }
    }
    #[cfg(feature = "alloc")]
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
    #[cfg(feature = "alloc")]
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
                    .zip(stores)
                    .map(|(item, store)| item.encode(store))
                    .collect(),
            );

            CSliceMut::from_slice(Some(ctypes))
        }
    }

    #[cfg(feature = "alloc")]
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
            self.map(|item| Box::into_raw(Box::new(item)))
        }
    }

    impl<R: Encode, S: Cloned, const N: usize> Encode for [R; N]
    where
        Self: ReprFamily<Kind = [S; N]>,
    {
        type Store = ArraySyncStore<R::Store, N>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            assert_arr_has_non_zero_len::<N>();

            let store = &mut store.0;

            let mut items = self.into_iter();
            let mut stores = store.iter_mut();

            core::array::from_fn(|_| {
                let item = items.next().unwrap();
                let store = stores.next().unwrap();

                item.encode(store)
            })
        }
    }

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

    impl<'d, R: ReprC + 'd> Decode<'d> for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            Some(source)
        }
    }
    #[cfg(feature = "alloc")]
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
    impl<'d, R: CheckedTransmute<Target: Decode<'d>>> Decode<'d> for R
    where
        Self: ReprFamily<Kind = Transmuted>,
    {
        type Store = <R::Target as Decode<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { Decode::decode(source, store).and_then(|inner| transmute_from_target(inner)) }
        }
    }

    #[cfg(feature = "unstable-refs")]
    impl<'d, R: DecodeCloned<'d>, S: Cloned> Decode<'d> for &'d R
    where
        Self: ReprFamily<Kind = &'d S>,
    {
        type Store = RefDecodeStore<R, R::Store>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let source = unsafe { source.read() };
            let value = unsafe { R::decode_cloned(source, &mut store.store)? };
            Some(store.value.insert(value))
        }
    }

    #[cfg(feature = "unstable-refs")]
    impl<'d, R: DecodeCloned<'d> + Encode, S: Cloned> Decode<'d> for &'d mut R
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

            let value = unsafe { R::decode_cloned(source, &mut store.decode_store)? };
            Some(store.value.insert(value))
        }
    }

    #[cfg(feature = "alloc")]
    impl<'d, R: Decode<'d>, S: Cloned> Decode<'d> for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
    {
        type Store = R::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { decode_cloned_box_ptr(source, store, |item, store| R::decode(item, store)) }
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
            transmute_from_target_ref_slice(unsafe { <&[R::Target]>::decode(source, store)? })
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
    #[cfg(all(feature = "alloc", feature = "unstable-refs"))]
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

            if source.iter().any(|item| item.is_null()) {
                return None;
            }

            let store = store.0.insert(
                source
                    .iter()
                    .map(|&item| unsafe { &*item }.clone())
                    .collect(),
            );

            Some(store)
        }
    }
    #[cfg(all(feature = "alloc", feature = "unstable-refs"))]
    impl<'slice, R: DecodeCloned<'slice>, S: Cloned> Decode<'slice> for &'slice [R]
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

            let values = store.values.insert(
                source
                    .iter()
                    .zip(stores)
                    .map(|(&item, store)| unsafe { R::decode_cloned(item, store) })
                    .collect::<Option<_>>()?,
            );

            Some(values)
        }
    }

    impl<'slice, R: CheckedTransmute> Decode<'slice> for &'slice mut [R]
    where
        &'slice mut [<R as CheckedTransmute>::Target]: Decode<'slice>,
        Self: ReprFamily<Kind = &'slice mut [Transmuted]>,
    {
        type Store = <&'slice mut [R::Target] as Decode<'slice>>::Store;

        unsafe fn decode<'itm: 'slice>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            transmute_from_target_slice_mut(unsafe { <&mut [R::Target]>::decode(source, store)? })
        }
    }
    impl<'slice, R: ReprC> Decode<'slice> for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [Robust]>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'slice>(source: Self::CType, (): &mut ()) -> Option<Self> {
            unsafe { source.into_rust() }
        }
    }
    #[cfg(all(feature = "alloc", feature = "unstable-refs"))]
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

            if source.iter().any(|item| item.is_null()) {
                return None;
            }

            let values = store.values.insert(
                source
                    .iter()
                    .map(|&item| unsafe { &*item }.clone())
                    .collect(),
            );

            Some(values)
        }
    }
    #[cfg(feature = "unstable-refs")]
    impl<'slice, R: DecodeCloned<'slice> + Encode, S: Cloned> Decode<'slice> for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [S]>,
    {
        type Store = MutSliceDecodeStore<R, <R as Decode<'slice>>::Store>;

        unsafe fn decode<'itm: 'slice>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source: &mut CSliceMut<_> = store.source.insert(source);
            let source: &mut [_] = unsafe { source.into_rust() }?;

            let stores = store.stores.insert(
                core::iter::repeat_with(Default::default)
                    .take(source.len())
                    .collect(),
            );

            let values = store.values.insert(
                source
                    .iter()
                    .zip(stores)
                    .map(|(&item, store)| unsafe { R::decode_cloned(item, store) })
                    .collect::<Option<_>>()?,
            );

            Some(values)
        }
    }

    #[cfg(all(feature = "alloc", feature = "owned-as-ref"))]
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
    #[cfg(all(feature = "alloc", feature = "owned-as-ref"))]
    impl<'d, R: ReprC + 'd> Decode<'d> for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Robust]>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            unsafe { source.into_rust() }.map(|slice| slice.into())
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: 'd> Decode<'d> for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Opaque]>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let source = unsafe { source.into_rust() }?;

            if source.iter().any(|item| item.is_null()) {
                return None;
            }

            let values = source
                .iter()
                .map(|&item| unsafe { *Box::from_raw(item) })
                .collect();

            Some(values)
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: Decode<'d>, S: Cloned> Decode<'d> for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[S]>>,
    {
        type Store = DecodeStoreSlice<R::Store>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { decode_cloned_collection(source, store, |item, store| R::decode(item, store)) }
        }
    }

    #[cfg(all(feature = "alloc", feature = "owned-as-ref"))]
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
    #[cfg(all(feature = "alloc", feature = "owned-as-ref"))]
    impl<'d, R: ReprC + 'd> Decode<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Robust>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            unsafe { source.into_rust() }.map(|slice| slice.to_vec())
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: 'd> Decode<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Opaque>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let source = unsafe { source.into_rust()? };

            if source.iter().any(|item| item.is_null()) {
                return None;
            }

            let values = source
                .iter()
                .map(|&item| unsafe { *Box::from_raw(item) })
                .collect();

            Some(values)
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: Decode<'d>, S: Cloned> Decode<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>>,
    {
        type Store = DecodeStoreSlice<R::Store>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { decode_cloned_collection(source, store, |item, store| R::decode(item, store)) }
        }
    }

    #[cfg(feature = "alloc")]
    impl<'d, R: 'd, const N: usize> Decode<'d> for [R; N]
    where
        Self: ReprFamily<Kind = [Opaque; N]>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            assert_arr_has_non_zero_len::<N>();

            if source.iter().any(|item| item.is_null()) {
                return None;
            }

            Some(source.map(|item| unsafe { *Box::from_raw(item) }))
        }
    }
    impl<'d, R: Decode<'d>, S: Cloned, const N: usize> Decode<'d> for [R; N]
    where
        Self: ReprFamily<Kind = [S; N]>,
    {
        type Store = ArraySyncStore<R::Store, N>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            decode_cloned_array(source, store, |item, store| unsafe { R::decode(item, store) })
        }
    }

    impl<'d, R: Decode<'d>> Decode<'d> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithoutNiche>>,
    {
        type Store = <R as Decode<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            decode_cloned_option_without_niche(source, store, |source, store| unsafe {
                R::decode(source, store)
            })
        }
    }
    impl<'d, R: Niche<CType: PartialEq> + Decode<'d>> Decode<'d> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithCustomNiche>>,
    {
        type Store = <R as Decode<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            decode_cloned_option_with_custom_niche(
                source,
                store,
                R::NICHE_VALUE,
                |source, store| unsafe { R::decode(source, store) },
            )
        }
    }
}

// TODO: Could the store just be synced on drop?
// FIXME: Encode types can never error during sync
pub trait Store {
    fn sync(self) -> Option<()>;
}

impl Store for () {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

#[cfg(feature = "alloc")]
pub struct DecodeStoreSlice<D>(Option<Box<[D]>>);

#[cfg(feature = "alloc")]
impl<D> Default for DecodeStoreSlice<D> {
    fn default() -> Self {
        Self(None)
    }
}

#[cfg(feature = "alloc")]
impl<D: Store> Store for DecodeStoreSlice<D> {
    fn sync(self) -> Option<()> {
        for store in self.0.unwrap() {
            store.sync()?;
        }

        Some(())
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
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

#[cfg(feature = "alloc")]
pub struct OwningStore<T>(Option<Box<[T]>>);

#[cfg(feature = "alloc")]
impl<T> Default for OwningStore<T> {
    fn default() -> Self {
        Self(None)
    }
}

#[cfg(feature = "alloc")]
impl<T> Store for OwningStore<T> {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

#[cfg(feature = "alloc")]
pub struct DecodeStoreSlicePair<R, D> {
    #[allow(dead_code)]
    values: Option<Box<[R]>>,
    stores: Option<Box<[D]>>,
}

#[cfg(feature = "alloc")]
impl<R, D> Default for DecodeStoreSlicePair<R, D> {
    fn default() -> Self {
        Self {
            stores: None,
            values: None,
        }
    }
}

#[cfg(feature = "alloc")]
impl<R, D: Store> Store for DecodeStoreSlicePair<R, D> {
    #[cfg(feature = "alloc")]
    fn sync(self) -> Option<()> {
        for store in self.stores.unwrap() {
            store.sync()?;
        }

        Some(())
    }
}

#[cfg(all(feature = "alloc", feature = "unstable-refs"))]
pub struct OpaqueMutSliceEncodeStore<'a, R> {
    ctypes: Option<Box<[*mut R]>>,
    original: Option<&'a mut [R]>,
}

#[cfg(all(feature = "alloc", feature = "unstable-refs"))]
impl<'a, R> Default for OpaqueMutSliceEncodeStore<'a, R> {
    fn default() -> Self {
        Self {
            ctypes: None,
            original: None,
        }
    }
}

#[cfg(all(feature = "alloc", feature = "unstable-refs"))]
impl<'a, R> Store for OpaqueMutSliceEncodeStore<'a, R> {
    fn sync(self) -> Option<()> {
        let ctypes = self.ctypes.unwrap();

        // TODO: this can be disabled if unsafe-optimizations
        // is active, but is it worth it? Quite unlikely it is
        if ctypes.iter().any(|ptr| ptr.is_null()) {
            return None;
        }

        for (orig, ptr) in self.original.unwrap().iter_mut().zip(ctypes) {
            if !core::ptr::eq(orig, ptr) {
                *orig = unsafe { ptr.read() };
            }
        }

        Some(())
    }
}

#[cfg(any(feature = "unstable-refs", feature = "owned-as-ref"))]
pub struct RefStore<R: Encode> {
    ctype: Option<R::CType>,
    encode_store: R::Store,
}

#[cfg(any(feature = "unstable-refs", feature = "owned-as-ref"))]
impl<R: Encode> Default for RefStore<R> {
    fn default() -> Self {
        Self {
            ctype: None,
            encode_store: Default::default(),
        }
    }
}

#[cfg(any(feature = "unstable-refs", feature = "owned-as-ref"))]
impl<R: Encode> Store for RefStore<R> {
    fn sync(self) -> Option<()> {
        self.encode_store.sync()
    }
}

#[cfg(feature = "unstable-refs")]
pub struct RefMutStore<'a, R: Encode> {
    ctype: Option<R::CType>,
    encode_store: R::Store,
    original: Option<&'a mut R>,
}

#[cfg(feature = "unstable-refs")]
impl<'a, R: Encode> Default for RefMutStore<'a, R> {
    fn default() -> Self {
        Self {
            ctype: None,
            encode_store: Default::default(),
            original: None,
        }
    }
}

#[cfg(feature = "unstable-refs")]
impl<'a, 'b, R: Encode + Decode<'b> + 'b> Store for RefMutStore<'a, R> {
    fn sync(self) -> Option<()> {
        #[cfg(not(feature = "unsafe-optimizations"))]
        const {
            assert!(
                impls::impls!(R: crate::out_ptr::NonLocal),
                "Not yet implemented"
            );
        }

        if let (Some(ctype), Some(original)) = (self.ctype, self.original) {
            let mut decode_store = Default::default();

            let store_ref = unsafe {
                core::mem::transmute::<&mut <R as Decode>::Store, &'b mut <R as Decode>::Store>(
                    &mut decode_store,
                )
            };

            *original = unsafe { R::decode(ctype, store_ref)? };
        }

        Some(())
    }
}

#[cfg(feature = "alloc")]
pub struct SliceStore<C, D> {
    ctypes: Option<Box<[C]>>,
    stores: Option<Box<[D]>>,
}

#[cfg(feature = "alloc")]
impl<C, D> Default for SliceStore<C, D> {
    fn default() -> Self {
        Self {
            ctypes: None,
            stores: None,
        }
    }
}

#[cfg(feature = "alloc")]
impl<C, D: Store> Store for SliceStore<C, D> {
    fn sync(self) -> Option<()> {
        for store in self.stores.unwrap() {
            store.sync()?;
        }

        Some(())
    }
}

#[cfg(all(feature = "alloc", feature = "unstable-refs"))]
pub struct MutSliceStore<'slice, R: Encode> {
    ctypes: Option<Box<[R::CType]>>,
    stores: Option<Box<[R::Store]>>,
    original: Option<&'slice mut [R]>,
}

#[cfg(all(feature = "alloc", feature = "unstable-refs"))]
impl<'slice, R: Encode> Default for MutSliceStore<'slice, R> {
    fn default() -> Self {
        Self {
            ctypes: None,
            stores: None,
            original: None,
        }
    }
}

#[cfg(all(feature = "alloc", feature = "unstable-refs"))]
impl<'slice, 'b, R: Encode + Decode<'b> + 'b> Store for MutSliceStore<'slice, R> {
    fn sync(self) -> Option<()> {
        const {
            #[cfg(not(feature = "unsafe-optimizations"))]
            assert!(
                impls::impls!(R: crate::out_ptr::NonLocal),
                "Not yet implemented"
            );
        }

        if let (Some(borrows), Some(ctypes)) = (self.original, self.ctypes) {
            let mut decode_store = Default::default();

            for (original, ctypes) in borrows.iter_mut().zip(ctypes) {
                let store_ref = unsafe {
                    core::mem::transmute::<&mut <R as Decode>::Store, &'b mut <R as Decode>::Store>(
                        &mut decode_store,
                    )
                };

                *original = unsafe { R::decode(ctypes, store_ref)? };
            }
        }

        Some(())
    }
}

#[cfg(all(
    feature = "alloc",
    feature = "unstable-refs",
    not(feature = "unsafe-optimizations")
))]
pub struct SliceMutTransmuteStore<'slice, R: CheckedTransmute> {
    target: Option<Box<[R::Target]>>,
    original: Option<&'slice mut [R]>,
}

#[cfg(all(
    feature = "alloc",
    feature = "unstable-refs",
    not(feature = "unsafe-optimizations")
))]
impl<'slice, R: CheckedTransmute> Default for SliceMutTransmuteStore<'slice, R> {
    fn default() -> Self {
        Self {
            target: None,
            original: None,
        }
    }
}

#[cfg(all(
    feature = "alloc",
    feature = "unstable-refs",
    not(feature = "unsafe-optimizations")
))]
impl<'slice, R: CheckedTransmute> Store for SliceMutTransmuteStore<'slice, R> {
    fn sync(self) -> Option<()> {
        if let (Some(borrows), Some(target)) = (self.original, self.target) {
            let target = crate::transmute::transmute_from_target_boxed_slice(target)?;

            for (original, t) in borrows.iter_mut().zip(target) {
                *original = t;
            }
        }

        Some(())
    }
}

pub struct ArraySyncStore<D, const N: usize>([D; N]);

impl<D: Default, const N: usize> Default for ArraySyncStore<D, N> {
    fn default() -> Self {
        // FIXME: https://github.com/rust-lang/rust/issues/61415
        Self(core::array::from_fn(|_| D::default()))
    }
}

impl<D: Store, const N: usize> Store for ArraySyncStore<D, N> {
    fn sync(self) -> Option<()> {
        for store in self.0 {
            store.sync()?;
        }

        Some(())
    }
}

#[cfg(feature = "unstable-refs")]
pub struct RefMutDecodeStore<R: ExternC, DS> {
    value: Option<R>,
    decode_store: DS,
    source: Option<*mut R::CType>,
}

#[cfg(feature = "unstable-refs")]
impl<R: ExternC, DS: Default> Default for RefMutDecodeStore<R, DS> {
    fn default() -> Self {
        Self {
            value: None,
            decode_store: Default::default(),
            source: None,
        }
    }
}

#[cfg(feature = "unstable-refs")]
impl<R: Encode, DS: Store> Store for RefMutDecodeStore<R, DS> {
    fn sync(self) -> Option<()> {
        let mut encode_store = Default::default();
        let ctypes = self.value.unwrap().encode(&mut encode_store);
        unsafe { *self.source.unwrap() = ctypes };
        Some(())
    }
}

#[cfg(feature = "unstable-refs")]
pub struct MutSliceDecodeStore<R: ExternC, DS> {
    values: Option<Box<[R]>>,
    stores: Option<Box<[DS]>>,
    source: Option<CSliceMut<R::CType>>,
}

#[cfg(feature = "unstable-refs")]
impl<R: ExternC, DS: Default> Default for MutSliceDecodeStore<R, DS> {
    fn default() -> Self {
        Self {
            values: None,
            stores: None,
            source: None,
        }
    }
}

#[cfg(feature = "unstable-refs")]
impl<R: Encode, DS: Store> Store for MutSliceDecodeStore<R, DS> {
    fn sync(self) -> Option<()> {
        let mut encode_store = Default::default();

        let source = unsafe { self.source.unwrap().into_rust().unwrap() };
        for (src, decoded) in source.iter_mut().zip(self.values.unwrap()) {
            *src = decoded.encode(&mut encode_store);
        }

        Some(())
    }
}

#[cfg(all(feature = "alloc", feature = "unstable-refs"))]
pub struct OpaqueMutSliceDecodeStore<R> {
    values: Option<Box<[R]>>,
    source: Option<CSliceMut<*mut R>>,
}

#[cfg(all(feature = "alloc", feature = "unstable-refs"))]
impl<R> Default for OpaqueMutSliceDecodeStore<R> {
    fn default() -> Self {
        Self {
            values: None,
            source: None,
        }
    }
}

#[cfg(all(feature = "alloc", feature = "unstable-refs"))]
impl<R> Store for OpaqueMutSliceDecodeStore<R> {
    fn sync(self) -> Option<()> {
        let slice = unsafe { self.source.unwrap().into_rust().unwrap() };

        for (&mut src, decoded) in slice.iter_mut().zip(self.values.unwrap()) {
            unsafe { *src = decoded };
        }

        Some(())
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
/// // a change the unsafe code in [`co3::reprC!`] will not compile, thus preventing UB
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
/// co3::reprC! {
///     // SAFETY: Type MUST NOT have traps
///     unsafe impl Robust for RobustStruct {}
/// }
///
/// co3::reprC! {
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
/// co3::reprC! {
///     unsafe impl Transparent for Wrapper {
///         type Target = WrapperInner;
///     }
/// }
/// ```
#[macro_export]
macro_rules! reprC {
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
                    assert!(impls::impls!
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

            unsafe impl $(<$($params)*>)? $crate::transmute::EncodeTransmuted for $self_ty
            where
                $target: $crate::Encode,
                $($($preds)*)?
            {
                type Store = <$target as $crate::Encode>::Store;
            }
        };
        (unsafe impl Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
            type Target = $target:ty;
        }) => {
            $crate::reprC! {
                @transparent [for<'_dummy>] [] $self_ty $([$($preds)*])? {
                    type Target = $target;
                    // NOTE: When delegating there is no trap representations in the immediate `Self::Target`
                    // Whether `Self::Target` itself has trap representations is not to be considered here
                    fn is_valid(_target: &Self::Target) -> bool { true }
                }
            }
        };
        (unsafe impl ( $($params:tt)* ) Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
            type Target = $target:ty;
        }) => {
            $crate::reprC! {
                @transparent [] [<$($params)*>] $self_ty $([$($preds)*])? {
                    type Target = $target;
                    // NOTE: When delegating there is no trap representations in the immediate `Self::Target`
                    // Whether `Self::Target` itself has trap representations is not to be considered here
                    fn is_valid(_target: &Self::Target) -> bool { true }
                }
            }
        };
        (unsafe impl Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
            type Target = $target:ty;

            fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
                $block:block
        }) => {
            $crate::reprC! {
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
            $crate::reprC! {
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
            unsafe impl $($impl_generics)* $crate::transmute::EncodeTransmuted for $self_ty
            where
                $($for_dummy)* $target: $crate::Encode,
                $($($preds)*)?
            {
                type Store = <$target as $crate::Encode>::Store;
            }
        };
    }

reprC! {
    unsafe impl(R) Robust for *const R {}
}
reprC! {
    unsafe impl(R) Robust for *mut R {}
}

// SAFETY: Array is just a contiguous block of memory
unsafe impl<R: ReprC, const N: usize> ReprC for [R; N] {}

// TODO: Check https://github.com/mversic/co3/issues/13
const fn assert_arr_has_non_zero_len<const N: usize>() {
    assert!(N != 0, "empty array is a ZST");
}

#[cfg(test)]
mod tests {
    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    use crate::niche::{StableNiche, WithStableNiche};

    #[test]
    fn robust_u8() {
        assert_impl_all!(u8:
            ReprFamily<Kind = Robust>,
            NicheFamily<Kind = WithoutNiche>,
            Decode<'static>,
            Encode,
            ReprC,
        );
        assert_impl_all!(&u8:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut u8:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );
        // FIXME:
        //assert_impl_all!(Box<u8>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut u8>,
        //    Decode<'static>,
        //    Encode,
        //);
        assert_impl_all!(&[u8]:
            ReprFamily<Kind = &'static [Robust]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [u8]:
            ReprFamily<Kind = &'static mut [Robust]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[u8]>:
            ReprFamily<Kind = Box<[Robust]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<u8>:
            ReprFamily<Kind = Vec<Robust>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([u8; 2]:
            ReprFamily<Kind = Robust>,
            NicheFamily<Kind = WithoutNiche>,
            Decode<'static>,
            Encode,
            ReprC,
        );
        assert_impl_all!(Option<u8>:
            ReprFamily<Kind = Option<WithoutNiche>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = COption<u8>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(u8: CheckedTransmute);
        assert_not_impl_any!(&u8: ReprC);
        assert_not_impl_any!(&mut u8: ReprC);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<u8>: ReprC);
        assert_not_impl_any!(&[u8]: ReprC, StableNiche);
        assert_not_impl_any!(&mut [u8]: ReprC, StableNiche);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<[u8]>: ReprC, StableNiche);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Vec<u8>: ReprC, StableNiche);
        assert_not_impl_any!([u8; 2]: CheckedTransmute, Niche);
        assert_not_impl_any!(Option<u8>: ReprC, StableNiche);
    }

    #[test]
    fn const_ptr_is_robust() {
        assert_impl_all!(*const bool:
            ReprFamily<Kind = Robust>,
            NicheFamily<Kind = WithoutNiche>,
            Decode<'static>,
            Encode,
            ReprC,
        );
        assert_impl_all!(&*const bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const *const bool>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut *const bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut *const bool>,
            Decode<'static>,
            Encode,
        );
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<*const bool>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut *const bool>,
        //    Decode<'static>,
        //    Encode,
        //);
        assert_impl_all!(&[*const bool]:
            ReprFamily<Kind = &'static [Robust]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*const bool>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [*const bool]:
            ReprFamily<Kind = &'static mut [Robust]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*const bool>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[*const bool]>:
            ReprFamily<Kind = Box<[Robust]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*const bool>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<*const bool>:
            ReprFamily<Kind = Vec<Robust>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*const bool>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([*const bool; 2]:
            ReprFamily<Kind = Robust>,
            NicheFamily<Kind = WithoutNiche>,
            Decode<'static>,
            Encode,
            ReprC,
        );
        assert_impl_all!(Option<*const bool>:
            ReprFamily<Kind = Option<WithoutNiche>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = COption<*const bool>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(*const bool: CheckedTransmute);
        assert_not_impl_any!(&*const bool: ReprC);
        assert_not_impl_any!(&mut *const bool: ReprC);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<*const bool>: ReprC);
        assert_not_impl_any!(&[*const bool]: ReprC, StableNiche);
        assert_not_impl_any!(&mut [*const bool]: ReprC, StableNiche);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<[*const bool]>: ReprC, StableNiche);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Vec<*const bool>: ReprC, StableNiche);
        assert_not_impl_any!([*const bool; 2]: CheckedTransmute, Niche);
        assert_not_impl_any!(Option<*const bool>: ReprC, StableNiche);
    }

    #[test]
    #[cfg(feature = "unstable-refs")]
    fn encode_cloned_mut_ref() {
        use crate::option::COption;

        let inner = 8u8;
        let other = 42u8;
        let mut value = Some(inner);
        let value_mut_ref: &mut Option<u8> = &mut value;
        {
            let mut store: Box<<&mut Option<u8> as Encode>::Store> = Box::new(Default::default());
            let encoded = value_mut_ref.encode(&mut *store);
            unsafe {
                *encoded = COption::Some(other);
            }
            store.sync().unwrap();
        }
        assert_eq!(value, Some(42u8));

        let mut slice = [Some(1u8)];
        let ref_mut: &mut [_] = &mut slice;
        {
            let mut store: Box<<&mut [Option<u8>] as Encode>::Store> = Box::new(Default::default());
            let encoded = ref_mut.encode(&mut *store);
            let c_slice = unsafe { encoded.into_rust().unwrap() };
            c_slice[0] = COption::Some(other);
            store.sync().unwrap();
        }
        assert_eq!(slice, [Some(42u8)]);
    }

    #[test]
    #[cfg(feature = "unstable-refs")]
    fn decode_cloned_mut_ref() {
        use crate::{option::COption, slice::CSliceMut};

        let mut c_opt = COption::Some(1u8);
        let c_ptr: *mut _ = &mut c_opt;
        let new_val: u8 = 42;
        {
            let mut store: Box<<&mut Option<u8> as Decode>::Store> = Box::new(Default::default());
            let decoded = unsafe { <&mut Option<u8>>::decode(c_ptr, &mut *store) }.unwrap();
            *decoded = Some(new_val);
            store.sync().unwrap();
        }
        assert_eq!(c_opt, COption::Some(42u8));

        let mut c_opts = [COption::Some(1u8)];
        let c_slice = CSliceMut::from_slice(Some(&mut c_opts));
        let x: u8 = 10;
        {
            let mut store: Box<<&mut [Option<u8>] as Decode>::Store> = Box::new(Default::default());
            let decoded = unsafe { <&mut [Option<u8>]>::decode(c_slice, &mut *store) }.unwrap();
            decoded[0] = Some(x);
            store.sync().unwrap();
        }
        assert_eq!(c_opts[0], COption::Some(10u8));
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "unstable-refs"))]
    fn encode_opaque_ref_mut_slice() {
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

            store.sync().unwrap();
        }
        assert_eq!(items[0].value, 100);
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "unstable-refs"))]
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
            store.sync().unwrap();
        }

        assert_eq!(item1.value, 100);
    }
}
