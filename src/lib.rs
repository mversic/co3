//! Structures and macros related to FFI and generation of FFI bindings. Any type that implements
//! [`ExternC`] can be used in the FFI bindings generated with [`export`]/[`extern_C!`]. It
//! is advisable to implement [`Ir`] and benefit from automatic implementation of [`ExternC`]
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc as alloc_crate;
extern crate self as co3;

#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};

#[cfg(feature = "derive")]
pub use co3_derive::*;
use derive_more::Display;

use disjoint_impls::disjoint_impls;
// TODO: I don't like having to reexport macros from other crates
#[doc(hidden)]
pub use impls::impls;

#[cfg(not(feature = "unsafe-optimizations"))]
use crate::transmute::EncodeTransmuted;
#[cfg(feature = "alloc")]
use crate::{
    boxed::{CBox, CBoxedSlice},
    transmute::{transmute_from_target_boxed_dst, transmute_into_target_boxed_dst},
};
use crate::{
    cloned::{
        DecodeCloned, decode_cloned_array, decode_cloned_option_with_custom_niche,
        decode_cloned_option_without_niche,
    },
    dst::{DstFamily, ExternTypeLike, Sized_, SliceDst, SliceLike},
    external::{ExternRef, ExternRefMut, External},
    ir::{Cloned, Opaque, ReprFamily, Robust, Transmuted},
    niche::{Niche, WithCustomNiche, WithoutNiche},
    option::COption,
    out_ptr::Zst,
    slice::{CSlice, CSliceMut},
    transmute::{
        CheckedTransmute, transmute_from_target, transmute_from_target_dst_mut,
        transmute_from_target_ref_dst, transmute_into_target, transmute_into_target_dst_mut,
        transmute_into_target_ref_dst,
    },
};

#[cfg(feature = "alloc")]
pub mod alloc;
pub mod borrow;
#[cfg(feature = "alloc")]
pub mod boxed;
pub mod cloned;
pub mod dst;
pub mod external;
pub mod handle;
pub mod heapify;
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
pub unsafe trait ReprC: Sized + Copy {}

/// `ReprC` type that is allowed as a C function argument type.
///
/// # Safety
///
/// Type must be allowed as a C function argument type
pub unsafe trait FnArg: ReprC {}

disjoint_impls! {
    /// A Rust type that has an `extern "C"` ABI
    pub trait ExternC: Sized {
        /// The C-compatible representation of this Rust type.
        type CType: ReprC;
    }

    impl<R: ReprC> ExternC for R
    where
       Self: ReprFamily<Kind = Robust>,
    {
        type CType = Self;
    }
    #[cfg(feature = "alloc")]
    impl<R> ExternC for R
    where
    Self: ReprFamily<Kind = Opaque>,
    {
        type CType = CBox<Self>;
    }
    impl<R: CheckedTransmute<Target: ExternC>> ExternC for R
    where
        Self: ReprFamily<Kind = Transmuted>,
    {
        type CType = <R::Target as ExternC>::CType;
    }

    impl<'a, R: ?Sized + SliceDst<Elem: ReprC>> ExternC for &'a R
    where
        Self: ReprFamily<Kind = &'a Robust>,
        R: DstFamily<Kind = SliceLike>,
    {
        type CType = CSlice<R::Elem>;
    }
    impl<'a, R: ?Sized + SliceDst> ExternC for &'a R
    where
        Self: ReprFamily<Kind = &'a Opaque>,
        R: DstFamily<Kind = SliceLike>,
    {
        type CType = CSlice<R::Elem>;
    }
    impl<'a, R: ?Sized + CheckedTransmute> ExternC for &'a R
    where
        &'a <R as CheckedTransmute>::Target: ExternC,
        Self: ReprFamily<Kind = &'a Transmuted>,
        R: DstFamily<Kind = SliceLike>,
    {
        type CType = <&'a R::Target as ExternC>::CType;
    }
    impl<'a, R> ExternC for &'a R
    where
        Self: ReprFamily<Kind = &'a Transmuted>,
        R: DstFamily<Kind = ExternTypeLike>,
        ExternRef<'a, R>: ExternC,
    {
        type CType = <ExternRef<'a, R> as ExternC>::CType;
    }
    impl<'a, R: ExternC, S: Cloned + 'a> ExternC for &'a R
    where
        Self: ReprFamily<Kind = &'a S>,
        R: DstFamily<Kind = Sized_>,
    {
        type CType = *const R::CType;
    }
    impl<'a, R: ?Sized + SliceDst<Elem: ExternC>, S: Cloned + ?Sized + 'a> ExternC for &'a R
    where
        Self: ReprFamily<Kind = &'a S>,
        R: DstFamily<Kind = SliceLike>,
    {
        type CType = CSlice<<R::Elem as ExternC>::CType>;
    }

    impl<'a, R: ?Sized + SliceDst<Elem: ReprC>> ExternC for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut Robust>,
        R: DstFamily<Kind = SliceLike>,
    {
        type CType = CSliceMut<R::Elem>;
    }
    impl<'a, R: ?Sized + SliceDst> ExternC for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut Opaque>,
        R: DstFamily<Kind = SliceLike>,
    {
        type CType = CSliceMut<R::Elem>;
    }
    impl<'a, R: ?Sized + CheckedTransmute> ExternC for &'a mut R
    where
        &'a mut <R as CheckedTransmute>::Target: ExternC,
        Self: ReprFamily<Kind = &'a mut Transmuted>,
        R: DstFamily<Kind = SliceLike>,
    {
        type CType = <&'a mut R::Target as ExternC>::CType;
    }
    impl<'a, R> ExternC for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut Transmuted>,
        R: DstFamily<Kind = ExternTypeLike>,
        ExternRefMut<'a, R>: ExternC,
    {
        type CType = <ExternRefMut<'a, R> as ExternC>::CType;
    }
    impl<'a, R: ExternC, S: Cloned + 'a> ExternC for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut S>,
        R: DstFamily<Kind = Sized_>,
    {
        type CType = *mut R::CType;
    }
    impl<'a, R: ?Sized + SliceDst<Elem: ExternC>, S: Cloned + ?Sized + 'a> ExternC for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut S>,
        R: DstFamily<Kind = SliceLike>,
    {
        type CType = CSliceMut<<R::Elem as ExternC>::CType>;
    }

    #[cfg(feature = "alloc")]
    impl<R: ?Sized + SliceDst<Elem: ReprC>> ExternC for Box<R>
    where
        Self: ReprFamily<Kind = Box<Robust>>,
        R: DstFamily<Kind = SliceLike>,
    {
        type CType = CBoxedSlice<R::Elem>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized + SliceDst> ExternC for Box<R>
    where
        Self: ReprFamily<Kind = Box<Opaque>>,
        R: DstFamily<Kind = SliceLike>,
    {
        type CType = CBoxedSlice<R::Elem>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized + CheckedTransmute> ExternC for Box<R>
    where
        Box<<R as CheckedTransmute>::Target>: ExternC,
        Self: ReprFamily<Kind = Box<Transmuted>>,
        R: DstFamily<Kind = SliceLike>,
    {
        type CType = <Box<R::Target> as ExternC>::CType;
    }
    #[cfg(feature = "alloc")]
    impl<R: External + ExternC> ExternC for Box<R>
    where
        Self: ReprFamily<Kind = Box<Transmuted>>,
        R: DstFamily<Kind = ExternTypeLike>,
    {
        type CType = <R as ExternC>::CType;
    }
    #[cfg(feature = "alloc")]
    impl<R: ExternC, S: Cloned> ExternC for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
        R: DstFamily<Kind = Sized_>,
    {
        type CType = CBox<R::CType>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized + SliceDst<Elem: ExternC>, S: Cloned + ?Sized> ExternC for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
        R: DstFamily<Kind = SliceLike>,
    {
        type CType = CBoxedSlice<<R::Elem as ExternC>::CType>;
    }

    #[cfg(feature = "alloc")]
    impl<R, S> ExternC for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>>,
        Box<[R]>: ExternC,
    {
        type CType = <Box<[R]> as ExternC>::CType;
    }

    impl<R: ExternC, S: Cloned, const N: usize> ExternC for [R; N]
    where
        Self: ReprFamily<Kind = S>,
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
    pub trait EncodeWithStore<const UNSAFE_OPTIMIZATIONS: bool = false>: ExternC {
        /// Auxiliary storage used during conversion. If storage is not used, set the type to `()`.
        ///
        /// Use cases include:
        /// - Keeping the result of the conversion of references of [`Cloned`] types
        /// - Storing mutable references that need to be updated in [`Store::sync`]
        ///
        /// Conceptually, serves a role similar to the "context" captured by a closure.
        type Store: Store + Default;

        /// Convert from [`Self`] into [`Self::CType`].
        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm;
    }

    impl<R: ReprC> EncodeWithStore<false> for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            self
        }
    }
    #[cfg(feature = "alloc")]
    impl<R> EncodeWithStore<false> for R
    where
        Self: ReprFamily<Kind = Opaque>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            CBox::from_box(Some(Box::new(self)))
        }
    }
    impl<
        #[cfg(not(feature = "unsafe-optimizations"))] R: EncodeTransmuted,
        #[cfg(feature = "unsafe-optimizations")] R,
    > EncodeWithStore<false> for R
    where
        R: ReprFamily<Kind = Transmuted> + CheckedTransmute<Target: EncodeWithStore>,
    {
        #[cfg(not(feature = "unsafe-optimizations"))]
        type Store = R::Store;
        #[cfg(feature = "unsafe-optimizations")]
        type Store = <R::Target as EncodeWithStore>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            #[cfg(not(feature = "unsafe-optimizations"))]
            {
                let target = R::encode_transmuted(self, store);
                let store =
                    unsafe { &mut *(store as *mut Self::Store as *mut <R::Target as EncodeWithStore>::Store) };
                EncodeWithStore::encode(target, store)
            }
            #[cfg(feature = "unsafe-optimizations")]
            {
                EncodeWithStore::encode(transmute_into_target(self), store)
            }
        }
    }

    impl<'a, R: ?Sized + SliceDst<Elem: ReprC>> EncodeWithStore<false> for &'a R
    where
        Self: ReprFamily<Kind = &'a Robust>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            CSlice::from_slice(Some(unsafe {
                core::slice::from_raw_parts(self.as_ptr(), self.len())
            }))
        }
    }
    //impl<'a, R: ?Sized + Dst> EncodeWithStore<false> for &'a R
    //where
    //    Self: ReprFamily<Kind = &'a Opaque>,
    //{
    //    type Store = ();
    //
    //    fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    //    where
    //        Self: 'itm,
    //    {
    //        let ctypes = self.iter().map(core::ptr::from_ref).collect();
    //        CSlice::from_slice(Some(store.0.insert(ctypes)))
    //    }
    //}
    impl<'a, R: ?Sized + SliceDst + CheckedTransmute<Target: SliceDst + 'a>> EncodeWithStore<false> for &'a R
    where
        &'a <R as CheckedTransmute>::Target: EncodeWithStore,
        Self: ReprFamily<Kind = &'a Transmuted>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = <&'a R::Target as EncodeWithStore>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            EncodeWithStore::encode(transmute_into_target_ref_dst(self), store)
        }
    }
    impl<'a, R: External> EncodeWithStore<false> for &'a R
    where
        Self: ReprFamily<Kind = &'a Transmuted>,
        R: DstFamily<Kind = ExternTypeLike>,
        ExternRef<'a, R>: Encode,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            Encode::encode(ExternRef::new(self))
        }
    }
    impl<'a, R: EncodeWithStore + Clone, S: Cloned + 'a> EncodeWithStore<false> for &'a R
    where
        Self: ReprFamily<Kind = &'a S>,
        R: DstFamily<Kind = Sized_>,
    {
        type Store = RefStore<R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let value = EncodeWithStore::encode(self.clone(), &mut store.encode_store);
            store.ctype.insert(value)
        }
    }
    impl<'a, R: ?Sized + SliceDst<Elem: EncodeWithStore + Clone>, S: Cloned + ?Sized + 'a> EncodeWithStore<false> for &'a R
    where
        Self: ReprFamily<Kind = &'a S>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = SliceStore<<R::Elem as ExternC>::CType, <R::Elem as EncodeWithStore>::Store>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let source = unsafe { core::slice::from_raw_parts(self.as_ptr(), self.len()) };

            let stores = store.stores.insert(
                core::iter::repeat_with(Default::default)
                    .take(source.len())
                    .collect(),
            );

            let ctypes = store.ctypes.insert(
                source
                    .iter()
                    .cloned()
                    .zip(stores)
                    .map(|(item, store)| EncodeWithStore::encode(item, store))
                    .collect(),
            );

            CSlice::from_slice(Some(ctypes))
        }
    }

    impl<'a, R: ?Sized + SliceDst<Elem: ReprC>> EncodeWithStore<false> for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut Robust>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            CSliceMut::from_slice(Some(unsafe {
                core::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len())
            }))
        }
    }
    //impl<'a, R: ?Sized + Dst> EncodeWithStore<false> for &'a mut R
    //where
    //    Self: ReprFamily<Kind = &'a mut Opaque>,
    //{
    //    type Store = ();
    //
    //    fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    //    where
    //        Self: 'itm,
    //    {
    //        let original = store.original.insert(self);
    //        let ctypes = original.iter_mut().map(core::ptr::from_mut).collect();
    //        CSliceMut::from_slice(Some(store.ctypes.insert(ctypes)))
    //    }
    //}
    impl<'a, R: ?Sized + SliceDst + CheckedTransmute<Target: SliceDst + 'a>> EncodeWithStore<false> for &'a mut R
    where
        &'a mut <R as CheckedTransmute>::Target: EncodeWithStore,
        Self: ReprFamily<Kind = &'a mut Transmuted>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = <&'a mut R::Target as EncodeWithStore>::Store;

        fn encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            EncodeWithStore::encode(transmute_into_target_dst_mut(self), store)
        }
    }
    impl<'a, R: External> EncodeWithStore<false> for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut Transmuted>,
        R: DstFamily<Kind = ExternTypeLike>,
        ExternRefMut<'a, R>: Encode,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            Encode::encode(ExternRefMut::new(self))
        }
    }
    impl<'a, R: EncodeWithStore + DecodeWithStore<'a, false> + Clone, S: Cloned + 'a> EncodeWithStore<false> for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut S>,
        R: DstFamily<Kind = Sized_>,
    {
        type Store = RefMutStore<'a, R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let original = store.original.insert(self);
            let ctype = EncodeWithStore::encode(original.clone(), &mut store.encode_store);
            store.ctype.insert(ctype)
        }
    }
    impl<'a, R: ?Sized + SliceDst<Elem: DecodeWithStore<'a> + EncodeWithStore + Clone + 'a>, S: Cloned + ?Sized + 'a> EncodeWithStore<false> for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut S>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = MutSliceStore<'a, R::Elem>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let source = unsafe { core::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len()) };
            let original = store.original.insert(source);

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
                    .map(|(item, store)| EncodeWithStore::encode(item, store))
                    .collect(),
            );

            CSliceMut::from_slice(Some(ctypes))
        }
    }

    #[cfg(feature = "alloc")]
    impl<R: ?Sized + SliceDst<Elem: ReprC>> EncodeWithStore<false> for Box<R>
    where
        Self: ReprFamily<Kind = Box<Robust>>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let mut source = core::mem::ManuallyDrop::new(self);
            let slice_ptr = core::ptr::slice_from_raw_parts_mut(source.as_mut_ptr(), source.len());
            CBoxedSlice::from_boxed_slice(Some(unsafe { Box::from_raw(slice_ptr) }))
        }
    }
    //#[cfg(feature = "alloc")]
    //impl<R: ?Sized + Dst> EncodeWithStore<false> for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<Opaque>>,
    //{
    //    type Store = ();
    //
    //    fn encode<'itm>(self, (): &mut ()) -> Self::CType
    //    where
    //        Self: 'itm,
    //    {
    //        let ctypes = self
    //            .into_iter()
    //            .map(|item| CBox::from_box(Some(Box::new(item))))
    //            .collect();
    //        CBoxedSlice::from_boxed_slice(Some(ctypes))
    //    }
    //}
    #[cfg(feature = "alloc")]
    impl<R: ?Sized + SliceDst + CheckedTransmute<Target: SliceDst>> EncodeWithStore<false> for Box<R>
    where
        Box<<R as CheckedTransmute>::Target>: EncodeWithStore,
        Self: ReprFamily<Kind = Box<Transmuted>>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = <Box<R::Target> as EncodeWithStore>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            EncodeWithStore::encode(transmute_into_target_boxed_dst(self), store)
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: External + Encode> EncodeWithStore<false> for Box<R>
    where
        Self: ReprFamily<Kind = Box<Transmuted>>,
        R: DstFamily<Kind = ExternTypeLike>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            Encode::encode(*self)
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: EncodeWithStore, S: Cloned> EncodeWithStore<false> for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
        R: DstFamily<Kind = Sized_>,
    {
        type Store = R::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            CBox::from_box(Some(Box::new(EncodeWithStore::encode(*self, store))))
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized + SliceDst<Elem: EncodeWithStore + Clone>, S: Cloned + ?Sized> EncodeWithStore<false> for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = ClonedCollectionStore<<R::Elem as EncodeWithStore>::Store>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let source = unsafe { core::slice::from_raw_parts(self.as_ptr(), self.len()) };
            let stores = store.0.insert(
                core::iter::repeat_with(Default::default)
                    .take(source.len())
                    .collect(),
            );

            CBoxedSlice::from_boxed_slice(Some(
                source
                    .iter()
                    .cloned()
                    .zip(stores)
                    .map(|(item, store)| EncodeWithStore::encode(item, store))
                    .collect(),
            ))
        }
    }

    #[cfg(feature = "alloc")]
    impl<R, S> EncodeWithStore<false> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>>,
        Box<[R]>: EncodeWithStore,
    {
        type Store = <Box<[R]> as EncodeWithStore>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.into_boxed_slice().encode(store)
        }
    }

    impl<R: EncodeWithStore, S: Cloned, const N: usize> EncodeWithStore<false> for [R; N]
    where
        Self: ReprFamily<Kind = S>,
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

                EncodeWithStore::encode(item, store)
            })
        }
    }

    impl<R: EncodeWithStore> EncodeWithStore<false> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithoutNiche>>,
    {
        type Store = <R as EncodeWithStore>::Store;

        fn encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.map(|v| EncodeWithStore::encode(v, store)).into()
        }
    }
    impl<R: Niche + EncodeWithStore> EncodeWithStore<false> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithCustomNiche>>,
    {
        type Store = <R as EncodeWithStore>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            if let Some(value) = self {
                return EncodeWithStore::encode(value, store);
            }

            R::NICHE_VALUE
        }
    }
}

disjoint_impls! {
    /// Facilitates conversion into a Rust type from a corresponding C-compatible representation.
    pub trait DecodeWithStore<'d, const UNSAFE_OPTIMIZATIONS: bool = false>: ExternC<CType: 'd> {
        /// Auxiliary storage used during conversion. If storage is not used, set the type to `()`.
        ///
        /// Use cases include:
        /// - Storing the result of the conversion of references of [`Cloned`] types
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

    impl<'d, R: ReprC + 'd> DecodeWithStore<'d, false> for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            Some(source)
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: 'd> DecodeWithStore<'d, false> for R
    where
        Self: ReprFamily<Kind = Opaque>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            Some(*unsafe { source.unopaque()? })
        }
    }
    impl<'d, R: CheckedTransmute<Target: DecodeWithStore<'d>>> DecodeWithStore<'d, false> for R
    where
        Self: ReprFamily<Kind = Transmuted>,
    {
        type Store = <R::Target as DecodeWithStore<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { DecodeWithStore::decode(source, store).and_then(|inner| transmute_from_target(inner)) }
        }
    }

    impl<'d, R: ?Sized + SliceDst<Elem: ReprC + 'd>> DecodeWithStore<'d, false> for &'d R
    where
        Self: ReprFamily<Kind = &'d Robust>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let source = unsafe { source.into_rust() }?;
            Some(unsafe { R::from_raw_parts(source.as_ptr(), source.len()) })
        }
    }
    //impl<'a, R: ?Sized + Dst> DecodeWithStore<'a, false> for &'a R
    //where
    //    Self: ReprFamily<Kind = &'a Opaque>,
    //{
    //    type Store = ();

    //    unsafe fn decode<'itm: 'a>(
    //        source: Self::CType,
    //        store: &'itm mut Self::Store,
    //    ) -> Option<Self> {
    //        let source = unsafe { source.into_rust() }?;

    //        if source.iter().any(|item| item.is_null()) {
    //            return None;
    //        }

    //        let store = store.0.insert(
    //            source
    //                .iter()
    //                .map(|&item| unsafe { &*item }.clone())
    //                .collect(),
    //        );

    //        Some(store)
    //    }
    //}
    impl<'a, R: ?Sized + SliceDst + CheckedTransmute<Target: SliceDst + 'a>> DecodeWithStore<'a, false> for &'a R
    where
        &'a <R as CheckedTransmute>::Target: DecodeWithStore<'a>,
        Self: ReprFamily<Kind = &'a Transmuted>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = <&'a R::Target as DecodeWithStore<'a>>::Store;

        unsafe fn decode<'itm: 'a>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            transmute_from_target_ref_dst(unsafe {
                <&R::Target as DecodeWithStore>::decode(source, store)?
            })
        }
    }
    impl<'d, R: DecodeCloned<'d>, S: Cloned + 'd> DecodeWithStore<'d, false> for &'d R
    where
        Self: ReprFamily<Kind = &'d S>,
        R: DstFamily<Kind = Sized_>,
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
    impl<'a, R: ?Sized + SliceDst<Elem: DecodeCloned<'a>>, S: Cloned + ?Sized + 'a> DecodeWithStore<'a, false> for &'a R
    where
        Self: ReprFamily<Kind = &'a S>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = DecodeStoreSlicePair<R::Elem, <R::Elem as DecodeWithStore<'a>>::Store>;

        unsafe fn decode<'itm: 'a>(
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
                    .map(|(&item, store)| unsafe { R::Elem::decode_cloned(item, store) })
                    .collect::<Option<_>>()?,
            );

            Some(unsafe { R::from_raw_parts(values.as_ptr(), values.len()) })
        }
    }

    impl<'d, R: ?Sized + SliceDst<Elem: ReprC + 'd>> DecodeWithStore<'d, false> for &'d mut R
    where
        Self: ReprFamily<Kind = &'d mut Robust>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let source = unsafe { source.into_rust() }?;
            Some(unsafe { R::from_raw_parts_mut(source.as_mut_ptr(), source.len()) })
        }
    }
    //impl<'a, R: ?Sized + Dst> DecodeWithStore<'a, false> for &'a mut R
    //where
    //    Self: ReprFamily<Kind = &'a mut Opaque>,
    //{
    //    type Store = ();

    //    unsafe fn decode<'itm: 'a>(
    //        source: Self::CType,
    //        store: &'itm mut Self::Store,
    //    ) -> Option<Self> {
    //        let source: &mut CSliceMut<_> = store.source.insert(source);
    //        let source: &mut [*mut R] = unsafe { source.into_rust()? };

    //        if source.iter().any(|item| item.is_null()) {
    //            return None;
    //        }

    //        let values = store.values.insert(
    //            source
    //                .iter()
    //                .map(|&item| unsafe { &*item }.clone())
    //                .collect(),
    //        );

    //        Some(values)
    //    }
    //}
    impl<'a, R: ?Sized + SliceDst + CheckedTransmute<Target: SliceDst + 'a>> DecodeWithStore<'a, false> for &'a mut R
    where
        &'a mut <R as CheckedTransmute>::Target: DecodeWithStore<'a>,
        Self: ReprFamily<Kind = &'a mut Transmuted>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = <&'a mut R::Target as DecodeWithStore<'a>>::Store;

        unsafe fn decode<'itm: 'a>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            transmute_from_target_dst_mut(unsafe {
                <&mut R::Target as DecodeWithStore>::decode(source, store)?
            })
        }
    }
    impl<'d, R: DecodeCloned<'d> + EncodeWithStore, S: Cloned + 'd> DecodeWithStore<'d, false> for &'d mut R
    where
        Self: ReprFamily<Kind = &'d mut S>,
        R: DstFamily<Kind = Sized_>,
    {
        type Store = RefMutDecodeStore<R, <R as DecodeWithStore<'d>>::Store>;

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
    impl<'a, R: ?Sized + SliceDst<Elem: DecodeCloned<'a> + EncodeWithStore>, S: Cloned + ?Sized + 'a> DecodeWithStore<'a, false> for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut S>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = MutSliceDecodeStore<R::Elem, <R::Elem as DecodeWithStore<'a>>::Store>;

        unsafe fn decode<'itm: 'a>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = store.source.insert(source);
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
                    .map(|(&item, store)| unsafe { R::Elem::decode_cloned(item, store) })
                    .collect::<Option<_>>()?,
            );

            Some(unsafe { R::from_raw_parts_mut(values.as_mut_ptr(), values.len()) })
        }
    }

    //#[cfg(feature = "alloc")]
    //impl<R: ?Sized + Dst<Elem: ReprC>> DecodeWithStore<'_> for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<Robust>>,
    //{
    //    type Store = ();

    //    unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
    //        unsafe { source.into_rust() }
    //    }
    //}
    //#[cfg(feature = "alloc")]
    //impl<'d, R: ?Sized> DecodeWithStore<'d> for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<Opaque>>,
    //{
    //    type Store = ();

    //    unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
    //        unimplemented!()
    //    }
    //}
    #[cfg(feature = "alloc")]
    impl<'d, R: ?Sized + SliceDst + CheckedTransmute<Target: SliceDst>> DecodeWithStore<'d, false> for Box<R>
    where
        Box<<R as CheckedTransmute>::Target>: DecodeWithStore<'d>,
        Self: ReprFamily<Kind = Box<Transmuted>>,
        R: DstFamily<Kind = SliceLike>,
    {
        type Store = <Box<R::Target> as DecodeWithStore<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe {
                DecodeWithStore::decode(source, store)
                    .and_then(|output| transmute_from_target_boxed_dst(output))
            }
        }
    }
    //#[cfg(feature = "alloc")]
    //impl<'d, R: DecodeWithStore<'d>, S: Cloned> DecodeWithStore<'d> for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<S>>,
    //    R: SizeFamily<Kind = Sized_>,
    //{
    //    type Store = ();

    //    unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
    //        unimplemented!()
    //    }
    //}
    //#[cfg(feature = "alloc")]
    //impl<'d, R: ?Sized + Dst<Elem: DecodeWithStore<'d>>, S: Cloned> DecodeWithStore<'d> for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<S>>,
    //    R: SizeFamily<Kind = UnSized_>,
    //{
    //    type Store = DecodeStoreSlice<R::Store>;

    //    unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
    //        unimplemented!()
    //    }
    //}

    #[cfg(feature = "alloc")]
    impl<'d, R, S> DecodeWithStore<'d, false> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>>,
        Box<[R]>: DecodeWithStore<'d>
    {
        type Store = <Box<[R]> as DecodeWithStore<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { Box::decode(source, store) }.map(Into::into)
        }
    }

    impl<'d, R: DecodeWithStore<'d>, S: Cloned, const N: usize> DecodeWithStore<'d, false> for [R; N]
    where
        Self: ReprFamily<Kind = S>,
    {
        type Store = ArraySyncStore<R::Store, N>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            decode_cloned_array(source, store, |item, store| unsafe {
                <R as DecodeWithStore>::decode(item, store)
            })
        }
    }

    impl<'d, R: DecodeWithStore<'d>> DecodeWithStore<'d, false> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithoutNiche>>,
    {
        type Store = <R as DecodeWithStore<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            decode_cloned_option_without_niche(source, store, |source, store| unsafe {
                <R as DecodeWithStore>::decode(source, store)
            })
        }
    }
    impl<'d, R: Niche + DecodeWithStore<'d>> DecodeWithStore<'d, false> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithCustomNiche>>,
        <R as ExternC>::CType: PartialEq,
    {
        type Store = <R as DecodeWithStore<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            decode_cloned_option_with_custom_niche(
                source,
                store,
                R::NICHE_VALUE,
                |source, store| unsafe { <R as DecodeWithStore>::decode(source, store) },
            )
        }
    }
}

/// Refer to [`EncodeWithStore`]
pub trait Encode: EncodeWithStore {
    fn encode<'itm>(self) -> Self::CType
    where
        Self: 'itm;
}

/// Refer to [`DecodeWithStore`]
pub trait Decode<'d>: DecodeWithStore<'d> {
    /// # Safety
    ///
    /// - All conversions from a pointer must ensure pointer validity beforehand
    unsafe fn decode(source: Self::CType) -> Option<Self>;
}

impl<R: EncodeWithStore<Store = Z>, Z: Zst + Default> Encode for R {
    fn encode<'itm>(self) -> Self::CType
    where
        Self: 'itm,
    {
        let mut store = Default::default();
        <Self as EncodeWithStore>::encode(self, &mut store)
    }
}

impl<'d, R, Z: Zst + Default + 'd> Decode<'d> for R
where
    R: DecodeWithStore<'d, Store = Z>,
{
    unsafe fn decode(source: Self::CType) -> Option<Self> {
        let mut store = Default::default();
        // SAFETY: `Decode` is only blanket-implemented for zero-sized stores, so extending the
        // borrow of the local store does not extend the lifetime of any backing data.
        let store = unsafe { core::mem::transmute::<&mut Z, &'d mut Z>(&mut store) };
        unsafe { <Self as DecodeWithStore>::decode(source, store) }
    }
}

// TODO: Could the store just be synced on drop?
// FIXME: EncodeWithStore types can never error during sync
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
pub struct OwningStore<T>(#[expect(unused)] Option<Box<[T]>>);

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
    fn sync(self) -> Option<()> {
        for store in self.stores.unwrap() {
            store.sync()?;
        }

        Some(())
    }
}

pub struct OpaqueMutSliceEncodeStore<'a, R> {
    ctypes: Option<Box<[*mut R]>>,
    original: Option<&'a mut [R]>,
}

impl<'a, R> Default for OpaqueMutSliceEncodeStore<'a, R> {
    fn default() -> Self {
        Self {
            ctypes: None,
            original: None,
        }
    }
}

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

pub struct RefStore<R: EncodeWithStore> {
    ctype: Option<R::CType>,
    encode_store: R::Store,
}

impl<R: EncodeWithStore> Default for RefStore<R> {
    fn default() -> Self {
        Self {
            ctype: None,
            encode_store: Default::default(),
        }
    }
}

impl<R: EncodeWithStore> Store for RefStore<R> {
    fn sync(self) -> Option<()> {
        self.encode_store.sync()
    }
}

pub struct RefMutStore<'a, R: EncodeWithStore> {
    ctype: Option<R::CType>,
    encode_store: R::Store,
    original: Option<&'a mut R>,
}

impl<'a, R: EncodeWithStore> Default for RefMutStore<'a, R> {
    fn default() -> Self {
        Self {
            ctype: None,
            encode_store: Default::default(),
            original: None,
        }
    }
}

impl<'a, 'b, R: EncodeWithStore + DecodeWithStore<'b> + 'b> Store for RefMutStore<'a, R> {
    fn sync(self) -> Option<()> {
        #[cfg(all(not(test), not(feature = "unsafe-optimizations")))]
        const {
            assert!(co3::impls!(R: Decode<'static>), "Not yet implemented");
        }

        if let (Some(ctype), Some(original)) = (self.ctype, self.original) {
            let mut decode_store = Default::default();

            let store_ref = unsafe {
                core::mem::transmute::<
                    &mut <R as DecodeWithStore<'b>>::Store,
                    &'b mut <R as DecodeWithStore<'b>>::Store,
                >(&mut decode_store)
            };

            *original = unsafe { R::decode(ctype, store_ref)? };
        }

        Some(())
    }
}

#[cfg(feature = "alloc")]
pub struct ClonedCollectionStore<D>(Option<Box<[D]>>);

#[cfg(feature = "alloc")]
impl<D> Default for ClonedCollectionStore<D> {
    fn default() -> Self {
        Self(None)
    }
}

#[cfg(feature = "alloc")]
impl<D: Store> Store for ClonedCollectionStore<D> {
    fn sync(self) -> Option<()> {
        for store in self.0.unwrap() {
            store.sync()?;
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

#[cfg(feature = "alloc")]
pub struct MutSliceStore<'a, R: EncodeWithStore> {
    ctypes: Option<Box<[R::CType]>>,
    stores: Option<Box<[R::Store]>>,
    original: Option<&'a mut [R]>,
}

#[cfg(feature = "alloc")]
impl<'a, R: EncodeWithStore> Default for MutSliceStore<'a, R> {
    fn default() -> Self {
        Self {
            ctypes: None,
            stores: None,
            original: None,
        }
    }
}

#[cfg(feature = "alloc")]
impl<'a, 'b, R: EncodeWithStore + DecodeWithStore<'b> + 'b> Store for MutSliceStore<'a, R> {
    fn sync(self) -> Option<()> {
        const {
            #[cfg(all(not(test), not(feature = "unsafe-optimizations")))]
            assert!(co3::impls!(R: Decode<'static>), "Not yet implemented");
        }

        if let (Some(borrows), Some(ctypes)) = (self.original, self.ctypes) {
            let mut decode_store = Default::default();

            for (original, ctypes) in borrows.iter_mut().zip(ctypes) {
                let store_ref = unsafe {
                    core::mem::transmute::<
                        &mut <R as DecodeWithStore<'b>>::Store,
                        &'b mut <R as DecodeWithStore<'b>>::Store,
                    >(&mut decode_store)
                };

                *original = unsafe { R::decode(ctypes, store_ref)? };
            }
        }

        Some(())
    }
}

pub struct ArraySyncStore<D, const N: usize>([D; N]);
impl<D: Default, const N: usize> Default for ArraySyncStore<D, N> {
    fn default() -> Self {
        // TODO: https://github.com/rust-lang/rust/issues/61415
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

pub struct RefMutDecodeStore<R: ExternC, DS> {
    value: Option<R>,
    decode_store: DS,
    source: Option<*mut R::CType>,
}

impl<R: ExternC, DS: Default> Default for RefMutDecodeStore<R, DS> {
    fn default() -> Self {
        Self {
            value: None,
            decode_store: Default::default(),
            source: None,
        }
    }
}

impl<R: EncodeWithStore, DS: Store> Store for RefMutDecodeStore<R, DS> {
    fn sync(self) -> Option<()> {
        let mut encode_store = Default::default();
        let ctypes = EncodeWithStore::encode(self.value.unwrap(), &mut encode_store);
        unsafe { *self.source.unwrap() = ctypes };
        Some(())
    }
}

pub struct MutSliceDecodeStore<R: ExternC, DS> {
    values: Option<Box<[R]>>,
    stores: Option<Box<[DS]>>,
    source: Option<CSliceMut<R::CType>>,
}

impl<R: ExternC, DS: Default> Default for MutSliceDecodeStore<R, DS> {
    fn default() -> Self {
        Self {
            values: None,
            stores: None,
            source: None,
        }
    }
}

impl<R: EncodeWithStore, DS: Store> Store for MutSliceDecodeStore<R, DS> {
    fn sync(self) -> Option<()> {
        let mut encode_store = Default::default();

        let source = unsafe { self.source.unwrap().into_rust().unwrap() };
        for (src, decoded) in source.iter_mut().zip(self.values.unwrap()) {
            *src = EncodeWithStore::encode(decoded, &mut encode_store);
        }

        Some(())
    }
}

#[cfg(feature = "alloc")]
pub struct OpaqueMutSliceDecodeStore<R> {
    values: Option<Box<[R]>>,
    source: Option<CSliceMut<*mut R>>,
}

#[cfg(feature = "alloc")]
impl<R> Default for OpaqueMutSliceDecodeStore<R> {
    fn default() -> Self {
        Self {
            values: None,
            source: None,
        }
    }
}

#[cfg(feature = "alloc")]
impl<R> Store for OpaqueMutSliceDecodeStore<R> {
    fn sync(self) -> Option<()> {
        let slice = unsafe { self.source.unwrap().into_rust().unwrap() };

        for (&mut src, decoded) in slice.iter_mut().zip(self.values.unwrap()) {
            unsafe { *src = decoded };
        }

        Some(())
    }
}

/// Macro for defining FFI types of a known category ([`Robust`], [`Transmuted`] or [`Cloned`]).
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
///     borrow::{Borrow, ToOwned},
///     ir::{DstFamily, Sized_},
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
///     impl(T: ?Sized) Cloned for NoRepr<T> {}
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
/// impl DstFamily for RobustStruct {
///     type Kind = Sized_;
/// }
/// impl<T: ?Sized + DstFamily> DstFamily for NoRepr<T> {
///     type Kind = T::Kind;
/// }
///
/// ```
#[doc(hidden)]
#[macro_export]
macro_rules! reprC {
    (unsafe impl $(())? Robust for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        $crate::reprC! { @robust_common [for<'_dummy> Self: Copy,] [for<'_dummy> Self: Sized,] [] $self_ty $([$($preds)*])? }
    };

    (unsafe impl ( $($params:tt)+ ) Robust for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        $crate::reprC! { @robust_common [Self: Copy,] [Self: Sized,] [$($params)+] $self_ty $([$($preds)*])? }
    };

    (unsafe impl $(( $($params:tt)+ ))? SizedRobust for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        $crate::reprC! { @assert_sized [$($($params)+)?] $self_ty $([$($preds)*])? }
        $crate::reprC! { @robust_common [] [] [$($($params)+)?] $self_ty $([$($preds)*])? }
    };

    (@robust_common [$($copy_bound:tt)*] [$($sized_bound:tt)*] [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])?) => {
        $crate::reprC! { @assert_non_zst [$($sized_bound)*] [$($impl_generics)*] $self_ty $([$($preds)*])? }

        impl<$($impl_generics)*> $crate::ir::ReprFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::ir::Robust;
        }

        impl<$($impl_generics)*> $crate::niche::NicheFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::niche::WithoutNiche;
        }

        unsafe impl<$($impl_generics)*> $crate::ReprC for $self_ty where
            $($copy_bound)*
            $($($preds)*)?
        {}

        unsafe impl<$($impl_generics)*> $crate::FnArg for $self_ty where
            $($copy_bound)*
            $($($preds)*)?
        {}

        $crate::reprC! { @no_drop_borrow_ir [$($sized_bound)*] [$($impl_generics)*] $self_ty $([$($preds)*])? }
    };

    (impl $(( $($params:tt)* ))? Cloned for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        $crate::reprC! { @cloned_common [$($($params)*)?] $self_ty $([$($preds)*])? {} }
    };

    (impl $(( $($params:tt)* ))? SizedCloned for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        $crate::reprC! { @assert_sized [$($($params)*)?] $self_ty $([$($preds)*])? }
        $crate::reprC! { @cloned_common [$($($params)*)?] $self_ty $([$($preds)*])? {} }
    };

    (@cloned_common [$($params:tt)*] $self_ty:ty $([$($preds:tt)*])? {}) => {
        impl<$($params)*> $crate::ir::Cloned for $self_ty $(where $($preds)*)? {}

        impl<$($params)*> $crate::ir::ReprFamily for $self_ty $(where $($preds)*)? {
            type Kind = Self;
        }
    };

    (unsafe impl $(())? Transmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;
    }) => {
        impl $crate::dst::DstFamily for $self_ty where $($($preds)*)? {
            type Kind = <$target as $crate::dst::DstFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_delegate_niche_valid [for<'_dummy>] [for<'_dummy> Self: Sized,] [] $self_ty $([$($preds)*])? {
                type Target = $target;
            }
        }

        $crate::reprC! { @transmuted_delegate_borrow [for<'_dummy>] [] $self_ty $([$($preds)*])? }
    };
    (unsafe impl ( $($params:tt)+ ) Transmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;
    }) => {
        impl<$($params)*> $crate::dst::DstFamily for $self_ty where
            $target: $crate::dst::DstFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::dst::DstFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_delegate_niche_valid [] [Self: Sized,] [$($params)+] $self_ty $([$($preds)*])? {
                type Target = $target;
            }
        }

        $crate::reprC! { @transmuted_delegate_borrow [] [$($params)+] $self_ty $([$($preds)*])? }
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
        impl $crate::dst::DstFamily for $self_ty where $($($preds)*)? {
            type Kind = <$target as $crate::dst::DstFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_delegate_niche_sized [] [for<'_dummy> Self: Sized,] [] $self_ty $([$($preds)*])? {
                type Target = $target;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

        $crate::reprC! { @transmuted_delegate_borrow [for<'_dummy>] [] $self_ty $([$($preds)*])? }
    };
    (unsafe impl ( $($params:tt)+ ) Transmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        impl<$($params)+> $crate::dst::DstFamily for $self_ty where
            $target: $crate::dst::DstFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::dst::DstFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_delegate_niche_sized [] [Self: Sized,] [$($params)+] $self_ty $([$($preds)*])? {
                type Target = $target;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

        $crate::reprC! { @transmuted_delegate_borrow [] [$($params)+] $self_ty $([$($preds)*])? }
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
        impl $crate::dst::DstFamily for $self_ty where $($($preds)*)? {
            type Kind = <$target as $crate::dst::DstFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_explicit_niche [for<'_dummy>] [] [] $self_ty $([$($preds)*])? {
                type Target = $target;
                const NICHE_VALUE: $niche_ty = $niche_value;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

        $crate::reprC! { @transmuted_delegate_borrow [for<'_dummy>] [] $self_ty $([$($preds)*])? }
    };
    (unsafe impl ( $($params:tt)+ ) Transmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;
        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        impl<$($params)+> $crate::dst::DstFamily for $self_ty where
            $target: $crate::dst::DstFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::dst::DstFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_explicit_niche [] [] [$($params)+] $self_ty $([$($preds)*])? {
                type Target = $target;
                const NICHE_VALUE: $niche_ty = $niche_value;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

        $crate::reprC! { @transmuted_delegate_borrow [] [$($params)+] $self_ty $([$($preds)*])? }
    };

    (unsafe impl $(())? NoDropSizedTransmuted for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;
        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        $crate::reprC! { @no_drop_sized_transmuted [] $self_ty $([$($preds)*])? {} }

        $crate::reprC! {
            @transmuted_explicit_niche [for<'_dummy>] [Self: Sized,] [] $self_ty $([$($preds)*])? {
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
            @transmuted_explicit_niche [] [Self: Sized,] [$($params)+] $self_ty $([$($preds)*])? {
                type Target = $target;
                const NICHE_VALUE: $niche_ty = $niche_value;
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }
    };

    (@no_drop_sized_transmuted [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])? {}) => {
        $crate::reprC! { @assert_sized [$($impl_generics)*] $self_ty $([$($preds)*])? }
        $crate::reprC! { @assert_no_drop [$($impl_generics)*] $self_ty $([$($preds)*])? }

        $crate::reprC! { @no_drop_borrow_ir [] [$($impl_generics)*] $self_ty $([$($preds)*])? }
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

        impl<$($impl_generics)*> $crate::niche::NicheFamily for $self_ty where
            $target: $crate::niche::NicheFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::niche::NicheFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_common [$($for_dummy)*] [$($sized_bound)*] [$($impl_generics)*] $self_ty $([$($preds)*])? {
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

    (@transmuted_explicit_niche [$($for_dummy:tt)*] [$($sized_bound:tt)*] [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])? {
        type Target = $target:ty;
        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;
        fn is_valid($target_var:ident: $target_ty:ty) -> bool $block:block
    }) => {
        impl<$($impl_generics)*> $crate::ir::ReprFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::ir::Transmuted;
        }

        impl<$($impl_generics)*> $crate::niche::NicheFamily for $self_ty where
            $($sized_bound)*
            $($($preds)*)?
        {
            type Kind = $crate::niche::WithCustomNiche;
        }

        $crate::reprC! {
            @transmuted_common [$($for_dummy)*] [$($sized_bound)*] [$($impl_generics)*] $self_ty $([$($preds)*])? {
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
                    // TODO: This introduces a dependency, can we do without?
                    // and it also adds checks for internal types like `NonZeroU8`
                    ($target: $crate::niche::NicheFamily<Kind = $crate::niche::WithoutNiche>),
                    "Transmuted CAN'T define a custom niche if target has a niche"
                );

                $niche_value
            };
        }
    };

    (@transmuted_common [$($for_dummy:tt)*] [$($sized_bound:tt)*] [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])? {
        type Target = $target:ty;
        fn is_valid($target_var:ident: $target_ty:ty) -> bool $block:block
    }) => {
        unsafe impl<$($impl_generics)*> $crate::transmute::CheckedTransmute for $self_ty $(where $($preds)*)? {
            type Target = $target;

            #[inline(always)]
            fn is_valid($target_var: $target_ty) -> bool $block
        }

        unsafe impl<$($impl_generics)*> $crate::transmute::EncodeTransmuted for $self_ty
        where
            $($for_dummy)* $target: $crate::EncodeWithStore,
            $($sized_bound)*
            $($($preds)*)?
        {
            type Store = <$target as $crate::EncodeWithStore>::Store;
        }
    };

    (@transmuted_delegate_borrow [$($for_dummy:tt)*] [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])?) => {
        $crate::reprC! { @assert_no_drop [$($impl_generics)*] $self_ty $([$($preds)*])? }

        impl<$($impl_generics)*> $crate::heapify::Heapify for $self_ty
        where
            Self: $crate::transmute::CheckedTransmute<Target: $crate::heapify::Heapify> + Sized,
            $($($preds)*)?
        {
            type Kind = <<Self as $crate::transmute::CheckedTransmute>::Target as $crate::heapify::Heapify>::Kind;

            #[inline(always)]
            fn heapify(self) -> Self::Kind {
                $crate::transmute::transmute_into_target(self).heapify()
            }

            #[inline(always)]
            fn unheapify(kind: Self::Kind) -> Self {
                let target = <<Self as $crate::transmute::CheckedTransmute>::Target as $crate::heapify::Heapify>::unheapify(kind);
                $crate::transmute::transmute_from_target(target).unwrap()
            }
        }
        $crate::reprC! { @impl_with_const_generics [$($impl_generics)*] $crate::borrow::Borrow<IN_STRUCT> for $self_ty
        where
            Self: $crate::transmute::CheckedTransmute<Target: $crate::borrow::Borrow<IN_STRUCT>> + Sized,
            $($($preds)*)?
        {
            type Borrowed<'itm>
                = <<Self as $crate::transmute::CheckedTransmute>::Target as $crate::borrow::Borrow<IN_STRUCT>>::Borrowed<'itm>
            where
                Self: 'itm;

            type Store = <<Self as $crate::transmute::CheckedTransmute>::Target as $crate::borrow::Borrow<IN_STRUCT>>::Store;

            #[inline(always)]
            fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                $crate::transmute::transmute_into_target(self).borrow(store)
            }
        }}

        $crate::reprC! { @impl_with_lifetime_and_const_generics [$($impl_generics)*] $crate::borrow::ToOwned<'_išč, IN_STRUCT> for $self_ty
        where
            // FIXME: This bound is redundant for concrete types
            // like `u32`, but required for COption<T>
            $($for_dummy)* Self: $crate::transmute::CheckedTransmute<Target: $crate::borrow::ToOwned<'_išč, IN_STRUCT>> + Sized + '_išč,
            $($($preds)*)?
        {
            #[inline(always)]
            fn to_owned(borrowed: Self::Borrowed<'_išč>) -> Self {
                let target = <<Self as $crate::transmute::CheckedTransmute>::Target as $crate::borrow::ToOwned<'_, _>>::to_owned(borrowed);
                // TODO: unwrap is redundant here
                $crate::transmute::transmute_from_target(target).unwrap()
            }
        }}
    };

    (@no_drop_borrow_ir [$($sized_bound:tt)*] [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])?) => {
        impl<$($impl_generics)*> $crate::heapify::Heapify for $self_ty where
            $($sized_bound)*
            $($($preds)*)? {
            type Kind = Self;

            #[inline(always)]
            fn heapify(self) -> Self::Kind {
                self
            }

            #[inline(always)]
            fn unheapify(kind: Self::Kind) -> Self {
                kind
            }
        }

        $crate::reprC! { @impl_with_const_generics [$($impl_generics)*] $crate::borrow::Borrow<IN_STRUCT> for $self_ty where
            $($sized_bound)*
            $($($preds)*)?
        {
            type Borrowed<'_išč> = Self
            where
                Self: '_išč;

            type Store = ();

            fn borrow<'_itm>(self, (): &mut ()) -> Self::Borrowed<'_itm>
            where
                Self: '_itm,
            {
                self
            }
        }}

        $crate::reprC! { @impl_with_lifetime_and_const_generics [$($impl_generics)*] $crate::borrow::ToOwned<'_išč, IN_STRUCT> for $self_ty where
            $($sized_bound)*
            // FIXME: This bound is redundant for concrete types
            // like `u32`, but required for COption<T>
            Self: '_išč,
            $($($preds)*)?
        {
            fn to_owned(borrowed: Self::Borrowed<'_išč>) -> Self {
                borrowed
            }
        }}
    };

    (@assert_no_drop [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])?) => {
        const _: () = {
            #[allow(dead_code)]
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

    (@assert_non_zst [$($sized_bound:tt)*] [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])?) => {
        const _: () = {
            #[allow(dead_code)]
            trait AssertNonZst {
                fn assert_non_zst();
            }

            impl<$($impl_generics)*> AssertNonZst for $self_ty where $($sized_bound)* $($($preds)*)? {
                fn assert_non_zst() {
                    const {
                        assert!(
                            core::mem::size_of::<Self>() != 0,
                            "`impl Robust`/`impl SizedRobust` doesn't support ZST types"
                        );
                    }
                }
            }
        };
    };

    (@assert_sized [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])?) => {
        impl<$($impl_generics)*> $crate::dst::DstFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::dst::Sized_;
        }

        const _: () = {
            #[allow(dead_code)]
            trait AssertSized {
                fn assert_sized();
            }

            impl<$($impl_generics)*> AssertSized for $self_ty $(where $($preds)*)? {
                fn assert_sized() {
                    const {
                        assert!(
                            $crate::impls!(Self: Sized),
                            "`impl XXX` doesn't support `?Sized` types. Use `impl SizedXXX` instead"
                        );
                    }
                }
            }
        };
    };

    (@impl_with_const_generics [] $($rest:tt)*) => {
        impl<const IN_STRUCT: bool> $($rest)*
    };
    (@impl_with_const_generics [$($impl_generics:tt)+] $($rest:tt)*) => {
        $crate::reprC! { @impl_with_const_generics_acc [] [$($impl_generics)+] $($rest)* }
    };
    (@impl_with_lifetime_and_const_generics [] $($rest:tt)*) => {
        impl<'_išč, const IN_STRUCT: bool> $($rest)*
    };
    (@impl_with_lifetime_and_const_generics [$($impl_generics:tt)+] $($rest:tt)*) => {
        $crate::reprC! { @impl_with_lifetime_and_const_generics_acc [] [$($impl_generics)+] $($rest)* }
    };
    (@impl_with_const_generics_acc [$($out:tt)*] [] $($rest:tt)*) => {
        impl<$($out)*, const IN_STRUCT: bool> $($rest)*
    };
    (@impl_with_const_generics_acc [$($out:tt)*] [,] $($rest:tt)*) => {
        impl<$($out)*, const IN_STRUCT: bool> $($rest)*
    };
    (@impl_with_const_generics_acc [$($out:tt)*] [$head:tt $($tail:tt)*] $($rest:tt)*) => {
        $crate::reprC! { @impl_with_const_generics_acc [$($out)* $head] [$($tail)*] $($rest)* }
    };
    (@impl_with_lifetime_and_const_generics_acc [$($out:tt)*] [] $($rest:tt)*) => {
        impl<'_išč, $($out)*, const IN_STRUCT: bool> $($rest)*
    };
    (@impl_with_lifetime_and_const_generics_acc [$($out:tt)*] [,] $($rest:tt)*) => {
        impl<'_išč, $($out)*, const IN_STRUCT: bool> $($rest)*
    };
    (@impl_with_lifetime_and_const_generics_acc [$($out:tt)*] [$head:tt $($tail:tt)*] $($rest:tt)*) => {
        $crate::reprC! { @impl_with_lifetime_and_const_generics_acc [$($out)* $head] [$($tail)*] $($rest)* }
    };
}

reprC! {
    unsafe impl(R,) SizedRobust for *const R {}
}
reprC! {
    unsafe impl(R) SizedRobust for *mut R {}
}

// SAFETY: Array is just a contiguous block of memory
unsafe impl<R: ReprC, const N: usize> ReprC for [R; N] {}

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
            DecodeWithStore<'static>,
            EncodeWithStore,
            ReprC,
        );
        assert_impl_all!(&u8:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const u8>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&mut u8:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut u8>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        // FIXME:
        //assert_impl_all!(Box<u8>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut u8>,
        //    DecodeWithStore<'static>,
        //    EncodeWithStore,
        //);
        assert_impl_all!(&[u8]:
            ReprFamily<Kind = &'static Robust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&mut [u8]:
            ReprFamily<Kind = &'static mut Robust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[u8]>:
            ReprFamily<Kind = Box<Robust>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<u8>:
            ReprFamily<Kind = Vec<Robust>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!([u8; 2]:
            ReprFamily<Kind = Robust>,
            NicheFamily<Kind = WithoutNiche>,
            DecodeWithStore<'static>,
            EncodeWithStore,
            ReprC,
        );
        assert_impl_all!(Option<u8>:
            ReprFamily<Kind = Option<WithoutNiche>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = COption<u8>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
    }

    #[test]
    fn const_ptr_is_robust() {
        assert_impl_all!(*const bool:
            ReprFamily<Kind = Robust>,
            NicheFamily<Kind = WithoutNiche>,
            DecodeWithStore<'static>,
            EncodeWithStore,
            ReprC,
        );
        assert_impl_all!(&*const bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const *const bool>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&mut *const bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut *const bool>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<*const bool>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut *const bool>,
        //    DecodeWithStore<'static>,
        //    EncodeWithStore,
        //);
        assert_impl_all!(&[*const bool]:
            ReprFamily<Kind = &'static Robust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*const bool>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&mut [*const bool]:
            ReprFamily<Kind = &'static mut Robust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*const bool>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[*const bool]>:
            ReprFamily<Kind = Box<Robust>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const bool>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,

        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<*const bool>:
            ReprFamily<Kind = Vec<Robust>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const bool>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!([*const bool; 2]:
            ReprFamily<Kind = Robust>,
            NicheFamily<Kind = WithoutNiche>,
            DecodeWithStore<'static>,
            EncodeWithStore,
            ReprC,
        );
        assert_impl_all!(Option<*const bool>:
            ReprFamily<Kind = Option<WithoutNiche>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = COption<*const bool>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
    }

    #[test]
    fn encode_cloned_mut_ref() {
        let inner = 8u8;
        let other = 42u8;
        let mut value = Some(inner);
        let value_mut_ref: &mut Option<u8> = &mut value;
        {
            let mut store = Box::default();
            let encoded = EncodeWithStore::encode(value_mut_ref, &mut *store);
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
            let encoded = EncodeWithStore::encode(ref_mut, &mut *store);
            let c_slice = unsafe { encoded.into_rust().unwrap() };
            c_slice[0] = COption::Some(other);
            store.sync().unwrap();
        }
        assert_eq!(slice, [Some(42u8)]);
    }

    #[test]
    fn decode_cloned_mut_ref() {
        let mut c_opt = COption::Some(1u8);
        let c_ptr: *mut _ = &mut c_opt;
        let new_val: u8 = 42;
        {
            let mut store = Box::default();
            let decoded =
                unsafe { <&mut Option<u8> as DecodeWithStore>::decode(c_ptr, &mut *store) }
                    .unwrap();
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
                unsafe { <&mut [Option<u8>] as DecodeWithStore>::decode(c_slice, &mut *store) }
                    .unwrap();
            decoded[0] = Some(x);
            store.sync().unwrap();
        }
        assert_eq!(c_opts[0], COption::Some(10u8));
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn encode_opaque_ref_mut_slice() {
        #[derive(Clone, PartialEq, Eq)]
        struct OpaqueData {
            value: i32,
        }

        impl DstFamily for OpaqueData {
            type Kind = Sized_;
        }
        impl ReprFamily for OpaqueData {
            type Kind = Opaque;
        }

        let mut items = [OpaqueData { value: 1 }];
        let slice_ref: &mut [_] = &mut items;
        let other = Box::new(OpaqueData { value: 100 });
        {
            let mut store = Box::default();
            let encoded = EncodeWithStore::encode(slice_ref, &mut store);
            let c_slice = unsafe { encoded.into_rust().unwrap() };
            c_slice[0] = CBox::from_box(Some(other));

            store.sync().unwrap();
        }
        assert_eq!(items[0].value, 100);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn decode_opaque_ref_mut_slice() {
        #[derive(Clone, PartialEq, Eq)]
        struct OpaqueData {
            value: i32,
        }

        impl DstFamily for OpaqueData {
            type Kind = Sized_;
        }
        impl ReprFamily for OpaqueData {
            type Kind = Opaque;
        }

        let item = Box::new(OpaqueData { value: 10 });
        let mut ptrs = [CBox::from_box(Some(item))];
        let c_slice = CSliceMut::from_slice(Some(&mut ptrs));

        {
            let mut store = Box::default();
            let decoded =
                unsafe { <&mut [OpaqueData] as DecodeWithStore>::decode(c_slice, &mut store) }
                    .unwrap();
            decoded[0].value = 100;
            store.sync().unwrap();
        }

        let updated = unsafe { &*ptrs[0].data };
        assert_eq!(updated.value, 100);
    }
}
