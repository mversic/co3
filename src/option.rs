//! Logic related to the conversion of [`Option<T>`] to and from FFI-compatible representation

use crate::{
    FfiConvert, FfiReturn, FfiTuple2, FfiType, ReprC, Result,
    repr_c::{CTypeConvert, Cloned},
};

/// Type that has at least one trap representation that can be used as a niche value. The
/// niche value is used in the serialization of [`Option<T>`]. For example, [`Option<bool>`]
/// will be serilized into one byte and [`Option<*const T>`] will take the size of the pointer
// TODO: Lifetime is used as a hack to deal with https://github.com/rust-lang/rust/issues/48214
pub trait Niche<'dummy>: FfiType {
    /// The niche value of the type
    const NICHE_VALUE: Self::ReprC;
}

/// Marker for a type that doesn't have niche representation
#[derive(Debug, Clone, Copy)]
pub enum WithoutNiche {}

/// Used to implement specialized impls of [`crate::ir::Ir`] for [`Option<T>`]
pub trait Ir {
    /// Internal representation of [`Option<T>`]
    type Type;
}

// TODO: Are they all cloned?
impl<R> Cloned for Option<R> {}

impl<R, C> Niche<'_> for &R
where
    Self: FfiType<ReprC = *const C>,
{
    const NICHE_VALUE: Self::ReprC = core::ptr::null();
}

impl<R, C> Niche<'_> for &mut R
where
    Self: FfiType<ReprC = *mut C>,
{
    const NICHE_VALUE: Self::ReprC = core::ptr::null_mut();
}

impl<'dummy, R: Niche<'dummy>> Ir for R {
    type Type = Self;
}

impl<R: Ir> crate::ir::Ir for Option<R> {
    type Type = Option<R::Type>;
}

impl<'itm, R: FfiConvert<'itm, C>, C: ReprC>
    CTypeConvert<'itm, Option<WithoutNiche>, FfiTuple2<<u8 as FfiType>::ReprC, C>> for Option<R>
{
    type RustStore = R::RustStore;
    type FfiStore = R::FfiStore;

    fn into_repr_c(self, store: &'itm mut Self::RustStore) -> FfiTuple2<<u8 as FfiType>::ReprC, C> {
        match self {
            // TODO: No need to zero the memory because it must never be read
            None => FfiTuple2(0u8.into_ffi(&mut ()), unsafe { core::mem::zeroed() }),
            Some(value) => FfiTuple2(1u8.into_ffi(&mut ()), value.into_ffi(store)),
        }
    }

    unsafe fn try_from_repr_c(
        source: FfiTuple2<<u8 as FfiType>::ReprC, C>,
        store: &'itm mut Self::FfiStore,
    ) -> Result<Self> {
        match unsafe { u8::try_from_ffi(source.0, &mut ()) }? {
            0 => Ok(None),
            1 => Ok(Some(unsafe { R::try_from_ffi(source.1, store) }?)),
            _ => Err(FfiReturn::TrapRepresentation),
        }
    }
}
impl<'dummy, 'itm, R: Niche<'dummy, ReprC = C> + FfiConvert<'itm, C>, C: ReprC>
    CTypeConvert<'itm, Self, C> for Option<R>
where
    R::ReprC: PartialEq,
{
    type RustStore = R::RustStore;
    type FfiStore = R::FfiStore;

    fn into_repr_c(self, store: &'itm mut Self::RustStore) -> C {
        if let Some(value) = self {
            return value.into_ffi(store);
        }

        R::NICHE_VALUE
    }

    unsafe fn try_from_repr_c(source: C, store: &'itm mut Self::FfiStore) -> Result<Self> {
        if source == R::NICHE_VALUE {
            return Ok(None);
        }

        Ok(Some(unsafe { R::try_from_ffi(source, store) }?))
    }
}
