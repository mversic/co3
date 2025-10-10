//! Structures and macros related to FFI and generation of FFI bindings. Any type that implements
//! [`ExternC`] can be used in the FFI bindings generated with [`carbonate`]/[`decarbonate`]. It
//! is advisable to implement [`Ir`] and benefit from automatic implementation of [`ExternC`]
#![no_std]

extern crate alloc;

use core::mem::ManuallyDrop;

use alloc::{boxed::Box, vec::Vec};

pub use co3_derive::*;
use derive_more::Display;
use disjoint_impls::disjoint_impls;

use crate::{
    ir::{External, Ir, Opaque, Robust, Transparent},
    option::{Niche, WithoutNiche},
    out_ptr::NonLocal,
    repr_c::{Cloned, default_init_arr, write_non_local},
    slice::{OutBoxedSlice, RefMutSlice, RefSlice},
    transmute::{
        Transmute, transmute_from_target, transmute_from_target_box,
        transmute_from_target_boxed_slice, transmute_from_target_ref_slice,
        transmute_from_target_slice_mut, transmute_from_target_vec, transmute_into_target,
        transmute_into_target_box, transmute_into_target_boxed_slice,
        transmute_into_target_ref_slice, transmute_into_target_slice_mut,
        transmute_into_target_vec,
    },
};

pub mod handle;
pub mod ir;
pub mod option;
pub mod out_ptr;
pub mod primitives;
pub mod repr_c;
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

disjoint_impls! {
    /// A Rust type that has an `extern "C"` ABI
    pub trait ExternC {
        /// The C-compatible representation of this Rust type.
        type CType: ReprC;
    }

    // TODO: `ExternC` cannot be implemented for `&mut T`. Add compile test
    // TODO: `ExternC` cannot be implemented for `&mut [T]`. Add compile test

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
    impl<R: Transmute> ExternC for R
    where
        Self: Ir<Type = Transparent>,
        Self::Target: ExternC,
    {
        type CType = <<R>::Target as ExternC>::CType;
    }

    impl<'itm, R: External> ExternC for &'itm R
    where
        Self: Ir<Type = &'itm Extern>,
    {
        type CType = *const Extern;
    }
    impl<'a, R: Ir<Type = S> + ExternC, S: Cloned> ExternC for &'a R
    where
        Self: Ir<Type = &'a S>,
    {
        type CType = *const <R>::CType;
    }

    impl<'a, R: ReprC> ExternC for &'a [R]
    where
        Self: Ir<Type = &'a [Robust]>,
    {
        type CType = RefSlice<R>;
    }
    impl<'itm, R> ExternC for &'itm [R]
    where
        Self: Ir<Type = &'itm [Opaque]>,
    {
        type CType = RefSlice<*const R>;
    }
    impl<'slice, R: Transmute> ExternC for &'slice [R]
    where
        Self: Ir<Type = &'slice [Transparent]>,
        &'slice [<R>::Target]: ExternC,
    {
        type CType = <&'slice [<R>::Target] as ExternC>::CType;
    }
    impl<'a, R: Ir<Type = S> + ExternC, S: Cloned> ExternC for &'a [R]
    where
        Self: Ir<Type = &'a [S]>,
    {
        type CType = RefSlice<<R>::CType>;
    }

    impl<'a, R: ReprC> ExternC for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Robust]>,
    {
        type CType = RefMutSlice<R>;
    }
    impl<'itm, R> ExternC for &'itm mut [R]
    where
        Self: Ir<Type = &'itm mut [Opaque]>,
    {
        type CType = RefMutSlice<*mut R>;
    }
    impl<'slice, R: Transmute> ExternC for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Transparent]>,
        &'slice mut [<R>::Target]: ExternC,
    {
        type CType = <&'slice mut [<R>::Target] as ExternC>::CType;
    }

    impl<R: ReprC> ExternC for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        type CType = *const R;
    }
    impl<R> ExternC for Box<R>
    where
        Self: Ir<Type = Box<Opaque>>,
    {
        type CType = *mut R;
    }
    impl<R: External> ExternC for Box<R>
    where
        Self: Ir<Type = Box<Extern>>,
    {
        type CType = *mut Extern;
    }
    impl<R: Transmute> ExternC for Box<R>
    where
        Self: Ir<Type = Box<Transparent>>,
        Box<<R>::Target>: ExternC,
    {
        type CType = <Box<<R>::Target> as ExternC>::CType;
    }
    impl<R: Ir<Type = S> + ExternC, S: Cloned> ExternC for Box<R>
    where
        Self: Ir<Type = Box<S>>,
    {
        type CType = *const <R>::CType;
    }

    impl<R: ReprC> ExternC for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        type CType = RefSlice<R>;
    }
    impl<R> ExternC for Box<[R]>
    where
        Self: Ir<Type = Box<[Opaque]>>,
    {
        type CType = RefSlice<*mut R>;
    }
    impl<R: Transmute> ExternC for Box<[R]>
    where
        Self: Ir<Type = Box<[Transparent]>>,
        Box<[<R>::Target]>: ExternC,
    {
        type CType = <Box<[<R>::Target]> as ExternC>::CType;
    }
    impl<R: Ir<Type = S> + ExternC, S: Cloned> ExternC for Box<[R]>
    where
        Self: Ir<Type = Box<[S]>>,
    {
        type CType = RefSlice<<R>::CType>;
    }

    impl<R: ReprC> ExternC for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        type CType = RefSlice<R>;
    }
    impl<R> ExternC for Vec<R>
    where
        Self: Ir<Type = Vec<Opaque>>,
    {
        type CType = RefSlice<*mut R>;
    }
    impl<R: Transmute> ExternC for Vec<R>
    where
        Self: Ir<Type = Vec<Transparent>>,
        Vec<<R>::Target>: ExternC,
    {
        type CType = <Vec<<R>::Target> as ExternC>::CType;
    }
    impl<R: Ir<Type = S> + ExternC, S: Cloned> ExternC for Vec<R>
    where
        Self: Ir<Type = Vec<S>>,
    {
        type CType = RefSlice<<R>::CType>;
    }

    impl<R, const N: usize> ExternC for [R; N]
    where
        Self: Ir<Type = [Opaque; N]>,
    {
        type CType = [*mut R; N];
    }
    impl<R: Ir<Type = S> + ExternC, S: Cloned, const N: usize> ExternC for [R; N]
    where
        Self: Ir<Type = [S; N]>,
    {
        type CType = [<R>::CType; N];
    }

    impl<R: ExternC> ExternC for Option<R>
    where
        Self: Ir<Type = Option<WithoutNiche>>,
    {
        type CType = FfiTuple2<<u8 as ExternC>::CType, <R>::CType>;
    }
    impl<R: Niche> ExternC for Option<R>
    where
        Self: Ir<Type = Self>,
    {
        type CType = <R>::CType;
    }

    impl<'itm, R: NonLocal + 'itm, S: Cloned> ExternC for LocalRef<'itm, R>
    where
        &'itm R: Ir<Type = &'itm S> + ExternC,
        Self: Ir<Type = &'itm S>,
    {
        type CType = <&'itm R as ExternC>::CType;
    }
    impl<'itm, R: NonLocal + 'itm, S: Cloned> ExternC for LocalSlice<'itm, R>
    where
        &'itm [R]: Ir<Type = &'itm [S]> + ExternC,
        Self: Ir<Type = &'itm [S]>,
    {
        type CType = <&'itm [R] as ExternC>::CType;
    }
    // FIXME: Bounds are fishy here?
    impl<'itm, R, S> ExternC for LocalSlice<'itm, R>
    where
        Vec<R>: Ir<Type = Vec<S>> + ExternC,
        Self: Ir<Type = Vec<S>>,
    {
        type CType = <Vec<R> as ExternC>::CType;
    }
}

disjoint_impls! {
    /// Facilitates conversion of rust types to/from `ReprC` types.
    pub trait FfiConvert<'itm>: ExternC + Sized {
        /// Type into which state can be stored during conversion from [`Self`]. Useful for
        /// returning owning heap allocated types or non-owning types that are not transmutable.
        /// Serves similar purpose as does context in a closure
        type RustStore: Default;

        /// Type into which state can be stored during conversion into [`Self`]. Useful for
        /// returning non-owning types that are not transmutable. Serves similar purpose as
        /// does context in a closure
        type FfiStore: Default;

        /// Perform the conversion from [`Self`] into [`Self::CType`]
        fn encode(self, store: &'itm mut Self::RustStore) -> Self::CType;

        /// Perform the conversion from [`Self::CType`] into [`Self`]
        ///
        /// # Errors
        ///
        /// Check [`FfiReturn`]
        ///
        /// # Safety
        ///
        /// All conversions from a pointer must ensure pointer validity beforehand
        unsafe fn decode(source: Self::CType, store: &'itm mut Self::FfiStore) -> Result<Self>;
    }

    impl<R: ReprC> FfiConvert<'_> for R
    where
        Self: Ir<Type = Robust>,
    {
        type RustStore = ();
        type FfiStore = ();

        fn encode(self, (): &mut ()) -> Self::CType {
            self
        }

        unsafe fn decode(source: Self::CType, (): &mut ()) -> Result<Self> {
            Ok(source)
        }
    }
    impl<R> FfiConvert<'_> for R
    where
        Self: Ir<Type = Opaque>,
    {
        type RustStore = ();
        type FfiStore = ();

        fn encode(self, (): &mut ()) -> Self::CType {
            Box::into_raw(Box::new(self))
        }
        unsafe fn decode(source: Self::CType, (): &mut ()) -> Result<Self> {
            if source.is_null() {
                return Err(FfiReturn::ArgIsNull);
            }

            Ok(*unsafe { Box::from_raw(source) })
        }
    }
    impl<'itm, R: Transmute> FfiConvert<'itm> for R
    where
        <Self as Transmute>::Target: FfiConvert<'itm>,
        Self: Ir<Type = Transparent>,
    {
        type RustStore = <<R>::Target as FfiConvert<'itm>>::RustStore;
        type FfiStore = <<R>::Target as FfiConvert<'itm>>::FfiStore;

        fn encode(self, store: &'itm mut Self::RustStore) -> Self::CType {
            transmute_into_target(self).encode(store)
        }

        unsafe fn decode(source: Self::CType, store: &'itm mut Self::FfiStore) -> Result<Self> {
            unsafe {
                FfiConvert::decode(source, store).and_then(|inner| transmute_from_target(inner))
            }
        }
    }

    impl<'itm, R: Ir<Type = S> + FfiConvert<'itm> + Clone, S: Cloned> FfiConvert<'itm> for &'itm R
    where
        Self: Ir<Type = &'itm S>,
    {
        type RustStore = (Option<R::CType>, <R>::RustStore);
        type FfiStore = (Option<R>, <R>::FfiStore);

        fn encode(self, store: &'itm mut Self::RustStore) -> Self::CType {
            store.0.insert(self.clone().encode(&mut store.1))
        }

        unsafe fn decode(source: Self::CType, store: &'itm mut Self::FfiStore) -> Result<Self> {
            unsafe {
                if source.as_ref().is_none() {
                    return Err(FfiReturn::ArgIsNull);
                }

                Ok(store.0.insert(
                    <R>::decode(source.read(), &mut store.1)
                        .map(ManuallyDrop::new)
                        .map(|item| (*item).clone())?,
                ))
            }
        }
    }

    impl<'itm, R: ReprC> FfiConvert<'itm> for &'itm [R]
    where
        Self: Ir<Type = &'itm [Robust]>,
    {
        type RustStore = ();
        type FfiStore = ();

        fn encode(self, (): &mut ()) -> Self::CType {
            RefSlice::from_slice(Some(self))
        }

        unsafe fn decode(source: Self::CType, (): &mut ()) -> Result<Self> {
            unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)
        }
    }
    impl<'slice, R: Clone> FfiConvert<'slice> for &'slice [R]
    where
        Self: Ir<Type = &'slice [Opaque]>,
    {
        type RustStore = Box<[*const R]>;
        type FfiStore = Box<[R]>;

        fn encode(self, store: &mut Self::RustStore) -> Self::CType {
            *store = self.iter().map(core::ptr::from_ref).collect();
            RefSlice::from_slice(Some(store))
        }

        unsafe fn decode(source: Self::CType, store: &'slice mut Self::FfiStore) -> Result<Self> {
            let source = unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)?;

            *store = source
                .iter()
                .map(|item| {
                    unsafe { item.as_ref() }
                        // NOTE: This function clones every opaque pointer in the slice. This could
                        // be avoided with the entire slice being opaque, if that even makes sense.
                        .cloned()
                        .ok_or(FfiReturn::ArgIsNull)
                })
                .collect::<core::result::Result<_, _>>()?;

            Ok(store)
        }
    }
    impl<'slice, R: Transmute> FfiConvert<'slice> for &'slice [R]
    where
        &'slice [<R>::Target]: FfiConvert<'slice>,
        Self: Ir<Type = &'slice [Transparent]>,
    {
        type RustStore = <&'slice [<R>::Target] as FfiConvert<'slice>>::RustStore;
        type FfiStore = <&'slice [<R>::Target] as FfiConvert<'slice>>::FfiStore;

        fn encode(self, store: &'slice mut Self::RustStore) -> Self::CType {
            transmute_into_target_ref_slice(self).encode(store)
        }

        unsafe fn decode(source: Self::CType, store: &'slice mut Self::FfiStore) -> Result<Self> {
            unsafe {
                let slice = <&[<R>::Target]>::decode(source, store)?;
                transmute_from_target_ref_slice(slice)
            }
        }
    }
    impl<'slice, R: Ir<Type = S>, S: Cloned> FfiConvert<'slice> for &'slice [R]
    where
        R: FfiConvert<'slice> + Clone,
        Self: Ir<Type = &'slice [S]>,
    {
        type RustStore = (Box<[R::CType]>, Box<[<R>::RustStore]>);
        type FfiStore = (Box<[R]>, Box<[<R>::FfiStore]>);

        fn encode(self, store: &'slice mut Self::RustStore) -> Self::CType {
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

        unsafe fn decode(source: Self::CType, store: &'slice mut Self::FfiStore) -> Result<Self> {
            store.1 = core::iter::repeat_with(Default::default)
                .take(source.len())
                .collect();

            let source: Box<[_]> = unsafe { source.into_rust() }
                .ok_or(FfiReturn::ArgIsNull)?
                .iter()
                .zip(&mut *store.1)
                .map(|(&item, substore)| {
                    unsafe { <R>::decode(item, substore) }.map(ManuallyDrop::new)
                })
                .collect::<core::result::Result<_, _>>()?;

            store.0 = source
                .iter()
                .cloned()
                .map(ManuallyDrop::into_inner)
                .collect();

            Ok(&store.0)
        }
    }

    impl<'slice, R: ReprC> FfiConvert<'slice> for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Robust]>,
    {
        type RustStore = ();
        type FfiStore = ();

        fn encode(self, (): &mut ()) -> Self::CType {
            RefMutSlice::from_slice(Some(self))
        }

        unsafe fn decode(source: Self::CType, (): &mut ()) -> Result<Self> {
            unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)
        }
    }
    impl<'slice, R: Clone> FfiConvert<'slice> for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Opaque]>,
    {
        type RustStore = Box<[*mut R]>;
        type FfiStore = Box<[R]>;

        fn encode(self, store: &mut Self::RustStore) -> Self::CType {
            *store = self.iter_mut().map(core::ptr::from_mut).collect();
            RefMutSlice::from_slice(Some(store))
        }

        unsafe fn decode(source: Self::CType, store: &'slice mut Self::FfiStore) -> Result<Self> {
            let source = unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)?;

            *store = source
                .iter()
                .map(|item| {
                    unsafe { item.as_mut() }
                        // NOTE: This function clones every opaque pointer in the slice. This could
                        // be avoided with the entire slice being opaque, if that even makes sense.
                        .cloned()
                        .ok_or(FfiReturn::ArgIsNull)
                })
                .collect::<core::result::Result<_, _>>()?;

            Ok(store)
        }
    }
    impl<'slice, R: Transmute> FfiConvert<'slice> for &'slice mut [R]
    where
        &'slice mut [<R>::Target]: FfiConvert<'slice>,
        Self: Ir<Type = &'slice mut [Transparent]>,
    {
        type RustStore = <&'slice mut [<R>::Target] as FfiConvert<'slice>>::RustStore;
        type FfiStore = <&'slice mut [<R>::Target] as FfiConvert<'slice>>::FfiStore;

        fn encode(self, store: &'slice mut Self::RustStore) -> Self::CType {
            transmute_into_target_slice_mut(self).encode(store)
        }

        unsafe fn decode(source: Self::CType, store: &'slice mut Self::FfiStore) -> Result<Self> {
            unsafe {
                <&mut [<R>::Target]>::decode(source, store)
                    .and_then(|output| transmute_from_target_slice_mut(output))
            }
        }
    }

    impl<R: ReprC> FfiConvert<'_> for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        type RustStore = Option<Self>;
        type FfiStore = ();

        fn encode(self, store: &mut Self::RustStore) -> Self::CType {
            &mut **store.insert(self)
        }

        unsafe fn decode(source: Self::CType, (): &mut ()) -> Result<Self> {
            if source.is_null() {
                return Err(FfiReturn::ArgIsNull);
            }

            Ok(Box::new(unsafe { source.read() }))
        }
    }
    impl<R> FfiConvert<'_> for Box<R>
    where
        Self: Ir<Type = Box<Opaque>>,
    {
        // FIXME: Which type to use for unused store?
        type RustStore = ();
        type FfiStore = ();

        fn encode(self, (): &mut ()) -> Self::CType {
            Box::into_raw(self)
        }

        unsafe fn decode(source: Self::CType, (): &mut ()) -> Result<Self> {
            if source.is_null() {
                return Err(FfiReturn::ArgIsNull);
            }

            Ok(unsafe { Box::from_raw(source) })
        }
    }
    impl<'itm, R: Transmute> FfiConvert<'itm> for Box<R>
    where
        Box<<R>::Target>: FfiConvert<'itm>,
        Self: Ir<Type = Box<Transparent>>,
    {
        type RustStore = <Box<<R>::Target> as FfiConvert<'itm>>::RustStore;
        type FfiStore = <Box<<R>::Target> as FfiConvert<'itm>>::FfiStore;

        fn encode(self, store: &'itm mut Self::RustStore) -> Self::CType {
            transmute_into_target_box(self).encode(store)
        }

        unsafe fn decode(source: Self::CType, store: &'itm mut Self::FfiStore) -> Result<Self> {
            unsafe {
                Box::<<R>::Target>::decode(source, store)
                    .and_then(|output| transmute_from_target_box(output))
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + FfiConvert<'itm> + Clone, S: Cloned> FfiConvert<'itm> for Box<R>
    where
        Self: Ir<Type = Box<S>>,
    {
        type RustStore = (Option<R::CType>, <R>::RustStore);
        type FfiStore = <R>::FfiStore;

        fn encode(self, store: &'itm mut Self::RustStore) -> Self::CType {
            store.0.insert((*self).encode(&mut store.1))
        }
        unsafe fn decode(source: Self::CType, store: &'itm mut Self::FfiStore) -> Result<Self> {
            unsafe {
                if source.as_ref().is_none() {
                    return Err(FfiReturn::ArgIsNull);
                }

                <R>::decode(source.read(), store)
                    .map(ManuallyDrop::new)
                    .map(|item| (*item).clone())
                    .map(Box::new)
            }
        }
    }

    impl<R: ReprC> FfiConvert<'_> for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        type RustStore = Self;
        type FfiStore = ();

        fn encode(self, store: &mut Self::RustStore) -> Self::CType {
            *store = self;
            RefSlice::from_slice(Some(store))
        }

        unsafe fn decode(source: Self::CType, (): &mut ()) -> Result<Self> {
            unsafe { source.into_rust() }
                .ok_or(FfiReturn::ArgIsNull)
                .map(|slice| slice.into())
        }
    }
    impl<R> FfiConvert<'_> for Box<[R]>
    where
        Self: Ir<Type = Box<[Opaque]>>,
    {
        type RustStore = Box<[*mut R]>;
        type FfiStore = ();

        fn encode(self, store: &mut Self::RustStore) -> Self::CType {
            *store = Vec::from(self)
                .into_iter()
                .map(Box::new)
                .map(Box::into_raw)
                .collect();

            RefSlice::from_slice(Some(store))
        }

        unsafe fn decode(source: Self::CType, (): &mut ()) -> Result<Self> {
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
    impl<'itm, R: Transmute> FfiConvert<'itm> for Box<[R]>
    where
        Box<[<R>::Target]>: FfiConvert<'itm>,
        Self: Ir<Type = Box<[Transparent]>>,
    {
        type RustStore = <Box<[<R>::Target]> as FfiConvert<'itm>>::RustStore;
        type FfiStore = <Box<[<R>::Target]> as FfiConvert<'itm>>::FfiStore;

        fn encode(self, store: &'itm mut Self::RustStore) -> Self::CType {
            transmute_into_target_boxed_slice(self).encode(store)
        }

        unsafe fn decode(source: Self::CType, store: &'itm mut Self::FfiStore) -> Result<Self> {
            unsafe {
                <Box<[<R>::Target]>>::decode(source, store)
                    .and_then(|output| transmute_from_target_boxed_slice(output))
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + FfiConvert<'itm> + Clone, S: Cloned> FfiConvert<'itm> for Box<[R]>
    where
        Self: Ir<Type = Box<[S]>>,
    {
        type RustStore = (Box<[R::CType]>, Box<[<R>::RustStore]>);
        type FfiStore = Box<[<R>::FfiStore]>;

        fn encode(self, store: &'itm mut Self::RustStore) -> Self::CType {
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
        unsafe fn decode(source: Self::CType, store: &'itm mut Self::FfiStore) -> Result<Self> {
            let slice = unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)?;

            *store = core::iter::repeat_with(Default::default)
                .take(slice.len())
                .collect();

            let vec: Box<[_]> = slice
                .iter()
                .copied()
                .zip(&mut **store)
                .map(|(item, substore)| {
                    unsafe { <R>::decode(item, substore) }.map(ManuallyDrop::new)
                })
                .collect::<core::result::Result<_, _>>()?;

            Ok(vec.iter().cloned().map(ManuallyDrop::into_inner).collect())
        }
    }

    impl<R: ReprC> FfiConvert<'_> for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        type RustStore = Box<[R]>;
        type FfiStore = ();

        fn encode(self, store: &mut Self::RustStore) -> Self::CType {
            *store = self.into_boxed_slice();
            RefSlice::from_slice(Some(store))
        }

        unsafe fn decode(source: Self::CType, (): &mut ()) -> Result<Self> {
            unsafe { source.into_rust() }
                .ok_or(FfiReturn::ArgIsNull)
                .map(|slice| slice.to_vec())
        }
    }
    impl<R> FfiConvert<'_> for Vec<R>
    where
        Self: Ir<Type = Vec<Opaque>>,
    {
        type RustStore = Box<[*mut R]>;
        type FfiStore = ();

        fn encode(self, store: &mut Self::RustStore) -> Self::CType {
            *store = self.into_iter().map(Box::new).map(Box::into_raw).collect();
            RefSlice::from_slice(Some(store))
        }

        unsafe fn decode(source: Self::CType, (): &mut ()) -> Result<Self> {
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
    impl<'itm, R: Transmute> FfiConvert<'itm> for Vec<R>
    where
        Vec<<R>::Target>: FfiConvert<'itm>,
        Self: Ir<Type = Vec<Transparent>>,
    {
        type RustStore = <Vec<<R>::Target> as FfiConvert<'itm>>::RustStore;
        type FfiStore = <Vec<<R>::Target> as FfiConvert<'itm>>::FfiStore;

        fn encode(self, store: &'itm mut Self::RustStore) -> Self::CType {
            transmute_into_target_vec(self).encode(store)
        }

        unsafe fn decode(source: Self::CType, store: &'itm mut Self::FfiStore) -> Result<Self> {
            unsafe {
                <Vec<<R>::Target>>::decode(source, store)
                    .and_then(|output| transmute_from_target_vec(output))
            }
        }
    }
    impl<'itm, R: Ir<Type = S>, S: Cloned> FfiConvert<'itm> for Vec<R>
    where
        R: FfiConvert<'itm> + Clone,
        Self: Ir<Type = Vec<S>>,
    {
        type RustStore = (Box<[R::CType]>, Box<[<R>::RustStore]>);
        type FfiStore = Box<[<R>::FfiStore]>;

        fn encode(self, store: &'itm mut Self::RustStore) -> Self::CType {
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
        unsafe fn decode(source: Self::CType, store: &'itm mut Self::FfiStore) -> Result<Self> {
            let slice = unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)?;

            *store = core::iter::repeat_with(Default::default)
                .take(slice.len())
                .collect();

            let vec: Box<[_]> = slice
                .iter()
                .copied()
                .zip(&mut **store)
                .map(|(item, substore)| unsafe {
                    <R>::decode(item, substore).map(ManuallyDrop::new)
                })
                .collect::<core::result::Result<_, _>>()?;

            Ok(vec.iter().cloned().map(ManuallyDrop::into_inner).collect())
        }
    }

    impl<R: External> FfiConvert<'_> for Box<R>
    where
        Self: Ir<Type = Box<Extern>>,
    {
        type RustStore = ();
        type FfiStore = ();

        fn encode(self, (): &mut ()) -> Self::CType {
            ManuallyDrop::new(*self).as_extern_ptr_mut()
        }

        unsafe fn decode(source: Self::CType, (): &mut ()) -> Result<Self> {
            if source.is_null() {
                return Err(FfiReturn::ArgIsNull);
            }

            Ok(Box::new(unsafe { External::from_extern_ptr(source) }))
        }
    }

    impl<R, const N: usize> FfiConvert<'_> for [R; N]
    where
        Self: Ir<Type = [Opaque; N]>,
    {
        type RustStore = ();
        type FfiStore = ();

        fn encode(self, (): &mut Self::RustStore) -> Self::CType {
            let array = self
                .into_iter()
                .map(Box::new)
                .map(Box::into_raw)
                .collect::<Vec<_>>()
                .try_into();

            // SAFETY: Vec<T> length is N
            unsafe { array.unwrap_unchecked() }
        }

        unsafe fn decode(source: Self::CType, (): &mut ()) -> Result<Self> {
            let array = source
                .into_iter()
                .map(|item| unsafe {
                    if let Some(item) = item.as_mut() {
                        return Ok(*Box::from_raw(item));
                    }

                    Err(FfiReturn::ArgIsNull)
                })
                .collect::<core::result::Result<Vec<R>, _>>()?
                .try_into();

            Ok(unsafe { array.unwrap_unchecked() })
        }
    }
    impl<'itm, R: Ir<Type = S>, S: Cloned, const N: usize> FfiConvert<'itm> for [R; N]
    where
        R: FfiConvert<'itm> + Clone,
        [<R>::RustStore; N]: Default,
        [<R>::FfiStore; N]: Default,
        Self: Ir<Type = [S; N]>,
    {
        type RustStore = [<R>::RustStore; N];
        type FfiStore = [<R>::FfiStore; N];

        fn encode(self, store: &'itm mut Self::RustStore) -> Self::CType {
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
        unsafe fn decode(source: Self::CType, store: &'itm mut Self::FfiStore) -> Result<Self> {
            let vec: core::result::Result<[_; N], _> = source
                .into_iter()
                .zip(store.iter_mut())
                .map(|(item, substore)| unsafe {
                    <R>::decode(item, substore).map(ManuallyDrop::new)
                })
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

    impl<'itm, R: FfiConvert<'itm>> FfiConvert<'itm> for Option<R>
    where
        Self: Ir<Type = Option<WithoutNiche>>,
    {
        type RustStore = <R>::RustStore;
        type FfiStore = <R>::FfiStore;

        fn encode(self, store: &'itm mut Self::RustStore) -> Self::CType {
            match self {
                // TODO: No need to zero the memory because it must never be read
                None => FfiTuple2(FfiConvert::encode(0u8, &mut ()), unsafe { core::mem::zeroed() }),
                Some(value) => FfiTuple2(FfiConvert::encode(1u8, &mut ()), value.encode(store)),
            }
        }

        unsafe fn decode(source: Self::CType, store: &'itm mut Self::FfiStore) -> Result<Self> {
            let discriminant: <u8 as ExternC>::CType = unsafe { FfiConvert::decode(source.0, &mut ())? };

            match discriminant {
                0 => Ok(None),
                1 => Ok(Some(unsafe { <R>::decode(source.1, store) }?)),
                _ => Err(FfiReturn::TrapRepresentation),
            }
        }
    }
    impl<'itm, R: Niche + FfiConvert<'itm>> FfiConvert<'itm> for Option<R>
    where
        <R as ExternC>::CType: PartialEq,
        Self: Ir<Type = Self>,
    {
        type RustStore = <R>::RustStore;
        type FfiStore = <R>::FfiStore;

        fn encode(self, store: &'itm mut Self::RustStore) -> Self::CType {
            if let Some(value) = self {
                return value.encode(store);
            }

            <R>::NICHE_VALUE
        }

        unsafe fn decode(source: Self::CType, store: &'itm mut Self::FfiStore) -> Result<Self> {
            if source == <R>::NICHE_VALUE {
                return Ok(None);
            }

            Ok(Some(unsafe { <R>::decode(source, store) }?))
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
        /// Type used instead of the input type in the wrapper function generated by `decarbonate`
        type InputType;
        /// Type used instead of the output type in the wrapper function generated by `decarbonate`
        type ReturnType;
    }

    impl<R: ReprC> FfiWrapperType for R
    where
        Self: Ir<Type = Robust>,
    {
        type InputType = Self;
        type ReturnType = Self;
    }
    impl<R: Transmute> FfiWrapperType for R
    where
        Self: Ir<Type = Transparent>,
        <R>::Target: FfiWrapperType,
        <<R>::Target as FfiWrapperType>::InputType: WrapperTypeOf<Self>,
        <<R>::Target as FfiWrapperType>::ReturnType: WrapperTypeOf<Self>,
    {
        type InputType = <<<R>::Target as FfiWrapperType>::InputType as WrapperTypeOf<Self>>::Type;
        type ReturnType = <<<R>::Target as FfiWrapperType>::ReturnType as WrapperTypeOf<Self>>::Type;
    }

    impl<'itm, R: External> FfiWrapperType for &'itm R
    where
        Self: Ir<Type = &'itm Extern>,
    {
        type InputType = <R>::RefType<'itm>;
        type ReturnType = <R>::RefType<'itm>;
    }
    impl<'itm, R: Ir<Type = S> + FfiWrapperType, S: Cloned> FfiWrapperType for &'itm R
    where
        Self: Ir<Type = &'itm S>,
    {
        type InputType = &'itm <R>::InputType;
        type ReturnType = LocalRef<'itm, <R>::ReturnType>;
    }

    impl<'itm, R: External> FfiWrapperType for &'itm mut R
    where
        Self: Ir<Type = &'itm mut Extern>,
    {
        type InputType = <R>::RefMutType<'itm>;
        type ReturnType = <R>::RefMutType<'itm>;
    }

    impl<'a, R: ReprC> FfiWrapperType for &'a [R]
    where
        Self: Ir<Type = &'a [Robust]>,
    {
        type InputType = Self;
        type ReturnType = Self;
    }
    impl<'slice, R: Transmute> FfiWrapperType for &'slice [R]
    where
        Self: Ir<Type = &'slice [Transparent]>,
        &'slice [<R>::Target]: FfiWrapperType,
        <&'slice [<R>::Target] as FfiWrapperType>::InputType: WrapperTypeOf<Self>,
        <&'slice [<R>::Target] as FfiWrapperType>::ReturnType: WrapperTypeOf<Self>,
    {
        type InputType =
            <<&'slice [<R>::Target] as FfiWrapperType>::InputType as WrapperTypeOf<Self>>::Type;
        type ReturnType =
            <<&'slice [<R>::Target] as FfiWrapperType>::ReturnType as WrapperTypeOf<Self>>::Type;
    }

    impl<'itm, R: External> FfiWrapperType for &'itm [&'itm R]
    where
        Self: Ir<Type = &'itm [&'itm Extern]>,
    {
        type InputType = &'itm [<R>::RefType<'itm>];
        type ReturnType = &'itm [<R>::RefType<'itm>];
    }

    impl<'itm, R: External> FfiWrapperType for &'itm [&'itm mut R]
    where
        Self: Ir<Type = &'itm [&'itm mut Extern]>,
    {
        type InputType = &'itm [<R>::RefMutType<'itm>];
        type ReturnType = &'itm [<R>::RefMutType<'itm>];
    }

    impl<'a, R: ReprC> FfiWrapperType for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Robust]>,
    {
        type InputType = Self;
        type ReturnType = Self;
    }
    impl<'slice, R: Transmute> FfiWrapperType for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Transparent]>,
        &'slice mut [<R>::Target]: FfiWrapperType,
        <&'slice mut [<R>::Target] as FfiWrapperType>::InputType: WrapperTypeOf<Self>,
        <&'slice mut [<R>::Target] as FfiWrapperType>::ReturnType: WrapperTypeOf<Self>,
    {
        type InputType =
            <<&'slice mut [<R>::Target] as FfiWrapperType>::InputType as WrapperTypeOf<Self>>::Type;
        type ReturnType =
            <<&'slice mut [<R>::Target] as FfiWrapperType>::ReturnType as WrapperTypeOf<Self>>::Type;
    }
    impl<'itm, R: Ir<Type = S> + FfiWrapperType, S: Cloned> FfiWrapperType for &'itm [R]
    where
        Self: Ir<Type = &'itm [S]>,
    {
        type InputType = &'itm [<R>::InputType];
        type ReturnType = LocalSlice<'itm, <R>::ReturnType>;
    }

    impl<'itm, R: External> FfiWrapperType for &'itm mut [&'itm R]
    where
        Self: Ir<Type = &'itm mut [&'itm Extern]>,
    {
        type InputType = &'itm mut [<R>::RefType<'itm>];
        type ReturnType = &'itm mut [<R>::RefType<'itm>];
    }

    impl<'itm, R: External> FfiWrapperType for &'itm mut [&'itm mut R]
    where
        Self: Ir<Type = &'itm mut [&'itm mut Extern]>,
    {
        type InputType = &'itm mut [<R>::RefMutType<'itm>];
        type ReturnType = &'itm mut [<R>::RefMutType<'itm>];
    }

    impl<R: ReprC> FfiWrapperType for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        type InputType = Self;
        type ReturnType = Self;
    }
    impl<R: Transmute> FfiWrapperType for Box<R>
    where
        Self: Ir<Type = Box<Transparent>>,
        Box<<R>::Target>: FfiWrapperType,
        <Box<<R>::Target> as FfiWrapperType>::InputType: WrapperTypeOf<Self>,
        <Box<<R>::Target> as FfiWrapperType>::ReturnType: WrapperTypeOf<Self>,
    {
        type InputType = <<Box<<R>::Target> as FfiWrapperType>::InputType as WrapperTypeOf<Self>>::Type;
        type ReturnType =
            <<Box<<R>::Target> as FfiWrapperType>::ReturnType as WrapperTypeOf<Self>>::Type;
    }
    impl<R: External> FfiWrapperType for Box<R>
    where
        Self: Ir<Type = Box<Extern>>,
    {
        type InputType = R;
        type ReturnType = R;
    }
    impl<R: Ir<Type = S> + FfiWrapperType, S: Cloned> FfiWrapperType for Box<R>
    where
        Self: Ir<Type = Box<S>>,
    {
        type InputType = Box<<R>::InputType>;
        type ReturnType = Box<<R>::ReturnType>;
    }

    impl<'itm, R: External> FfiWrapperType for Box<&'itm R>
    where
        Self: Ir<Type = Box<&'itm Extern>>,
    {
        type InputType = Box<<R>::RefType<'itm>>;
        type ReturnType = Box<<R>::RefType<'itm>>;
    }

    impl<'itm, R: External> FfiWrapperType for Box<&'itm mut R>
    where
        Self: Ir<Type = Box<&'itm mut Extern>>,
    {
        type InputType = Box<<R>::RefMutType<'itm>>;
        type ReturnType = Box<<R>::RefMutType<'itm>>;
    }

    impl<R: ReprC> FfiWrapperType for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        type InputType = Self;
        type ReturnType = Self;
    }
    impl<R: Transmute> FfiWrapperType for Box<[R]>
    where
        Self: Ir<Type = Box<[Transparent]>>,
        Box<[<R>::Target]>: FfiWrapperType,
        <Box<[<R>::Target]> as FfiWrapperType>::InputType: WrapperTypeOf<Self>,
        <Box<[<R>::Target]> as FfiWrapperType>::ReturnType: WrapperTypeOf<Self>,
    {
        type InputType =
            <<Box<[<R>::Target]> as FfiWrapperType>::InputType as WrapperTypeOf<Self>>::Type;
        type ReturnType =
            <<Box<[<R>::Target]> as FfiWrapperType>::ReturnType as WrapperTypeOf<Self>>::Type;
    }
    impl<R: Ir<Type = S> + FfiWrapperType, S: Cloned> FfiWrapperType for Box<[R]>
    where
        Self: Ir<Type = Box<[S]>>,
    {
        type InputType = Box<[<R>::InputType]>;
        type ReturnType = Box<[<R>::ReturnType]>;
    }

    impl<'itm, R: External> FfiWrapperType for Box<[&'itm R]>
    where
        Self: Ir<Type = Box<[&'itm Extern]>>,
    {
        type InputType = Box<[<R>::RefType<'itm>]>;
        type ReturnType = Box<[<R>::RefType<'itm>]>;
    }

    impl<'itm, R: External> FfiWrapperType for Box<[&'itm mut R]>
    where
        Self: Ir<Type = Box<[&'itm mut Extern]>>,
    {
        type InputType = Box<[<R>::RefMutType<'itm>]>;
        type ReturnType = Box<[<R>::RefMutType<'itm>]>;
    }

    impl<R: ReprC> FfiWrapperType for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        type InputType = Self;
        type ReturnType = Self;
    }
    impl<R: Transmute> FfiWrapperType for Vec<R>
    where
        Self: Ir<Type = Vec<Transparent>>,
        Vec<<R>::Target>: FfiWrapperType,
        <Vec<<R>::Target> as FfiWrapperType>::InputType: WrapperTypeOf<Self>,
        <Vec<<R>::Target> as FfiWrapperType>::ReturnType: WrapperTypeOf<Self>,
    {
        type InputType = <<Vec<<R>::Target> as FfiWrapperType>::InputType as WrapperTypeOf<Self>>::Type;
        type ReturnType =
            <<Vec<<R>::Target> as FfiWrapperType>::ReturnType as WrapperTypeOf<Self>>::Type;
    }
    impl<R: Ir<Type = S> + FfiWrapperType, S: Cloned> FfiWrapperType for Vec<R>
    where
        Self: Ir<Type = Vec<S>>,
    {
        type InputType = Vec<<R>::InputType>;
        type ReturnType = Vec<<R>::ReturnType>;
    }

    impl<'itm, R: External> FfiWrapperType for Vec<&'itm R>
    where
        Self: Ir<Type = Vec<&'itm Extern>>,
    {
        type InputType = Vec<<R>::RefType<'itm>>;
        type ReturnType = Vec<<R>::RefType<'itm>>;
    }

    impl<'itm, R: External> FfiWrapperType for Vec<&'itm mut R>
    where
        Self: Ir<Type = Vec<&'itm mut Extern>>,
    {
        type InputType = Vec<<R>::RefMutType<'itm>>;
        type ReturnType = Vec<<R>::RefMutType<'itm>>;
    }

    impl<R: Ir<Type = S> + FfiWrapperType, S: Cloned, const N: usize> FfiWrapperType for [R; N]
    where
        Self: Ir<Type = [S; N]>,
    {
        type InputType = [<R>::InputType; N];
        type ReturnType = [<R>::ReturnType; N];
    }

    impl<'itm, R: External, const N: usize> FfiWrapperType for [&'itm R; N]
    where
        Self: Ir<Type = [&'itm Extern; N]>,
    {
        type InputType = [<R>::RefType<'itm>; N];
        type ReturnType = [<R>::RefType<'itm>; N];
    }

    impl<'itm, R: External, const N: usize> FfiWrapperType for [&'itm mut R; N]
    where
        Self: Ir<Type = [&'itm mut Extern; N]>,
    {
        type InputType = [<R>::RefMutType<'itm>; N];
        type ReturnType = [<R>::RefMutType<'itm>; N];
    }

    impl<R: FfiWrapperType> FfiWrapperType for Option<R>
    where
        Self: Ir<Type = Option<WithoutNiche>>,
    {
        type InputType = Option<<R>::InputType>;
        type ReturnType = Option<<R>::ReturnType>;
    }
    impl<R: FfiWrapperType> FfiWrapperType for Option<R>
    where
        Self: Ir<Type = Self>,
    {
        type InputType = Option<<R>::InputType>;
        type ReturnType = Option<<R>::ReturnType>;
    }
}

/// Reference that owns its referent. This struct is used when wrapper functions generated by
/// `decarbonate` return types that use store during FFI serialization (e.g. `&(u32, u32)`).
///
/// # Example
///
/// ```
/// #[co3::decarbonate]
/// pub fn func_returns_non_local(a: &u32) -> &u32 {
///    a
/// }
///
/// #[co3::decarbonate]
/// pub fn func_returns_local(a: &(u32, u32)) -> &(u32, u32) {
///    a
/// }
///
/// /* When expanded, `decarbonate` will replace the annotated functions with functions equivalent to the following:
/// pub fn func_returns_non_local(a: &u32) -> &u32 {
///     let mut in_store = ();
///     let a = a.encode(a, &mut in_store);
///
///     let mut output = core::mem::MaybeUninit::uninit();
///     __func_returns_non_local(a, output.as_mut_ptr());
///
///     let out_store = ();
///     let output = output.assume_init();
///
///     // &u32 doesn't reference local scope of a function
///     let output = FfiConvert::decode(output, &mut out_store);
///     output
/// }
///
/// pub fn func(a: &(u32, u32)) -> LocalRef<(u32, u32)> {
///     let mut in_store = Default::default();
///     let a = a.encode(a, &mut in_store);
///
///     let mut output = core::mem::MaybeUninit::uninit();
///     __func(a, output.as_mut_ptr());
///
///     let output = output.assume_init();
///     let out_store = Default::default();
///
///     // &(u32, u32) references out_store which is defined locally
///     let output = FfiConvert::decode(output, &mut out_store);
///     LocalRef(out_store.0, core::marker::PhantomData)
/// } */
/// ```
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct LocalRef<'data, R>(R, core::marker::PhantomData<&'data ()>);

/// Shared slice that owns its referent. This struct is used when wrapper functions generated by
/// `decarbonate` return types that use store during FFI serialization (e.g. `&[(u32, u32)]`).
///
/// # Example
///
/// ```
/// #[co3::decarbonate]
/// pub fn func_returns_non_local(a: &[u32]) -> &[u32] {
///    a
/// }
///
/// #[co3::decarbonate]
/// pub fn func_returns_local(a: &[(u32, u32)]) -> &[(u32, u32)] {
///    a
/// }
///
/// /* When expanded, `decarbonate` will replace the annotated functions with functions equivalent to the following:
/// pub fn func_returns_non_local(a: &[u32]) -> &[u32] {
///     let mut in_store = ();
///     let a = a.encode(a, &mut in_store);
///
///     let mut output = core::mem::MaybeUninit::uninit();
///     __func_returns_non_local(a, output.as_mut_ptr());
///
///     let out_store = ();
///     let output = output.assume_init();
///
///     // &[u32] doesn't reference local scope of a function
///     let output = FfiConvert::decode(output, &mut out_store);
///     output
/// }
///
/// pub fn func_returns_local(a: &[(u32, u32)]) -> LocalSlice<(u32, u32)> {
///     let mut in_store = Default::default();
///     let a = a.encode(a, &mut in_store);
///
///     let mut output = core::mem::MaybeUninit::uninit();
///     __func_returns_local(a, output.as_mut_ptr());
///
///     let output = output.assume_init();
///     let out_store = Default::default();
///
///     // &[(u32, u32)] references out_store which is defined locally
///     let output = FfiConvert::decode(output, &mut out_store);
///     LocalSlice(out_store.0, core::marker::PhantomData)
/// } */
/// ```
#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct LocalSlice<'data, R>(alloc::boxed::Box<[R]>, core::marker::PhantomData<&'data ()>);

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
///         validation_fn={|target: &Self::Target| !target.is_null()},
///         NICHE_VALUE=core::ptr::null_mut(),
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

        // SAFETY: Robust type with a defined C representation by definition
        unsafe impl$(<$($impl_generics $(: $bounds)?),*>)? $crate::transmute::InfallibleTransmute for $ty where Self: $crate::ReprC, $($($where_ty: $where_bound),*)? {}

        impl$(<$($impl_generics $(: $bounds)?),*>)? $crate::option::Ir for $ty where $($($where_ty: $where_bound),*)? {
            type Type = $crate::option::WithoutNiche;
        }

        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::WrapperTypeOf<Self> for $ty where $($($where_ty: $where_bound),*)? {
            type Type = Self;
        }
    };
    (unsafe impl $(<$($impl_generics: tt $(: $bounds: path)?),*>)? Transparent for $ty: ty $(where $($where_ty:ty: $where_bound:path),* )? {
        type Target = $target:ty;

        validation_fn={$validity_fn: expr},
        NICHE_VALUE=$niche_value: expr
        $(,)?
    }) => {
        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::ir::Ir for $ty where $($($where_ty: $where_bound),*)? {
            type Type = $crate::ir::Transparent;
        }

        // SAFETY: `$ty` is transmutable into `$target` and `is_valid` doesn't return false positives
        unsafe impl<$($($impl_generics $(: $bounds)?),*)?> $crate::transmute::Transmute for $ty where $($($where_ty: $where_bound),*)? {
            type Target = $target;

            #[inline]
            fn is_valid(target: &Self::Target) -> bool {
                $validity_fn(target)
            }
        }

        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::option::Niche for $ty where $($($where_ty: $where_bound),*)? {
            const NICHE_VALUE: <$target as $crate::ExternC>::CType = $niche_value;
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

            #[inline]
            fn is_valid(_: &Self::Target) -> bool {
                true
            }
        }

        // SAFETY: `$t` is robust with respect to `$target`
        unsafe impl<$($($impl_generics $(: $bounds)?),*)?> $crate::transmute::InfallibleTransmute for $ty where $($($where_ty: $where_bound),*)? {}

        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::option::Niche for $ty where for<'dummy> $target: $crate::option::Niche, $($($where_ty: $where_bound),*)? {
            const NICHE_VALUE: <$target as $crate::ExternC>::CType = <$target as $crate::option::Niche>::NICHE_VALUE;
        }

        impl<$($($impl_generics $(: $bounds)?),*)?> $crate::WrapperTypeOf<$ty> for $target where $($($where_ty: $where_bound),*)? {
            type Type = $ty;
        }
    };
}

/// Wrapper around struct/enum opaque pointer. When wrapped with the [`co3::extern_type`] macro in
/// the crate linking dynamically to some `cdylib` crate, it replaces struct/enum body definition
#[repr(C)]
pub struct Extern {
    __data: [u8; 0],

    // Required for !Send & !Sync & !Unpin.
    //
    // - `*mut u8` is !Send & !Sync. It's wrapped in `PhantomData` not to affect alignment.
    //
    // - `PhantomPinned` is !Unpin. It's wrapped in `PhantomData` because
    //   its memory representation is not guaranteed to be FFI-safe
    __marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
}

/// Define the correct [`FfiWrapperType::InputType`]/[`FfiWrapperType::ReturnType`] out of
/// the given [`FfiWrapperType::InputType`]/[`FfiWrapperType::ReturnType`]. The only situation
/// when this is evident is when [`Ir::Type`] is set to [`Transparent`] or [`Extern`] types
///
/// Example:
///
/// ```
/// use co3::ExternC;
///
/// #[derive(ExternC)]
/// #[mineral(unsafe(robust))]
/// #[repr(transparent)]
/// pub struct Example(u32);
///
/// /*
/// Due to the fact that implementations of traits for [`Transparent`] structures delegate to the inner
/// type `ExternC`, macro expansion will produce the following impl of [`FfiWrapperType`] for `Example`:
///
/// impl FfiWrapperType for Example {
///     type InputType = u32;
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
impl<R, T> WrapperTypeOf<Box<R>> for Box<T> {
    type Type = Box<R>;
}
impl<R, T> WrapperTypeOf<Vec<R>> for Vec<T> {
    type Type = Vec<R>;
}
impl<R, T, const N: usize> WrapperTypeOf<[R; N]> for [T; N] {
    type Type = [R; N];
}
impl<R, T> WrapperTypeOf<Option<R>> for Option<T> {
    type Type = Option<R>;
}

impl<'itm, R, T> WrapperTypeOf<&'itm R> for LocalRef<'itm, T> {
    type Type = LocalRef<'itm, R>;
}
impl<'slice, R, T> WrapperTypeOf<&'slice [R]> for LocalSlice<'slice, T> {
    type Type = LocalSlice<'slice, R>;
}

// SAFETY: `LocalRef` is transparent and `R` is transmutable into `<R>::Target`
unsafe impl<'itm, R: Transmute> Transmute for LocalRef<'itm, R> {
    type Target = LocalRef<'itm, <R>::Target>;

    #[inline]
    fn is_valid(target: &Self::Target) -> bool {
        <R>::is_valid(&target.0)
    }
}
// SAFETY: `LocalSlice` is transparent and `R` is transmutable into `<R>::Target`
unsafe impl<'itm, R: Transmute> Transmute for LocalSlice<'itm, R> {
    type Target = LocalSlice<'itm, <R>::Target>;

    #[inline]
    fn is_valid(target: &Self::Target) -> bool {
        target.iter().all(|item| <R>::is_valid(item))
    }
}

impl<R> core::ops::Deref for LocalRef<'_, R> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<R> core::ops::Deref for LocalSlice<'_, R> {
    type Target = [R];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
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
    type InputType = ();
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

        impl<'itm, $($ty: $crate::FfiConvert<'itm>),+> $crate::FfiConvert<'itm> for ($($ty,)+) {
            type RustStore = ($( $ty::RustStore, )+);
            type FfiStore = ($( $ty::FfiStore, )+);

            #[expect(non_snake_case)]
            fn encode(self, store: &'itm mut Self::RustStore) -> Self::CType {
                impl_tuple! {@decl_priv_store $($ty),+ for RustStore}

                let ($($ty,)+) = self;
                let store: private_store::Store<$($ty),+> = store.into();
                $ffi_ty($( <$ty as $crate::FfiConvert>::encode($ty, store.$ty),)+)
            }
            #[expect(non_snake_case)]
            unsafe fn decode(source: Self::CType, store: &'itm mut Self::FfiStore) -> Result<Self> {
                impl_tuple! {@decl_priv_store $($ty),+ for FfiStore}

                let $ffi_ty($($ty,)+) = source;
                let store: private_store::Store<$($ty),+> = store.into();
                Ok(unsafe {($( <$ty as $crate::FfiConvert>::decode($ty, store.$ty)?, )+)})
            }
        }

        impl<$($ty),+> $crate::FfiWrapperType for ($($ty,)+) {
            type InputType = Self;
            type ReturnType = Self;
        }
        impl<$($ty),+> $crate::WrapperTypeOf<Self> for ($($ty,)+) {
            type Type = Self;
        }
    };

    // NOTE: This is a trick to index tuples
    ( @decl_priv_store $( $ty:ident ),+ for $store:ident) => {
        mod private_store {
            pub struct Store<'itm, $($ty: $crate::FfiConvert<'itm>),+> {
                $(pub $ty: &'itm mut $ty::$store),+
            }

            impl<'itm, $($ty: $crate::FfiConvert<'itm>),+> From<&'itm mut ($($ty::$store,)+)> for Store<'itm, $($ty,)+> {
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
