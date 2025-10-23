//! Structures and macros related to FFI and generation of FFI bindings. Any type that implements
//! [`ExternC`] can be used in the FFI bindings generated with [`carbonate`]/[`decarbonate`]. It
//! is advisable to implement [`Ir`] and benefit from automatic implementation of [`ExternC`]
#![no_std]

extern crate alloc;

use core::mem::ManuallyDrop;

use alloc::{boxed::Box, vec::Vec};

#[cfg(feature = "derive")]
pub use co3_derive::*;
use derive_more::Display;
use disjoint_impls::disjoint_impls;

#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
use crate::transmute::{
    transmute_from_target_boxed_slice, transmute_from_target_vec,
    transmute_into_target_boxed_slice, transmute_into_target_vec,
};
use crate::{
    external::{ExternRef, ExternRefMut, External},
    ir::{Cloned, Extern, Ir, Opaque, Robust, Transparent},
    local::{LocalRef, LocalSlice},
    niche::{Niche, Optional},
    repr_c::default_init_arr,
    slice::{OutBoxedSlice, RefMutSlice, RefSlice},
    transmute::{
        Transmute, transmute_from_target, transmute_from_target_ref_slice,
        transmute_from_target_slice_mut, transmute_into_target, transmute_into_target_ref_slice,
        transmute_into_target_slice_mut,
    },
};

pub mod external;
pub mod handle;
pub mod ir;
pub mod local;
pub mod niche;
pub mod out_ptr;
pub mod primitives;
mod repr_c;
pub mod slice;
mod std_impls;
pub mod transmute;

/// A specialized `Result` type for FFI operations
pub type Result<T> = core::result::Result<T, FfiReturn>;

/// Robust type that conforms to C ABI and can be safely shared across FFI boundaries. This does
/// not guarantee the ABI compatibility of the referent for pointers. These pointers are opaque
///
/// # Safety
///
/// Type implementing the trait must be a robust type with a guaranteed C ABI. Care must be taken
/// not to dereference pointers whose referents don't implement `ReprC`; they are considered opaque
// NOTE: Type is `Copy` to indicate that there can be no ownership transfer
pub unsafe trait ReprC: Copy {}

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

    impl<R: Transmute> ExternC for R
    where
        Self: Ir<Type = Transparent>,
        <R>::Target: ExternC,
    {
        type CType = <R::Target as ExternC>::CType;
    }
    impl<R: ReprC> ExternC for R
    where
        Self: Ir<Type = Robust>,
    {
        type CType = Self;
    }
    impl<R> ExternC for R
    where
        Self: Ir<Type = Opaque>,
    {
        type CType = *mut Self;
    }
    impl<R: Ir<Type = Extern>> ExternC for R {
        type CType = *mut external::Extern;
    }

    impl<'a, R> ExternC for &'a R
    where
        Self: Ir<Type = &'a Extern>,
    {
        type CType = *const external::Extern;
    }
    impl<'a, R: ExternC, S: Cloned> ExternC for &'a R
    where
        Self: Ir<Type = &'a S>,
    {
        type CType = *const R::CType;
    }

    impl<'a, R> ExternC for &'a mut R
    where
        Self: Ir<Type = &'a mut Extern>,
    {
        type CType = *mut external::Extern;
    }

    impl<'slice, R: Transmute> ExternC for &'slice [R]
    where
        Self: Ir<Type = &'slice [Transparent]>,
        &'slice [<R>::Target]: ExternC,
    {
        type CType = <&'slice [R::Target] as ExternC>::CType;
    }
    impl<'a, R: ReprC> ExternC for &'a [R]
    where
        Self: Ir<Type = &'a [Robust]>,
    {
        type CType = RefSlice<R>;
    }
    impl<'a, R> ExternC for &'a [R]
    where
        Self: Ir<Type = &'a [Opaque]>,
    {
        type CType = RefSlice<*const R>;
    }
    impl<'a, R: ExternC, S: Cloned> ExternC for &'a [R]
    where
        Self: Ir<Type = &'a [S]>,
    {
        type CType = RefSlice<R::CType>;
    }

    impl<'slice, R: Transmute> ExternC for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Transparent]>,
        &'slice mut [<R>::Target]: ExternC,
    {
        type CType = <&'slice mut [R::Target] as ExternC>::CType;
    }
    impl<'a, R: ReprC> ExternC for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Robust]>,
    {
        type CType = RefMutSlice<R>;
    }

    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> ExternC for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        // NOTE:One might expect the serialized form to be `*const R` but there is no need
        // to postpone reading the pointer (only applies if there is no ownership transfer)
        type CType = R;
    }
    impl<R> ExternC for Box<R>
    where
        Self: Ir<Type = Box<Extern>>,
    {
        type CType = *mut external::Extern;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ExternC, S: Cloned> ExternC for Box<R>
    where
        Self: Ir<Type = Box<S>>,
    {
        // NOTE:One might expect the serialized form to be `*const R::CType` but there is no need
        // to postpone reading the pointer (only applies when there is no ownership transfer)
        type CType = R::CType;
    }

    impl<R: Transmute> ExternC for Box<[R]>
    where
        Self: Ir<Type = Box<[Transparent]>>,
        Box<[<R>::Target]>: ExternC,
    {
        type CType = <Box<[R::Target]> as ExternC>::CType;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> ExternC for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        type CType = RefSlice<R>;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R> ExternC for Box<[R]>
    where
        Self: Ir<Type = Box<[Opaque]>>,
    {
        type CType = RefSlice<*mut R>;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ExternC, S: Cloned> ExternC for Box<[R]>
    where
        Self: Ir<Type = Box<[S]>>,
    {
        type CType = RefSlice<R::CType>;
    }

    #[cfg(feature = "owned_types")]
    impl<R: Transmute> ExternC for Vec<R>
    where
        Self: Ir<Type = Vec<Transparent>>,
        Vec<<R>::Target>: ExternC,
    {
        type CType = <Vec<R::Target> as ExternC>::CType;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> ExternC for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        type CType = RefSlice<R>;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R> ExternC for Vec<R>
    where
        Self: Ir<Type = Vec<Opaque>>,
    {
        type CType = RefSlice<*mut R>;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ExternC, S: Cloned> ExternC for Vec<R>
    where
        Self: Ir<Type = Vec<S>>,
    {
        type CType = RefSlice<R::CType>;
    }

    impl<R, const N: usize> ExternC for [R; N]
    where
        Self: Ir<Type = [Opaque; N]>,
    {
        type CType = [*mut R; N];
    }
    impl<R, const N: usize> ExternC for [R; N]
    where
        Self: Ir<Type = [Extern; N]>,
    {
        type CType = [*mut external::Extern; N];
    }
    impl<R: ExternC, S: Cloned, const N: usize> ExternC for [R; N]
    where
        Self: Ir<Type = [S; N]>,
    {
        type CType = [R::CType; N];
    }

    impl<R: Optional> ExternC for R
    where
        Self: Ir<Type = Option<Transparent>>,
    {
        type CType = R::Inner;
    }
    impl<R: ExternC> ExternC for Option<R>
    where
        Self: Ir<Type = Option<Robust>>,
    {
        type CType = FfiTuple2<<u8 as ExternC>::CType, R::CType>;
    }
    impl<R: ExternC> ExternC for Option<R>
    where
        Self: Ir<Type = Option<Opaque>>,
    {
        type CType = *mut R;
    }
    impl<R: Niche, S: Cloned> ExternC for Option<R>
    where
        Self: Ir<Type = Option<S>>,
    {
        type CType = <R as ExternC>::CType;
    }

    // TODO: These shouldn't be required?
    impl<'a, R: 'a, S: Cloned> ExternC for LocalRef<'a, R>
    where
        Self: Ir<Type = &'a S>,
        &'a R: ExternC,
    {
        type CType = <&'a R as ExternC>::CType;
    }
    impl<'a, R: 'a> ExternC for LocalSlice<'a, R>
    where
        Self: Ir<Type = &'a [Opaque]>,
        &'a [R]: ExternC,
    {
        type CType = OutBoxedSlice<*const R>;
    }
    impl<'a, R: 'a, S: Cloned> ExternC for LocalSlice<'a, R>
    where
        Self: Ir<Type = &'a [S]>,
        &'a [R]: ExternC,
    {
        type CType = <&'a [R] as ExternC>::CType;
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
        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm;
    }

    impl<R: Transmute> Encode for R
    where
        <Self as Transmute>::Target: Encode,
        Self: Ir<Type = Transparent>,
    {
        type Store = <R::Target as Encode>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            transmute_into_target(self).encode(store)
        }
    }
    impl<R: ReprC> Encode for R
    where
        Self: Ir<Type = Robust>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
            self
        }
    }
    impl<R> Encode for R
    where
        Self: Ir<Type = Opaque>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
            Box::into_raw(Box::new(self))
        }
    }
    impl<R: Ir<Type = Extern> + External> Encode for R {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
            core::mem::ManuallyDrop::new(self).as_extern_ptr_mut()
        }
    }

    impl<'a, R: Encode + Clone, S: Cloned> Encode for &'a R
    where
        Self: Ir<Type = &'a S>,
    {
        type Store = (Option<R::CType>, R::Store);

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            store.0.insert(self.clone().encode(&mut store.1))
        }
    }
    impl<'a, R: External> Encode for &'a R
    where
        Self: Ir<Type = &'a Extern>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
            self.as_extern_ptr()
        }
    }

    impl<'a, R: External> Encode for &'a mut R
    where
        Self: Ir<Type = &'a mut Extern>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
            self.as_extern_ptr_mut()
        }
    }

    impl<'slice, R: Transmute> Encode for &'slice [R]
    where
        &'slice [<R>::Target]: Encode,
        Self: Ir<Type = &'slice [Transparent]>,
    {
        type Store = <&'slice [R::Target] as Encode>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            transmute_into_target_ref_slice(self).encode(store)
        }
    }
    impl<'slice, R: ReprC> Encode for &'slice [R]
    where
        Self: Ir<Type = &'slice [Robust]>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
            RefSlice::from_slice(Some(self))
        }
    }
    impl<'slice, R: Clone> Encode for &'slice [R]
    where
        Self: Ir<Type = &'slice [Opaque]>,
    {
        type Store = Box<[*const R]>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            *store = self.iter().map(core::ptr::from_ref).collect();
            RefSlice::from_slice(Some(store))
        }
    }
    impl<'slice, R: Encode + Clone, S: Cloned> Encode for &'slice [R]
    where
        Self: Ir<Type = &'slice [S]>,
    {
        type Store =  (
            Box<[R::CType]>,
            Box<[R::Store]>,
        );

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            let slice = self.to_vec();

            store.1 = core::iter::repeat_with(Default::default)
                .take(slice.len())
                .collect();

            store.0 = slice
                .into_iter()
                .zip(&mut *store.1)
                .map(|(item, substore)| item.encode(substore))
                .collect();

            RefSlice::from_slice(Some(&store.0))
        }
    }

    impl<'slice, R: Transmute> Encode for &'slice mut [R]
    where
        &'slice mut [<R>::Target]: Encode,
        Self: Ir<Type = &'slice mut [Transparent]>,
    {
        type Store = <&'slice mut [R::Target] as Encode>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            transmute_into_target_slice_mut(self).encode(store)
        }
    }
    impl<'slice, R: ReprC> Encode for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Robust]>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
            RefMutSlice::from_slice(Some(self))
        }
    }

    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> Encode for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
            *self
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Encode + Clone, S: Cloned> Encode for Box<R>
    where
        Self: Ir<Type = Box<S>>,
    {
        type Store = R::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            (*self).encode(store)
        }
    }

    #[cfg(feature = "owned_types")]
    impl<R: Transmute> Encode for Box<[R]>
    where
        Box<[<R>::Target]>: Encode,
        Self: Ir<Type = Box<[Transparent]>>,
    {
        type Store = <Box<[R::Target]> as Encode>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
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

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            *store = self;
            RefSlice::from_slice(Some(store))
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R> Encode for Box<[R]>
    where
        Self: Ir<Type = Box<[Opaque]>>,
    {
        type Store = Box<[*mut R]>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            *store = Vec::from(self)
                .into_iter()
                .map(Box::new)
                .map(Box::into_raw)
                .collect();

            RefSlice::from_slice(Some(store))
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Encode + Clone, S: Cloned> Encode for Box<[R]>
    where
        Self: Ir<Type = Box<[S]>>,
    {
        type Store = (
            Box<[R::CType]>,
            Box<[R::Store]>,
        );

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            let boxed_slice = self;

            store.1 = core::iter::repeat_with(Default::default)
                .take(boxed_slice.len())
                .collect();

            store.0 = Vec::from(boxed_slice)
                .into_iter()
                .zip(&mut *store.1)
                .map(|(item, substore)| item.encode(substore))
                .collect();

            RefSlice::from_slice(Some(&store.0))
        }
    }

    #[cfg(feature = "owned_types")]
    impl<R: Transmute> Encode for Vec<R>
    where
        Vec<<R>::Target>: Encode,
        Self: Ir<Type = Vec<Transparent>>,
    {
        type Store = <Vec<R::Target> as Encode>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
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

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            *store = self.into_boxed_slice();
            RefSlice::from_slice(Some(store))
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R> Encode for Vec<R>
    where
        Self: Ir<Type = Vec<Opaque>>,
    {
        type Store = Box<[*mut R]>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            *store = self.into_iter().map(Box::new).map(Box::into_raw).collect();
            RefSlice::from_slice(Some(store))
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Encode + Clone, S: Cloned> Encode for Vec<R>
    where
        Self: Ir<Type = Vec<S>>,
    {
        type Store = (
            Box<[R::CType]>,
            Box<[R::Store]>,
        );

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            let vec = self;

            store.1 = core::iter::repeat_with(Default::default)
                .take(vec.len())
                .collect();

            store.0 = vec
                .into_iter()
                .zip(&mut *store.1)
                .map(|(item, substore)| item.encode(substore))
                .collect();

            RefSlice::from_slice(Some(&store.0))
        }
    }

    impl<R, const N: usize> Encode for [R; N]
    where
        Self: Ir<Type = [Opaque; N]>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
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
    impl<R: External, const N: usize> Encode for [R; N]
    where
        Self: Ir<Type = [Extern; N]>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
            assert_arr_has_non_zero_len::<N>();

            let array = self
                .into_iter()
                .map(|item| ManuallyDrop::new(item).as_extern_ptr_mut())
                .collect::<Vec<_>>()
                .try_into();

            // SAFETY: Vec<T> length is N
            unsafe { array.unwrap_unchecked() }
        }
    }
    impl<R: Encode + Clone, S: Cloned, const N: usize> Encode for [R; N]
    where
        // FIXME: https://github.com/rust-lang/rust/issues/61415
        [<R>::Store; N]: Default,
        Self: Ir<Type = [S; N]>,
    {
        type Store = [R::Store; N];

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
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

    impl<R: Optional> Encode for R
    where
        Self: Ir<Type = Option<Transparent>>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
            // WARN: We assume that niche value used by rust matches `Niche::NICHE_VALUE`
            unimplemented!();
            //// SAFETY: Guaranteed by [`Optional`]
            //let inner = unsafe {
            //    core::mem::transmute::<R, R::Inner>(self)
            //};
        }
    }
    impl<R: Encode> Encode for Option<R>
    where
        Self: Ir<Type = Option<Robust>>,
    {
        type Store = R::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            match self {
                // TODO: No need to zero the memory because it must never be read
                None => FfiTuple2(Encode::encode(0u8, &mut ()), unsafe { core::mem::zeroed() }),
                Some(value) => FfiTuple2(Encode::encode(1u8, &mut ()), value.encode(store)),
            }
        }
    }
    impl<R: Encode> Encode for Option<R>
    where
        Self: Ir<Type = Option<Opaque>>,
    {
        type Store = R::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
            unimplemented!()
        }
    }
    impl<R: Niche + Encode, S: Cloned> Encode for Option<R>
    where
        Self: Ir<Type = Option<S>>,
    {
        type Store = <R as Encode>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
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

    impl<'d, R: Transmute> Decode<'d> for R
    where
        <Self as Transmute>::Target: Decode<'d>,
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
    impl<'d, R: Ir<Type = Extern> + External> Decode<'d> for R {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Result<Self> {
            if source.is_null() {
                return Err(FfiReturn::ArgIsNull);
            }

            Ok(unsafe { Self::from_extern_ptr(source) })
        }
    }

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

    impl<'slice, R: Transmute> Decode<'slice> for &'slice [R]
    where
        &'slice [<R>::Target]: Decode<'slice>,
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
    impl<'slice, R: Decode<'slice> + Clone, S: Cloned> Decode<'slice> for &'slice [R]
    where
        Self: Ir<Type = &'slice [S]>,
    {
        type Store = (Box<[R]>, Box<[R::Store]>);

        unsafe fn decode<'itm: 'slice>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            store.1 = core::iter::repeat_with(Default::default)
                .take(source.len())
                .collect();

            let source: Box<[_]> = unsafe { source.into_rust() }
                .ok_or(FfiReturn::ArgIsNull)?
                .iter()
                .zip(&mut *store.1)
                .map(|(&item, substore)| unsafe { R::decode(item, substore) }.map(ManuallyDrop::new))
                .collect::<core::result::Result<_, _>>()?;

            store.0 = source
                .iter()
                .cloned()
                .map(ManuallyDrop::into_inner)
                .collect();

            Ok(&store.0)
        }
    }

    impl<'slice, R: Transmute> Decode<'slice> for &'slice mut [R]
    where
        &'slice mut [<R>::Target]: Decode<'slice>,
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
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: ReprC + 'd> Decode<'d> for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Result<Self> {
            Ok(Box::new(source))
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
            unsafe {
                R::decode(source, store)
                    .map(ManuallyDrop::new)
                    .map(|item| (*item).clone())
                    .map(Box::new)
            }
        }
    }

    #[cfg(feature = "owned_types")]
    impl<'d, R: Transmute> Decode<'d> for Box<[R]>
    where
        Box<[<R>::Target]>: Decode<'d>,
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
    impl<'d, R: Transmute> Decode<'d> for Vec<R>
    where
        Vec<<R>::Target>: Decode<'d>,
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
    impl<'d, R: External + 'd, const N: usize> Decode<'d> for [R; N]
    where
        Self: Ir<Type = [Extern; N]>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Result<Self> {
            assert_arr_has_non_zero_len::<N>();

            let array = source
                .into_iter()
                .map(|item| unsafe { External::from_extern_ptr(item) })
                .collect::<Vec<_>>()
                .try_into();

            Ok(unsafe { array.unwrap_unchecked() })
        }
    }
    impl<'d, R: Decode<'d> + Clone, S: Cloned, const N: usize> Decode<'d> for [R; N]
    where
        // FIXME: https://github.com/rust-lang/rust/issues/61415
        [<R>::Store; N]: Default,
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

    impl<'d, R: Optional + 'd> Decode<'d> for R
    where
        Self: Ir<Type = Option<Transparent>>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            unimplemented!()
            //Ok(core::mem::transmute::<R::Inner, R>(source))
        }
    }
    impl<'d, R: Decode<'d>> Decode<'d> for Option<R>
    where
        Self: Ir<Type = Option<Robust>>,
    {
        type Store = R::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            let discriminant: <u8 as ExternC>::CType = unsafe { Decode::decode(source.0, &mut ())? };

            match discriminant {
                0 => Ok(None),
                1 => Ok(Some(unsafe { R::decode(source.1, store) }?)),
                _ => Err(FfiReturn::TrapRepresentation),
            }
        }
    }
    impl<'d, R: Decode<'d> + 'd> Decode<'d> for Option<R>
    where
        Self: Ir<Type = Option<Opaque>>,
    {
        type Store = R::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
            unimplemented!()
        }
    }
    impl<'d, R: Niche<CType: PartialEq> + Decode<'d>, S: Cloned> Decode<'d> for Option<R>
    where
        Self: Ir<Type = Option<S>>,
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

disjoint_impls! {
    /// The trait is used to replace the type in the wrapper function generated by [`decarbonate`].
    ///
    /// Most notably, the necessity to replace the output type for a wrapper function arises when:
    /// - the wrapper function returns a reference that uses store during conversion (i.e. that are cloned)
    /// - the wrapper function takes/return an opaque type reference
    pub trait FfiWrapperType {
        /// Type used instead of the output type in the wrapper function generated by `decarbonate`
        type ReturnType;
    }

    impl<R: Transmute> FfiWrapperType for R
    where
        Self: Ir<Type = Transparent>,
        <R>::Target: FfiWrapperType,
        <<R>::Target as FfiWrapperType>::ReturnType: WrapperTypeOf<Self>,
    {
        type ReturnType = <<R::Target as FfiWrapperType>::ReturnType as WrapperTypeOf<Self>>::Type;
    }
    impl<R: ReprC> FfiWrapperType for R
    where
        Self: Ir<Type = Robust>,
    {
        type ReturnType = Self;
    }
    impl<R: Ir<Type = Extern> + External> FfiWrapperType for R {
        type ReturnType = Self;
    }

    //impl<'a, R> FfiWrapperType for Option<&'a R>
    //where
    //    Self: Ir<Type = Option<&'a Transparent>>,
    //    &'a R: Optional<Inner: WrapperTypeOf<Self>>,
    //{
    //    type ReturnType = <<&'a R as Optional>::Inner as WrapperTypeOf<Self>>::Type;
    //}
    //impl<'a, R> FfiWrapperType for Option<&'a mut R>
    //where
    //    Self: Ir<Type = Option<&'a mut Transparent>>,
    //    &'a mut R: Optional<Inner: WrapperTypeOf<Self>>,
    //{
    //    type ReturnType = <<&'a mut R as Optional>::Inner as WrapperTypeOf<Self>>::Type;
    //}
    //impl<R> FfiWrapperType for Option<Box<R>>
    //where
    //    Self: Ir<Type = Option<Box<Transparent>>>,
    //    Box<R>: Optional<Inner: WrapperTypeOf<Self>>,
    //{
    //    type ReturnType = <<Box<R> as Optional>::Inner as WrapperTypeOf<Self>>::Type;
    //}
    //impl<R: Ir<Type = Option<Opaque>>> FfiWrapperType for &R
    //where
    //    Self: Ir<Type = Option<Transparent>>,
    //{
    //    type ReturnType = Self;
    //}
    //impl<'a, R> FfiWrapperType for Option<&'a mut R>
    //where
    //    Self: Ir<Type = Option<&'a mut Opaque>>,
    //{
    //    type ReturnType = Self;
    //}
    //impl<R> FfiWrapperType for Option<Box<R>>
    //where
    //    Self: Ir<Type = Option<Box<Opaque>>>,
    //{
    //    type ReturnType = Self;
    //}
    impl<'a, R> FfiWrapperType for Option<&'a R>
    where
        Self: Ir<Type = Option<&'a Extern>>,
    {
        type ReturnType = Option<ExternRef<'a, R>>;
    }
    //impl<'a, R> FfiWrapperType for Option<&'a mut R>
    //where
    //    Self: Ir<Type = Option<&'a mut Extern>>,
    //{
    //    type ReturnType = Option<ExternRefMut<'a, R>>;
    //}
    //impl<R> FfiWrapperType for Option<Box<R>>
    //where
    //    Self: Ir<Type = Option<Box<Extern>>>,
    //{
    //    type ReturnType = Option<R>;
    //}
    //impl<R: Optional> FfiWrapperType for R
    //where
    //    Self: Ir<Type = Option<Transparent>> + crate::niche::Ir<Type = Robust>,
    //{
    //    type ReturnType = <<R::Target as FfiWrapperType>::ReturnType as WrapperTypeOf<Self>>::Type;
    //}
    impl<R: Transmute> FfiWrapperType for Option<R>
    where
        Self: Ir<Type = Option<Transparent>>,
        Option<<R>::Target>: FfiWrapperType,
        <Option<<R>::Target> as FfiWrapperType>::ReturnType: WrapperTypeOf<Self>,
    {
        type ReturnType = <<Option<R::Target> as FfiWrapperType>::ReturnType as WrapperTypeOf<Self>>::Type;
    }
    impl<R> FfiWrapperType for Option<R>
    where
        Self: Ir<Type = Option<Robust>>,
    {
        type ReturnType = Self;
    }
    impl<R> FfiWrapperType for Option<R>
    where
        Self: Ir<Type = Option<Opaque>>,
    {
        type ReturnType = Self;
    }
    //impl<R: FfiWrapperType, S: Cloned> FfiWrapperType for R
    //where
    //    Self: Ir<Type = Option<S>> + crate::niche::Ir<Type = Robust>,
    //{
    //    type ReturnType = Self;
    //}
    impl<R: FfiWrapperType, S: Cloned> FfiWrapperType for R
    where
        Self: Ir<Type = Option<S>> + crate::niche::Ir<Type = S>,
    {
        type ReturnType = Self;
    }

    impl<'itm, R: External> FfiWrapperType for &'itm R
    where
        Self: Ir<Type = &'itm Extern>,
    {
        type ReturnType = ExternRef<'itm, R>;
    }
    impl<'itm, R: Ir<Type = S> + FfiWrapperType, S: Cloned> FfiWrapperType for &'itm R
    where
        Self: Ir<Type = &'itm S>,
    {
        type ReturnType = LocalRef<'itm, R::ReturnType>;
    }

    impl<'itm, R: External> FfiWrapperType for &'itm mut R
    where
        Self: Ir<Type = &'itm mut Extern>,
    {
        type ReturnType = ExternRefMut<'itm, R>;
    }

    impl<'slice, R: Transmute> FfiWrapperType for &'slice [R]
    where
        Self: Ir<Type = &'slice [Transparent]>,
        &'slice [<R>::Target]: FfiWrapperType,
        <&'slice [<R>::Target] as FfiWrapperType>::ReturnType: WrapperTypeOf<Self>,
    {
        type ReturnType =
            <<&'slice [R::Target] as FfiWrapperType>::ReturnType as WrapperTypeOf<Self>>::Type;
    }
    impl<'a, R: ReprC> FfiWrapperType for &'a [R]
    where
        Self: Ir<Type = &'a [Robust]>,
    {
        type ReturnType = Self;
    }

    impl<'slice, R: Transmute> FfiWrapperType for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Transparent]>,
        &'slice mut [<R>::Target]: FfiWrapperType,
        <&'slice mut [<R>::Target] as FfiWrapperType>::ReturnType: WrapperTypeOf<Self>,
    {
        type ReturnType =
            <<&'slice mut [R::Target] as FfiWrapperType>::ReturnType as WrapperTypeOf<Self>>::Type;
    }
    impl<'a, R: ReprC> FfiWrapperType for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Robust]>,
    {
        type ReturnType = Self;
    }
    impl<'itm, R: Ir<Type = S> + FfiWrapperType, S: Cloned> FfiWrapperType for &'itm [R]
    where
        Self: Ir<Type = &'itm [S]>,
    {
        type ReturnType = LocalSlice<'itm, R::ReturnType>;
    }

    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> FfiWrapperType for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        type ReturnType = Self;
    }
    impl<R: External> FfiWrapperType for Box<R>
    where
        Self: Ir<Type = Box<Extern>>,
    {
        type ReturnType = R;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Ir<Type = S> + FfiWrapperType, S: Cloned> FfiWrapperType for Box<R>
    where
        Self: Ir<Type = Box<S>>,
    {
        type ReturnType = Box<<R>::ReturnType>;
    }

    #[cfg(feature = "owned_types")]
    impl<R: Transmute> FfiWrapperType for Box<[R]>
    where
        Self: Ir<Type = Box<[Transparent]>>,
        Box<[<R>::Target]>: FfiWrapperType,
        <Box<[<R>::Target]> as FfiWrapperType>::ReturnType: WrapperTypeOf<Self>,
    {
        type ReturnType =
            <<Box<[R::Target]> as FfiWrapperType>::ReturnType as WrapperTypeOf<Self>>::Type;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> FfiWrapperType for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        type ReturnType = Self;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Ir<Type = S> + FfiWrapperType, S: Cloned> FfiWrapperType for Box<[R]>
    where
        Self: Ir<Type = Box<[S]>>,
    {
        type ReturnType = Box<[R::ReturnType]>;
    }

    #[cfg(feature = "owned_types")]
    impl<R: Transmute> FfiWrapperType for Vec<R>
    where
        Self: Ir<Type = Vec<Transparent>>,
        Vec<<R>::Target>: FfiWrapperType,
        <Vec<<R>::Target> as FfiWrapperType>::ReturnType: WrapperTypeOf<Self>,
    {
        type ReturnType =
            <<Vec<R::Target> as FfiWrapperType>::ReturnType as WrapperTypeOf<Self>>::Type;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> FfiWrapperType for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        type ReturnType = Self;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Ir<Type = S> + FfiWrapperType, S: Cloned> FfiWrapperType for Vec<R>
    where
        Self: Ir<Type = Vec<S>>,
    {
        type ReturnType = Vec<<R>::ReturnType>;
    }

    impl<R: Ir<Type = S> + FfiWrapperType, S: Cloned, const N: usize> FfiWrapperType for [R; N]
    where
        Self: Ir<Type = [S; N]>,
    {
        type ReturnType = [R::ReturnType; N];
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

/// Macro for defining FFI types of a known category ([`Robust`] or [`Transmute`]).
/// The implementation for an FFI type of one of the categories incurs a lot of bloat that
/// is reduced by the use of this macro
///
/// # Safety
///
/// * If the type is [`Robust`], it derives [`ReprC`]. Check safety invariants for [`ReprC`]
/// * If the type is [`Transparent`], it derives [`Transmute`]. Check safety invariants for [`Transmute`]
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
/// // SAFETY: Type is robust #[repr(C)]
/// unsafe impl ReprC for RobustStruct {}
/// co3::mineral! { impl Robust for RobustStruct {} }
///
/// co3::mineral! {
///     unsafe impl<T> Transparent for NonNull<T> {
///         type Target = NonNullInner<T>;
///
///         const NICHE_VALUE: Self::CType = core::ptr::null_mut();
///         fn is_valid(target: &Self::Target) -> bool {
///             !target.is_null()
///         }
///     }
/// }
///
/// // Validation function is `|_| true` implicitly indicating
/// // this type is robust with respect to the wrapped type
/// co3::mineral! {
///     unsafe impl Transparent for Wrapper {
///         type Target = WrapperInner;
///     }
/// }
/// ```
#[macro_export]
macro_rules! mineral {
    (impl $(<$($impl_generics: tt $(: $bounds: path)?),*>)? Robust for $ty: ty $(where $($where_ty:ty: $where_bound:path),* )? {} ) => {
        impl$(<$($impl_generics $(: $bounds)?),*>)? $crate::ir::Ir for $ty where Self: $crate::ReprC, $($($where_ty: $where_bound),*)? {
            type Type = $crate::ir::Robust;
        }

        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::WrapperTypeOf<Self> for $ty where $($($where_ty: $where_bound),*)? {
            type Type = Self;
        }
    };
    (unsafe impl $(<$($impl_generics: tt $(: $bounds: path)?),*>)? Transparent for $ty: ty $(where $($where_ty:ty: $where_bound:path),* )? {
        type Target = $target:ty;

        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;
        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::ir::Ir for $ty where $($($where_ty: $where_bound),*)? {
            type Type = $crate::ir::Transparent;
        }

        // SAFETY: `$ty` is transmutable into `$target` and `is_valid` doesn't return false positives
        unsafe impl<$($($impl_generics $(: $bounds)?),*)?> $crate::transmute::Transmute for $ty where $($($where_ty: $where_bound),*)? {
            type Target = $target;

            fn is_valid($target_var: $target_ty) -> bool $block
        }

        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::niche::Niche for $ty where $($($where_ty: $where_bound),*)? {
            const NICHE_VALUE: $niche_ty = $niche_value;
        }

        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::niche::Ir for $ty where $($($where_ty: $where_bound),*)? {
            type Type = $crate::ir::Transparent;
        }
    };
    (unsafe impl $(<$($impl_generics: tt $(: $bounds: path)?),*>)? Transparent for $ty: ty $(where $($where_ty:ty: $where_bound:path),* )? {
        type Target = $target:ty;
        const NICHE_VALUE = "DELEGATE";
    }) => {
        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::ir::Ir for $ty where $($($where_ty: $where_bound),*)? {
            type Type = $crate::ir::Transparent;
        }

        // SAFETY: `$ty` is transmutable into `$target` and `is_valid` doesn't return false positives
        unsafe impl<$($($impl_generics $(: $bounds)?),*)?> $crate::transmute::Transmute for $ty where $($($where_ty: $where_bound),*)? {
            type Target = $target;

            fn is_valid(_: &Self::Target) -> bool {
                true
            }
        }

        // SAFETY: `$t` is robust with respect to `$target`
        unsafe impl<$($($impl_generics $(: $bounds)?),*)?> $crate::transmute::InfallibleTransmute for $ty where $($($where_ty: $where_bound),*)? {}

        const _: () = {
            use $crate::niche::Ir;

            disjoint_impls::disjoint_impls! {
                #[disjoint_impls(remote)]
                trait Ir {
                    type Type;
                }

                impl<$($($impl_generics $(: $bounds)?),*)?> Ir for $ty where
                    Self: $crate::transmute::Transmute,
                    <Self as $crate::transmute::Transmute>::Target: Ir<Type = $crate::ir::Robust>,
                    $($($where_ty: $where_bound),*)?
                {
                    type Type = $crate::ir::Robust;
                }
                impl<$($($impl_generics $(: $bounds)?),*)?> Ir for $ty where
                    Self: $crate::transmute::Transmute,
                    <Self as $crate::transmute::Transmute>::Target: Ir<Type = $crate::ir::Transparent>,
                    $($($where_ty: $where_bound),*)?
                {
                    type Type = $crate::ir::Transparent;
                }
                impl<S: $crate::ir::Cloned, $($($impl_generics $(: $bounds)?),*)?> Ir for $ty where
                    Self: $crate::transmute::Transmute + $crate::niche::Niche,
                    <Self as $crate::transmute::Transmute>::Target: Ir<Type = S>,
                    $($($where_ty: $where_bound),*)?
                {
                    type Type = Self;
                }
            }
        };

        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::niche::Niche for $ty where
            <Self as $crate::transmute::Transmute>::Target: $crate::niche::Niche,
            $($($where_ty: $where_bound),*)? {
            const NICHE_VALUE: <Self as $crate::ExternC>::CType = <$target as $crate::niche::Niche>::NICHE_VALUE;
        }

        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::WrapperTypeOf<$ty> for $target where $($($where_ty: $where_bound),*)? {
            type Type = $ty;
        }
    };
    (unsafe impl $(<$($impl_generics: tt $(: $bounds: path)?),*>)? Transparent for $ty: ty $(where $($where_ty:ty: $where_bound:path),* )? {
        type Target = $target:ty;
    } ) => {
        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::ir::Ir for $ty where $($($where_ty: $where_bound),*)? {
            type Type = $crate::ir::Transparent;
        }

        // SAFETY: `$ty` is transmutable into `$target` and `is_valid` doesn't return false positives
        unsafe impl<$($($impl_generics $(: $bounds)?),*)?> $crate::transmute::Transmute for $ty where $($($where_ty: $where_bound),*)? {
            type Target = $target;

            fn is_valid(_: &Self::Target) -> bool {
                true
            }
        }

        // SAFETY: `$t` is robust with respect to `$target`
        unsafe impl<$($($impl_generics $(: $bounds)?),*)?> $crate::transmute::InfallibleTransmute for $ty where $($($where_ty: $where_bound),*)? {}

        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::niche::Ir for $ty where $($($where_ty: $where_bound),*)? {
            type Type = $crate::ir::Robust;
        }

        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::WrapperTypeOf<$ty> for $target where $($($where_ty: $where_bound),*)? {
            type Type = $ty;
        }
    };
}

/// Define the correct [`FfiWrapperType::ReturnType`] out of
/// the given [`FfiWrapperType::ReturnType`]. The only situation
/// when this is evident is when [`Ir::Type`] is set to [`Transparent`] or [`Extern`] types
///
/// Example:
///
/// ```
/// use co3::ExternC;
///
/// #[derive(ExternC)]
/// #[mineral(unsafe(robust, has_niche = "false"))]
/// #[repr(transparent)]
/// pub struct Example(u32);
///
/// /*
/// Due to the fact that implementations of traits for [`Transparent`] structures delegate to the inner
/// type `ExternC`, macro expansion will produce the following impl of [`FfiWrapperType`] for `Example`:
///
/// impl FfiWrapperType for Example {
///     type ReturnType = u32;
/// }
///
/// which is corrected via [`WrapperTypeOf`] so that functions generated by [`decarbonate`] return [`Example`]
/// */
/// ```
pub trait WrapperTypeOf<T> {
    /// Correct return type of `T` in a function generated via [`decarbonate`]
    // TODO: Is associated type necessary if we already have a generic?
    type Type;
}

impl<R> WrapperTypeOf<Self> for *const R {
    type Type = Self;
}
impl<R> WrapperTypeOf<Self> for *mut R {
    type Type = Self;
}
impl<'itm, T> WrapperTypeOf<&'itm T> for *const T {
    type Type = &'itm T;
}
impl<'itm, T> WrapperTypeOf<&'itm mut T> for *mut T {
    type Type = &'itm mut T;
}
impl<'itm, R: ?Sized, T: ?Sized> WrapperTypeOf<&'itm R> for &'itm T {
    type Type = &'itm R;
}
impl<'itm, R: ?Sized, T: ?Sized> WrapperTypeOf<&'itm mut R> for &'itm mut T {
    type Type = &'itm mut R;
}
#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
impl<R, T> WrapperTypeOf<Box<R>> for Box<T> {
    type Type = Box<R>;
}
#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
impl<R, T> WrapperTypeOf<Vec<R>> for Vec<T> {
    type Type = Vec<R>;
}
impl<R, T, const N: usize> WrapperTypeOf<[R; N]> for [T; N] {
    type Type = [R; N];
}
impl<R, T> WrapperTypeOf<Option<R>> for Option<T> {
    type Type = Option<R>;
}

// SAFETY: `*const R` is robust with a defined C ABI regardless of whether `R` is
// When `R` is not `ReprC` the pointer is opaque; dereferencing is immediate UB
unsafe impl<R> ReprC for *const R {}
// SAFETY: `*mut R` is robust with a defined C ABI regardless of whether `R` is
// When `R` is not `ReprC` the pointer is opaque; dereferencing is immediate UB
unsafe impl<R> ReprC for *mut R {}
// SAFETY: `*mut R` is robust with a defined C ABI
unsafe impl<C: ReprC, const N: usize> ReprC for [C; N] {}

impl FfiWrapperType for () {
    type ReturnType = ();
}

macro_rules! impl_tuple {
    ( ($( $ty:ident ),+) -> $ffi_ty:ident ) => {
        /// FFI-compatible tuple with n elements
        #[repr(C)]
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $ffi_ty<$($ty: ReprC),+>($(pub $ty),+);

        #[expect(non_snake_case)]
        impl<$($ty: $crate::ReprC),+> From<($( $ty, )+)> for $ffi_ty<$($ty),+> {
            fn from(source: ($( $ty, )+)) -> Self {
                let ($($ty,)+) = source;
                Self($( $ty ),+)
            }
        }

        // SAFETY: Implementing type is robust with a defined C ABI
        unsafe impl<$($ty: ReprC),+> ReprC for $ffi_ty<$($ty),+> {}

        impl<$($ty),+> $crate::ir::Ir for ($($ty,)+) {
            type Type = Self;
        }

        // FIXME: Produce an impl of niche::Niche and niche::Ir
        // for every combination of bounds on input parameters
        impl<$($ty: $crate::niche::Niche),+> $crate::niche::Niche for ($($ty,)+) {
            const NICHE_VALUE: <Self as $crate::ExternC>::CType = $ffi_ty($(<$ty as $crate::niche::Niche>::NICHE_VALUE,)+);
        }

        const _: () = {
            use crate::niche::Ir;

            disjoint_impls::disjoint_impls! {
                #[disjoint_impls(remote)]
                trait Ir {
                    type Type;
                }

                impl<$($ty: $crate::niche::Ir<Type = $crate::ir::Robust>),+> Ir for ($($ty,)+) {
                    type Type = $crate::ir::Robust;
                }
                //impl<$($ty: $crate::niche::Ir<Type = $crate::ir::Transparent>),+> Ir for ($($ty,)+) {
                //    type Type = Self;
                //}
                // FIXME: This is even incorrect because every type should be mapped into different S
                //impl<S: $crate::ir::Cloned, $($ty: $crate::niche::Ir<Type = S>),+ + Niche> Ir for ($($ty,)+) {
                //    type Type = Self;
                //}
            }
        };

        impl<$($ty),+> Cloned for ($($ty,)+) {}

        // SAFETY: Tuple doesn't use store if it's inner types don't use it
        unsafe impl<$($ty: $crate::out_ptr::NonLocal),+> $crate::out_ptr::NonLocal for ($($ty,)+) {}

        impl<$($ty: ExternC),+> $crate::ExternC for ($($ty,)+) {
            type CType = $ffi_ty<$($ty::CType),+>;
        }

        impl<$($ty: $crate::out_ptr::OutPtr),+> $crate::out_ptr::OutPtr for ($($ty,)+) {
            type OutPtr = $ffi_ty<$($ty::OutPtr),+>;
        }

        #[expect(non_snake_case)]
        impl<$($ty: $crate::out_ptr::OutPtrWrite),+> $crate::out_ptr::OutPtrWrite for ($($ty,)+) {
            unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
                impl_tuple! {@decl_priv_out_ptr $($ty),+}
                let mut field_out_ptrs = ($(core::mem::MaybeUninit::<$ty::OutPtr>::uninit(),)+);

                let ($($ty,)+) = self;
                let field_out_ptrs: private_out_ptr::OutPtr<$($ty),+> = (&mut field_out_ptrs).into();

                unsafe {
                    $( $crate::out_ptr::OutPtrWrite::write_out($ty, field_out_ptrs.$ty.as_mut_ptr()); )+
                    out_ptr.write($ffi_ty($( field_out_ptrs.$ty.assume_init() ),+));
                }
            }
        }
        #[expect(non_snake_case)]
        impl<$($ty: $crate::out_ptr::OutPtrRead),+> $crate::out_ptr::OutPtrRead for ($($ty,)+) {
            unsafe fn try_read_out(source: Self::OutPtr) -> Result<Self> {
                impl_tuple! {@decl_priv_out_ptr $($ty),+}

                let $ffi_ty($($ty,)+) = source;
                Ok(unsafe {($( $crate::out_ptr::OutPtrRead::try_read_out($ty)?, )+)})
            }
        }

        impl<$($ty: $crate::Encode),+> $crate::Encode for ($($ty,)+) {
            type Store = ($( $ty::Store, )+);

            #[expect(non_snake_case)]
            fn encode<'itm>(self, store: &mut Self::Store) -> Self::CType where Self: 'itm {
                impl_tuple! {@decl_priv_store $($ty),+ for $crate::Encode : Store}

                let ($($ty,)+) = self;
                let store: private_store::Store<$($ty),+> = store.into();
                $ffi_ty($( <$ty as $crate::Encode>::encode($ty, store.$ty),)+)
            }
        }
        impl<'d, $($ty: $crate::Decode<'d>),+> $crate::Decode<'d> for ($($ty,)+) {
            type Store = ($( $ty::Store, )+);

            #[expect(non_snake_case)]
            unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
                impl_tuple! {@decl_priv_store $($ty),+ for $crate::Decode<'itm> : Store}

                let $ffi_ty($($ty,)+) = source;
                let store: private_store::Store<$($ty),+> = store.into();
                Ok(unsafe {($( <$ty as $crate::Decode<'d>>::decode($ty, store.$ty)?, )+)})
            }
        }

        impl<$($ty),+> $crate::FfiWrapperType for ($($ty,)+) {
            type ReturnType = Self;
        }
        impl<$($ty),+> $crate::WrapperTypeOf<Self> for ($($ty,)+) {
            type Type = Self;
        }
    };

    // NOTE: This is a trick to index tuples
    ( @decl_priv_store $( $ty:ident ),+ for $trait:path : $store:ident) => {
        mod private_store {
            pub struct Store<'itm, $($ty: $trait),+> {
                $(pub $ty: &'itm mut $ty::$store),+
            }

            impl<'itm, $($ty: $trait),+> From<&'itm mut ($($ty::$store,)+)> for Store<'itm, $($ty,)+> {
                fn from(($($ty,)+): &'itm mut ($($ty::$store,)+)) -> Self {
                    Self {$($ty,)+}
                }
            }
        }
    };

    // NOTE: This is a trick to index tuples
    ( @decl_priv_out_ptr $( $ty:ident ),+ $(,)? ) => {
        mod private_out_ptr {
            #[allow(dead_code)]
            pub struct OutPtr<'itm, $($ty: $crate::out_ptr::OutPtrWrite),+> {
                $(pub $ty: &'itm mut core::mem::MaybeUninit::<$ty::OutPtr>),+
            }

            impl<'itm, $($ty: $crate::out_ptr::OutPtrWrite),+> From<&'itm mut ($(core::mem::MaybeUninit::<$ty::OutPtr>,)+)> for OutPtr<'itm, $($ty),+> {
                fn from(($($ty,)+): &'itm mut ($(core::mem::MaybeUninit::<$ty::OutPtr>,)+)) -> Self {
                    Self {$($ty,)+}
                }
            }
        }
    };
}

impl_tuple! {(A) -> FfiTuple1}
impl_tuple! {(A, B) -> FfiTuple2}
impl_tuple! {(A, B, C) -> FfiTuple3}
impl_tuple! {(A, B, C, D) -> FfiTuple4}
impl_tuple! {(A, B, C, D, E) -> FfiTuple5}
impl_tuple! {(A, B, C, D, E, F) -> FfiTuple6}
impl_tuple! {(A, B, C, D, E, F, G) -> FfiTuple7}
impl_tuple! {(A, B, C, D, E, F, G, H) -> FfiTuple8}
impl_tuple! {(A, B, C, D, E, F, G, H, I) -> FfiTuple9}
impl_tuple! {(A, B, C, D, E, F, G, H, I, J) -> FfiTuple10}
impl_tuple! {(A, B, C, D, E, F, G, H, I, J, K) -> FfiTuple11}
impl_tuple! {(A, B, C, D, E, F, G, H, I, J, K, L) -> FfiTuple12}
