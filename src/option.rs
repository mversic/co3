//! Logic related to the conversion of [`Option<T>`] to and from FFI-compatible representation

use crate::{ExternC, repr_c::Cloned};

/// Type that has at least one trap representation that can be used as a niche value. The
/// niche value is used in the serialization of [`Option<T>`]. For example, [`Option<bool>`]
/// will be serilized into one byte and [`Option<*const T>`] will take the size of the pointer
// TODO: Lifetime is used as a hack to deal with https://github.com/rust-lang/rust/issues/48214
pub trait Niche: ExternC {
    /// The niche value of the type
    const NICHE_VALUE: Self::CType;
}

/// Marker for a type that doesn't have niche representation
#[derive(Debug, Clone, Copy)]
pub enum WithoutNiche {}

/// Used to implement specialized impls of [`crate::ir::Ir`] for [`Option<T>`]
pub trait Ir {
    /// Internal representation of [`Option<T>`]
    type Type;
}

impl<R> Cloned for Option<R> {}

impl<R, C> Niche for &R
where
    Self: ExternC<CType = *const C>,
{
    const NICHE_VALUE: Self::CType = core::ptr::null();
}

impl<R, C> Niche for &mut R
where
    Self: ExternC<CType = *mut C>,
{
    const NICHE_VALUE: Self::CType = core::ptr::null_mut();
}

impl<R: Niche> Ir for R {
    type Type = Self;
}

impl<R: Ir> crate::ir::Ir for Option<R> {
    type Type = Option<R::Type>;
}
