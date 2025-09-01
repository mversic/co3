#![allow(trivial_casts)]

//! Logic related to the conversion of IR types to equivalent robust C types. Types that are mapped into
//! one of the predefined [`Ir`] types will be provided an automatic implementation of traits in this module.
//!
//! Traits in this module mainly exist to bridge the gap between IR and C type equivalents. User should
//! only implement these traits if none of the predefined IR types provide an adequate mapping.
use alloc::{boxed::Box, vec::Vec};
use core::{mem::ManuallyDrop, ptr::addr_of_mut};

use crate::{
    Extern, FfiConvert, FfiReturn, ReprC, Result,
    ir::{External, Ir, Opaque, Robust, Transparent},
    out_ptr::NonLocal,
    slice::{RefMutSlice, RefSlice},
    transmute::{
        Transmute, transmute_from_target, transmute_from_target_box,
        transmute_from_target_boxed_slice, transmute_from_target_ref_slice,
        transmute_from_target_slice_mut, transmute_from_target_vec, transmute_into_target,
        transmute_into_target_box, transmute_into_target_boxed_slice,
        transmute_into_target_ref_slice, transmute_into_target_slice_mut,
        transmute_into_target_vec,
    },
};

/// Type that cannot be transmuted into a [`ReprC`] type is therefore cloned when
/// converting the likes of `&Self` or `&[Self]` into an FFI-compatible representation
pub trait Cloned {}

impl<R: Ir> Cloned for &R where R::Type: Cloned {}
impl<R> Cloned for Box<R> {}
impl<R> Cloned for &[R] {}
impl<R> Cloned for Vec<R> {}
// TODO: This means there is unnecesary clone?
impl<const N: usize> Cloned for [Opaque; N] {}
impl<R: Ir, const N: usize> Cloned for [R; N] where R::Type: Cloned {}

fn default_init_arr<R: Default, const N: usize>() -> [R; N] {
    let vec = core::iter::repeat_with(Default::default)
        .take(N)
        .collect::<Vec<_>>();

    // SAFETY: Vec<T> length is N
    unsafe { TryFrom::try_from(vec).unwrap_unchecked() }
}

/// Write a rust value into an out-pointer of any type that doesn't return a
/// reference to the store during serialization into an FFI-compatible type
///
/// # Safety
///
/// out-pointer must be valid for writes
pub unsafe fn write_non_local<
    'itm,
    R: NonLocal + CTypeConvert<'itm, S, R::ReprC> + 'itm,
    S: 'itm,
>(
    source: R,
    out_ptr: *mut R::ReprC,
) {
    let mut store = Default::default();

    unsafe {
        // NOTE: Bypasses the erroneous lifetime check.
        // Correct as long as `R::into_repr_c` doesn't return a reference to the store (`R: NonLocal`)
        let store_borrow = &mut *addr_of_mut!(store);
        out_ptr.write(CTypeConvert::into_repr_c(source, store_borrow));
    }
}

/// Read a rust value from an out-pointer of any type that doesn't return a reference
/// reference to the store during serialization from an FFI-compatible type
///
/// # Errors
///
/// Check [`CTypeConvert::try_from_repr_c`]
///
/// # Safety
///
/// Check [`CTypeConvert::try_from_repr_c`]
pub unsafe fn read_non_local<
    'itm,
    R: NonLocal + CTypeConvert<'itm, S, R::ReprC> + 'itm,
    S: 'itm,
>(
    out_ptr: R::ReprC,
) -> Result<R> {
    let mut store = Default::default();

    unsafe {
        // NOTE: Bypasses the erroneous lifetime check.
        // Correct as long as `R::try_from_repr_c` doesn't return a reference to the store (`R: NonLocal`)
        let store_borrow = &mut *addr_of_mut!(store);
        CTypeConvert::try_from_repr_c(out_ptr, store_borrow)
    }
}

/// The trait facilitates conversion of rust types to/from `ReprC` types.
///
/// If a type also implements [`Ir`], i.e. has a defined internal representation, a blanket
/// implementation of [`FfiConvert`] will be provided.
pub trait CTypeConvert<'itm, S, C: ReprC>: Sized {
    /// Type into which state can be stored during conversion from [`Self`]. Useful for
    /// returning owning heap allocated types or non-owning types that are not transmutable.
    /// Serves similar purpose as does context in a closure
    type RustStore: Default;

    /// Type into which state can be stored during conversion into [`Self`]. Useful for
    /// returning non-owning types that are not transmutable. Serves similar purpose as
    /// does context in a closure
    type FfiStore: Default;

    /// Perform the conversion from [`Self`] into `[Self::ReprC]`
    fn into_repr_c(self, store: &'itm mut Self::RustStore) -> C;

    /// Perform the conversion from [`Self::ReprC`] into `[Self]`
    ///
    /// # Errors
    ///
    /// Check [`FfiReturn`]
    ///
    /// # Safety
    ///
    /// All conversions from a pointer must ensure pointer validity beforehand
    unsafe fn try_from_repr_c(source: C, store: &'itm mut Self::FfiStore) -> Result<Self>;
}

impl<'itm, R: CTypeConvert<'itm, S, C> + Clone, S: Cloned, C: ReprC>
    CTypeConvert<'itm, &'itm S, *const C> for &'itm R
{
    type RustStore = (Option<C>, R::RustStore);
    type FfiStore = (Option<R>, R::FfiStore);

    fn into_repr_c(self, store: &'itm mut Self::RustStore) -> *const C {
        store.0.insert(self.clone().into_repr_c(&mut store.1))
    }

    unsafe fn try_from_repr_c(source: *const C, store: &'itm mut Self::FfiStore) -> Result<Self> {
        unsafe {
            if source.as_ref().is_none() {
                return Err(FfiReturn::ArgIsNull);
            }

            Ok(store.0.insert(
                R::try_from_repr_c(source.read(), &mut store.1)
                    .map(ManuallyDrop::new)
                    .map(|item| (*item).clone())?,
            ))
        }
    }
}

impl<'itm, R: CTypeConvert<'itm, S, C> + Clone, S: Cloned, C: ReprC>
    CTypeConvert<'itm, Box<S>, *mut C> for Box<R>
{
    type RustStore = (Option<C>, R::RustStore);
    type FfiStore = R::FfiStore;

    fn into_repr_c(self, store: &'itm mut Self::RustStore) -> *mut C {
        store.0.insert((*self).into_repr_c(&mut store.1))
    }
    unsafe fn try_from_repr_c(source: *mut C, store: &'itm mut Self::FfiStore) -> Result<Self> {
        unsafe {
            if source.as_mut().is_none() {
                return Err(FfiReturn::ArgIsNull);
            }

            R::try_from_repr_c(source.read(), store)
                .map(ManuallyDrop::new)
                .map(|item| (*item).clone())
                .map(Box::new)
        }
    }
}

impl<'itm, R: CTypeConvert<'itm, S, C> + Clone, S: Cloned, C: ReprC>
    CTypeConvert<'itm, Box<[S]>, RefMutSlice<C>> for Box<[R]>
{
    type RustStore = (Box<[C]>, Box<[R::RustStore]>);
    type FfiStore = Box<[R::FfiStore]>;

    fn into_repr_c(self, store: &'itm mut Self::RustStore) -> RefMutSlice<C> {
        let boxed_slice = self;

        store.1 = core::iter::repeat_with(Default::default)
            .take(boxed_slice.len())
            .collect();

        store.0 = Vec::from(boxed_slice)
            .into_iter()
            .zip(&mut *store.1)
            .map(|(item, substore)| item.into_repr_c(substore))
            .collect();

        RefMutSlice::from_slice(Some(&mut store.0))
    }
    unsafe fn try_from_repr_c(
        source: RefMutSlice<C>,
        store: &'itm mut Self::FfiStore,
    ) -> Result<Self> {
        let slice = unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)?;

        *store = core::iter::repeat_with(Default::default)
            .take(slice.len())
            .collect();

        let vec: Box<[_]> = slice
            .iter()
            .copied()
            .zip(&mut **store)
            .map(|(item, substore)| {
                unsafe { R::try_from_repr_c(item, substore) }.map(ManuallyDrop::new)
            })
            .collect::<core::result::Result<_, _>>()?;

        Ok(vec.iter().cloned().map(ManuallyDrop::into_inner).collect())
    }
}

impl<'slice, R: CTypeConvert<'slice, S, C> + Clone, S: Cloned, C: ReprC>
    CTypeConvert<'slice, &'slice [S], RefSlice<C>> for &'slice [R]
{
    type RustStore = (Box<[C]>, Box<[R::RustStore]>);
    type FfiStore = (Box<[R]>, Box<[R::FfiStore]>);

    fn into_repr_c(self, store: &'slice mut Self::RustStore) -> RefSlice<C> {
        let slice = self.to_vec();

        store.1 = core::iter::repeat_with(Default::default)
            .take(slice.len())
            .collect();

        store.0 = slice
            .into_iter()
            .zip(&mut *store.1)
            .map(|(item, substore)| item.into_repr_c(substore))
            .collect();

        RefSlice::from_slice(Some(&store.0))
    }

    unsafe fn try_from_repr_c(
        source: RefSlice<C>,
        store: &'slice mut Self::FfiStore,
    ) -> Result<Self> {
        store.1 = core::iter::repeat_with(Default::default)
            .take(source.len())
            .collect();

        let source: Box<[_]> = unsafe { source.into_rust() }
            .ok_or(FfiReturn::ArgIsNull)?
            .iter()
            .zip(&mut *store.1)
            .map(|(&item, substore)| {
                unsafe { R::try_from_repr_c(item, substore) }.map(ManuallyDrop::new)
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

impl<'itm, R: CTypeConvert<'itm, S, C> + Clone, S: Cloned, C: ReprC>
    CTypeConvert<'itm, Vec<S>, RefMutSlice<C>> for Vec<R>
{
    type RustStore = (Box<[C]>, Box<[R::RustStore]>);
    type FfiStore = Box<[R::FfiStore]>;

    fn into_repr_c(self, store: &'itm mut Self::RustStore) -> RefMutSlice<C> {
        let vec = self;

        store.1 = core::iter::repeat_with(Default::default)
            .take(vec.len())
            .collect();

        store.0 = vec
            .into_iter()
            .zip(&mut *store.1)
            .map(|(item, substore)| item.into_repr_c(substore))
            .collect();

        RefMutSlice::from_slice(Some(&mut store.0))
    }
    unsafe fn try_from_repr_c(
        source: RefMutSlice<C>,
        store: &'itm mut Self::FfiStore,
    ) -> Result<Self> {
        let slice = unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)?;

        *store = core::iter::repeat_with(Default::default)
            .take(slice.len())
            .collect();

        let vec: Box<[_]> = slice
            .iter()
            .copied()
            .zip(&mut **store)
            .map(|(item, substore)| unsafe {
                R::try_from_repr_c(item, substore).map(ManuallyDrop::new)
            })
            .collect::<core::result::Result<_, _>>()?;

        Ok(vec.iter().cloned().map(ManuallyDrop::into_inner).collect())
    }
}

impl<'itm, R: CTypeConvert<'itm, S, C> + Clone, S: Cloned, C: ReprC, const N: usize>
    CTypeConvert<'itm, [S; N], [C; N]> for [R; N]
where
    [R::RustStore; N]: Default,
    [R::FfiStore; N]: Default,
{
    type RustStore = [R::RustStore; N];
    type FfiStore = [R::FfiStore; N];

    fn into_repr_c(self, store: &'itm mut Self::RustStore) -> [C; N] {
        *store = default_init_arr();

        let array = self
            .into_iter()
            .zip(store.iter_mut())
            .map(|(item, substore)| item.into_repr_c(substore))
            .collect::<Vec<_>>()
            .try_into();

        // SAFETY: Vec<T> length is N
        unsafe { array.unwrap_unchecked() }
    }
    unsafe fn try_from_repr_c(source: [C; N], store: &'itm mut Self::FfiStore) -> Result<Self> {
        let vec: core::result::Result<[_; N], _> = source
            .into_iter()
            .zip(store.iter_mut())
            .map(|(item, substore)| unsafe {
                R::try_from_repr_c(item, substore).map(ManuallyDrop::new)
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
impl<'itm, R: CTypeConvert<'itm, S, C> + Clone, S: Cloned, C: ReprC, const N: usize>
    CTypeConvert<'itm, [S; N], *mut [C; N]> for [R; N]
where
    [R::RustStore; N]: Default,
    [R::FfiStore; N]: Default,
    [C; N]: Default,
{
    type RustStore = ([C; N], [R::RustStore; N]);
    type FfiStore = [R::FfiStore; N];

    fn into_repr_c(self, store: &'itm mut Self::RustStore) -> *mut [C; N] {
        store.0 = self.into_repr_c(&mut store.1);
        &mut store.0
    }
    unsafe fn try_from_repr_c(
        source: *mut [C; N],
        store: &'itm mut Self::FfiStore,
    ) -> Result<Self> {
        if source.is_null() {
            return Err(FfiReturn::ArgIsNull);
        }

        unsafe { Self::try_from_repr_c(source.read(), store) }
    }
}
impl<R: ReprC> CTypeConvert<'_, Robust, R> for R {
    type RustStore = ();
    type FfiStore = ();

    fn into_repr_c(self, (): &mut ()) -> R {
        self
    }

    unsafe fn try_from_repr_c(source: R, (): &mut ()) -> Result<Self> {
        Ok(source)
    }
}
impl<R: ReprC, const N: usize> CTypeConvert<'_, Robust, *mut [R; N]> for [R; N] {
    type RustStore = Option<Self>;
    type FfiStore = ();

    fn into_repr_c(self, store: &mut Self::RustStore) -> *mut [R; N] {
        store.insert(self)
    }

    unsafe fn try_from_repr_c(source: *mut [R; N], (): &mut ()) -> Result<Self> {
        if source.is_null() {
            return Err(FfiReturn::ArgIsNull);
        }

        Ok(unsafe { source.read() })
    }
}
impl<R: ReprC> CTypeConvert<'_, Box<Robust>, *mut R> for Box<R> {
    type RustStore = Option<Self>;
    type FfiStore = ();

    fn into_repr_c(self, store: &mut Self::RustStore) -> *mut R {
        &mut **store.insert(self)
    }

    unsafe fn try_from_repr_c(source: *mut R, (): &mut ()) -> Result<Self> {
        if source.is_null() {
            return Err(FfiReturn::ArgIsNull);
        }

        Ok(Box::new(unsafe { source.read() }))
    }
}
impl<R: ReprC> CTypeConvert<'_, Box<[Robust]>, RefMutSlice<R>> for Box<[R]> {
    type RustStore = Self;
    type FfiStore = ();

    fn into_repr_c(self, store: &mut Self::RustStore) -> RefMutSlice<R> {
        *store = self;
        RefMutSlice::from_slice(Some(store))
    }

    unsafe fn try_from_repr_c(source: RefMutSlice<R>, (): &mut ()) -> Result<Self> {
        unsafe { source.into_rust() }
            .ok_or(FfiReturn::ArgIsNull)
            .map(|slice| (&*slice).into())
    }
}
impl<R: ReprC> CTypeConvert<'_, &[Robust], RefSlice<R>> for &[R] {
    type RustStore = ();
    type FfiStore = ();

    fn into_repr_c(self, (): &mut ()) -> RefSlice<R> {
        RefSlice::from_slice(Some(self))
    }

    unsafe fn try_from_repr_c(source: RefSlice<R>, (): &mut ()) -> Result<Self> {
        unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)
    }
}
impl<R: ReprC> CTypeConvert<'_, &mut [Robust], RefMutSlice<R>> for &mut [R] {
    type RustStore = ();
    type FfiStore = ();

    fn into_repr_c(self, (): &mut ()) -> RefMutSlice<R> {
        RefMutSlice::from_slice(Some(self))
    }

    unsafe fn try_from_repr_c(source: RefMutSlice<R>, (): &mut ()) -> Result<Self> {
        unsafe { source.into_rust() }.ok_or(FfiReturn::ArgIsNull)
    }
}

impl<R: ReprC> CTypeConvert<'_, Vec<Robust>, RefMutSlice<R>> for Vec<R> {
    type RustStore = Box<[R]>;
    type FfiStore = ();

    fn into_repr_c(self, store: &mut Self::RustStore) -> RefMutSlice<R> {
        *store = self.into_boxed_slice();
        RefMutSlice::from_slice(Some(store))
    }

    unsafe fn try_from_repr_c(source: RefMutSlice<R>, (): &mut ()) -> Result<Self> {
        unsafe { source.into_rust() }
            .ok_or(FfiReturn::ArgIsNull)
            .map(|slice| slice.to_vec())
    }
}
impl<R> CTypeConvert<'_, Opaque, *mut R> for R {
    type RustStore = ();
    type FfiStore = ();

    fn into_repr_c(self, (): &mut ()) -> *mut R {
        Box::into_raw(Box::new(self))
    }
    unsafe fn try_from_repr_c(source: *mut R, (): &mut ()) -> Result<Self> {
        if source.is_null() {
            return Err(FfiReturn::ArgIsNull);
        }

        Ok(*unsafe { Box::from_raw(source) })
    }
}

impl<R> CTypeConvert<'_, Box<Opaque>, *mut R> for Box<R> {
    type RustStore = ();
    type FfiStore = ();

    fn into_repr_c(self, (): &mut ()) -> *mut R {
        Box::into_raw(self)
    }

    unsafe fn try_from_repr_c(source: *mut R, (): &mut ()) -> Result<Self> {
        if source.is_null() {
            return Err(FfiReturn::ArgIsNull);
        }

        Ok(unsafe { Box::from_raw(source) })
    }
}

impl<R> CTypeConvert<'_, Box<[Opaque]>, RefMutSlice<*mut R>> for Box<[R]> {
    type RustStore = Box<[*mut R]>;
    type FfiStore = ();

    fn into_repr_c(self, store: &mut Self::RustStore) -> RefMutSlice<*mut R> {
        *store = Vec::from(self)
            .into_iter()
            .map(Box::new)
            .map(Box::into_raw)
            .collect();

        RefMutSlice::from_slice(Some(store))
    }

    unsafe fn try_from_repr_c(source: RefMutSlice<*mut R>, (): &mut ()) -> Result<Self> {
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

impl<'slice, R: Clone> CTypeConvert<'slice, &'slice [Opaque], RefSlice<*const R>>
    for &'slice [R]
{
    type RustStore = Box<[*const R]>;
    type FfiStore = Box<[R]>;

    fn into_repr_c(self, store: &mut Self::RustStore) -> RefSlice<*const R> {
        *store = self.iter().map(core::ptr::from_ref).collect();
        RefSlice::from_slice(Some(store))
    }

    unsafe fn try_from_repr_c(
        source: RefSlice<*const R>,
        store: &'slice mut Self::FfiStore,
    ) -> Result<Self> {
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
impl<'slice, R: Clone> CTypeConvert<'slice, &mut [Opaque], RefMutSlice<*mut R>>
    for &'slice mut [R]
{
    type RustStore = Box<[*mut R]>;
    type FfiStore = Box<[R]>;

    fn into_repr_c(self, store: &mut Self::RustStore) -> RefMutSlice<*mut R> {
        *store = self.iter_mut().map(core::ptr::from_mut).collect();
        RefMutSlice::from_slice(Some(store))
    }

    unsafe fn try_from_repr_c(
        source: RefMutSlice<*mut R>,
        store: &'slice mut Self::FfiStore,
    ) -> Result<Self> {
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

impl<R> CTypeConvert<'_, Vec<Opaque>, RefMutSlice<*mut R>> for Vec<R> {
    type RustStore = Box<[*mut R]>;
    type FfiStore = ();

    fn into_repr_c(self, store: &mut Self::RustStore) -> RefMutSlice<*mut R> {
        *store = self.into_iter().map(Box::new).map(Box::into_raw).collect();
        RefMutSlice::from_slice(Some(store))
    }

    unsafe fn try_from_repr_c(source: RefMutSlice<*mut R>, (): &mut ()) -> Result<Self> {
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

impl<R, const N: usize> CTypeConvert<'_, [Opaque; N], [*mut R; N]> for [R; N] {
    type RustStore = ();
    type FfiStore = ();

    fn into_repr_c(self, (): &mut Self::RustStore) -> [*mut R; N] {
        let array = self
            .into_iter()
            .map(Box::new)
            .map(Box::into_raw)
            .collect::<Vec<_>>()
            .try_into();

        // SAFETY: Vec<T> length is N
        unsafe { array.unwrap_unchecked() }
    }

    unsafe fn try_from_repr_c(source: [*mut R; N], (): &mut ()) -> Result<Self> {
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
impl<R, const N: usize> CTypeConvert<'_, [Opaque; N], *mut [*mut R; N]> for [R; N] {
    type RustStore = Option<[*mut R; N]>;
    type FfiStore = ();

    fn into_repr_c(self, store: &mut Self::RustStore) -> *mut [*mut R; N] {
        store.insert(self.into_repr_c(&mut ()))
    }

    unsafe fn try_from_repr_c(source: *mut [*mut R; N], (): &mut ()) -> Result<Self> {
        if source.is_null() {
            return Err(FfiReturn::ArgIsNull);
        }

        unsafe { CTypeConvert::try_from_repr_c(source.read(), &mut ()) }
    }
}

impl<R: External> CTypeConvert<'_, Box<Extern>, *mut Extern> for Box<R> {
    type RustStore = ();
    type FfiStore = ();

    fn into_repr_c(self, (): &mut ()) -> *mut Extern {
        ManuallyDrop::new(*self).as_extern_ptr_mut()
    }

    unsafe fn try_from_repr_c(source: *mut Extern, (): &mut ()) -> Result<Self> {
        if source.is_null() {
            return Err(FfiReturn::ArgIsNull);
        }

        Ok(Box::new(unsafe { External::from_extern_ptr(source) }))
    }
}

impl<'itm, R: Transmute, C: ReprC> CTypeConvert<'itm, Transparent, C> for R
where
    R::Target: FfiConvert<'itm, C>,
{
    type RustStore = <R::Target as FfiConvert<'itm, C>>::RustStore;
    type FfiStore = <R::Target as FfiConvert<'itm, C>>::FfiStore;

    fn into_repr_c(self, store: &'itm mut Self::RustStore) -> C {
        transmute_into_target(self).into_ffi(store)
    }

    unsafe fn try_from_repr_c(source: C, store: &'itm mut Self::FfiStore) -> Result<Self> {
        unsafe {
            FfiConvert::try_from_ffi(source, store).and_then(|inner| transmute_from_target(inner))
        }
    }
}

impl<'itm, R: Transmute, C: ReprC> CTypeConvert<'itm, Box<Transparent>, C> for Box<R>
where
    Box<R::Target>: FfiConvert<'itm, C>,
{
    type RustStore = <Box<R::Target> as FfiConvert<'itm, C>>::RustStore;
    type FfiStore = <Box<R::Target> as FfiConvert<'itm, C>>::FfiStore;

    fn into_repr_c(self, store: &'itm mut Self::RustStore) -> C {
        transmute_into_target_box(self).into_ffi(store)
    }

    unsafe fn try_from_repr_c(source: C, store: &'itm mut Self::FfiStore) -> Result<Self> {
        unsafe {
            Box::<R::Target>::try_from_ffi(source, store)
                .and_then(|output| transmute_from_target_box(output))
        }
    }
}

impl<'itm, R: Transmute, C: ReprC> CTypeConvert<'itm, Box<[Transparent]>, C> for Box<[R]>
where
    Box<[R::Target]>: FfiConvert<'itm, C>,
{
    type RustStore = <Box<[R::Target]> as FfiConvert<'itm, C>>::RustStore;
    type FfiStore = <Box<[R::Target]> as FfiConvert<'itm, C>>::FfiStore;

    fn into_repr_c(self, store: &'itm mut Self::RustStore) -> C {
        transmute_into_target_boxed_slice(self).into_ffi(store)
    }

    unsafe fn try_from_repr_c(source: C, store: &'itm mut Self::FfiStore) -> Result<Self> {
        unsafe {
            <Box<[R::Target]>>::try_from_ffi(source, store)
                .and_then(|output| transmute_from_target_boxed_slice(output))
        }
    }
}

impl<'slice, R: Transmute, C: ReprC> CTypeConvert<'slice, &'slice [Transparent], C>
    for &'slice [R]
where
    &'slice [R::Target]: FfiConvert<'slice, C>,
{
    type RustStore = <&'slice [R::Target] as FfiConvert<'slice, C>>::RustStore;
    type FfiStore = <&'slice [R::Target] as FfiConvert<'slice, C>>::FfiStore;

    fn into_repr_c(self, store: &'slice mut Self::RustStore) -> C {
        transmute_into_target_ref_slice(self).into_ffi(store)
    }

    unsafe fn try_from_repr_c(source: C, store: &'slice mut Self::FfiStore) -> Result<Self> {
        unsafe {
            let slice = <&[R::Target]>::try_from_ffi(source, store)?;
            transmute_from_target_ref_slice(slice)
        }
    }
}

impl<'slice, R: Transmute, C: ReprC> CTypeConvert<'slice, &'slice mut [Transparent], C>
    for &'slice mut [R]
where
    &'slice mut [R::Target]: FfiConvert<'slice, C>,
{
    type RustStore = <&'slice mut [R::Target] as FfiConvert<'slice, C>>::RustStore;
    type FfiStore = <&'slice mut [R::Target] as FfiConvert<'slice, C>>::FfiStore;

    fn into_repr_c(self, store: &'slice mut Self::RustStore) -> C {
        transmute_into_target_slice_mut(self).into_ffi(store)
    }

    unsafe fn try_from_repr_c(source: C, store: &'slice mut Self::FfiStore) -> Result<Self> {
        unsafe {
            <&mut [R::Target]>::try_from_ffi(source, store)
                .and_then(|output| transmute_from_target_slice_mut(output))
        }
    }
}

impl<'itm, R: Transmute, C: ReprC> CTypeConvert<'itm, Vec<Transparent>, C> for Vec<R>
where
    Vec<R::Target>: FfiConvert<'itm, C>,
{
    type RustStore = <Vec<R::Target> as FfiConvert<'itm, C>>::RustStore;
    type FfiStore = <Vec<R::Target> as FfiConvert<'itm, C>>::FfiStore;

    fn into_repr_c(self, store: &'itm mut Self::RustStore) -> C {
        transmute_into_target_vec(self).into_ffi(store)
    }

    unsafe fn try_from_repr_c(source: C, store: &'itm mut Self::FfiStore) -> Result<Self> {
        unsafe {
            <Vec<R::Target>>::try_from_ffi(source, store)
                .and_then(|output| transmute_from_target_vec(output))
        }
    }
}
