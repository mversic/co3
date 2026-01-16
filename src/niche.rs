//! Logic related to the conversion of [`Option<T>`] to and from FFI-compatible representation

use alloc::{boxed::Box, vec::Vec};
use disjoint_impls::disjoint_impls;

use crate::{
    BoxedSliceCType, ExternC, ReprC, VecCType, assert_arr_has_non_zero_len,
    option::COption,
    slice::{CSlice, CSliceMut},
};

/// Marker trait for an [`NicheFamily`] type of a Rust type that has a niche value (stable or custom)
///
/// There are only 2 notable implementations of this trait:
/// 1. [`Transmuted`] types have a single stable (compiler guaranteed) niche value (e.g. `&u32`)
/// 2. [`Cloned`] types have a custom defined (by this crate) niche value (e.g. `[NonZeroU32; 2]`)
pub(crate) trait WithNiche {}

/// Marker for a type that has a single stable (compiler guaranteed) niche value (e.g. `&u32`).
///
/// Only a handful of [`crate::transmute::Transmuted`] types have a stable niche
pub enum WithStableNiche {}

/// Marker for a type that has a custom defined (by this crate) niche (e.g. `[NonZeroU8; 2]`).
pub enum WithCustomNiche {}

/// Marker for a type that has no trap representations and therefore no niche value
pub enum WithoutNiche {}

/// Type that has a trap representation that can be used as a niche value.
///
/// # Example
///
/// [`Option<bool>`]     - will be serilized into one byte
/// [`Option<*const T>`] - will take the size of the pointer
pub trait Niche: ExternC {
    const NICHE_VALUE: Self::CType;
}

/// Type that has a compiler guaranteed [`Niche`] value (e.g. `Box<T>`)
///
/// The stable niche value is made use of when serializing [`Option<T>`].
///
/// # Safety
///
/// - the niche value must be congruent with what is guaranteed by the Rust compiler
pub unsafe trait StableNiche: Niche {}

disjoint_impls! {
    /// Niche kind of the type in the internal representation [IR](`crate::ir::Repr`)
    pub trait NicheFamily {
        /// The internal representation (i.e. type family) of the type
        ///
        /// - If `Self` doesn't have any niche value, set [`NicheFamily::Kind`] to [`WithoutNiche`].
        ///   `Option<T>` will be serialized as [`crate::option::COption`]
        ///
        /// - If `Self` has a compiler guaranteed niche value, set [`NicheFamily::Kind`] to [`WithStableNiche`].
        ///   `Option<T>` will be blindly transmuted into underlying [`ReprC`] type
        ///
        /// - Otherwise, if `Self` has at least one trap, set [`NicheFamily::Kind`] to [`WithCustomNiche`].
        ///   `Option<T>` will be serialized into a [`T::CType`] with a manually set niche value
        type Kind;
    }

    impl<R: NicheFamily<Kind = WithStableNiche>, const N: usize> NicheFamily for [R; N] {
        type Kind = WithCustomNiche;
    }
    impl<R: NicheFamily<Kind = WithCustomNiche>, const N: usize> NicheFamily for [R; N] {
        type Kind = WithCustomNiche;
    }
    impl<R: NicheFamily<Kind = WithoutNiche>, const N: usize> NicheFamily for [R; N] {
        type Kind = WithoutNiche;
    }

    impl<R: NicheFamily<Kind = WithoutNiche>> NicheFamily for Option<R> {
        type Kind = WithCustomNiche;
    }
    impl<R: NicheFamily<Kind = WithStableNiche>> NicheFamily for Option<R> {
        type Kind = WithoutNiche;
    }
    // TODO: It can be either WithoutNiche or WithCustomNiche
    // Depends on: https://github.com/mversic/co3/issues/33
    //impl<R: Ir<Type = WithCustomNiche>> NicheFamily for Option<R> {
    //    type Kind = WithCustomNiche;  // like Option<bool>
    //    type Kind = WithoutNiche;     // like Option<&R>
    //}
}

impl<R> NicheFamily for &R {
    type Kind = WithStableNiche;
}
impl<R> NicheFamily for &mut R {
    type Kind = WithStableNiche;
}
impl<R> NicheFamily for Box<R> {
    type Kind = WithStableNiche;
}
impl<R> NicheFamily for &[R] {
    type Kind = WithCustomNiche;
}
impl<R> NicheFamily for &mut [R] {
    type Kind = WithCustomNiche;
}
#[cfg(feature = "owned_types")]
impl<R> NicheFamily for Box<[R]> {
    type Kind = WithCustomNiche;
}
#[cfg(feature = "owned_types")]
impl<R> NicheFamily for Vec<R> {
    type Kind = WithCustomNiche;
}

impl<R, C> Niche for &R
where
    Self: ExternC<CType = *const C>,
{
    const NICHE_VALUE: *const C = core::ptr::null();
}

impl<R, C> Niche for &mut R
where
    Self: ExternC<CType = *mut C>,
{
    const NICHE_VALUE: *mut C = core::ptr::null_mut();
}

impl<R, C> Niche for Box<R>
where
    Self: ExternC<CType = *mut C>,
{
    const NICHE_VALUE: *mut C = core::ptr::null_mut();
}

impl<R, C> Niche for &[R]
where
    Self: ExternC<CType = CSlice<C>>,
{
    const NICHE_VALUE: CSlice<C> = CSlice::none();
}

impl<R, C> Niche for &mut [R]
where
    Self: ExternC<CType = CSliceMut<C>>,
{
    const NICHE_VALUE: CSliceMut<C> = CSliceMut::none();
}

#[cfg(feature = "owned_types")]
impl<R, C> Niche for Box<[R]>
where
    Self: ExternC<CType = BoxedSliceCType<C>>,
{
    const NICHE_VALUE: Self::CType = BoxedSliceCType::none();
}

#[cfg(feature = "owned_types")]
impl<R, C> Niche for Vec<R>
where
    Self: ExternC<CType = VecCType<C>>,
{
    const NICHE_VALUE: Self::CType = VecCType::none();
}

impl<R: Niche, const N: usize> Niche for [R; N]
where
    Self: ExternC<CType = [R::CType; N]>,
{
    const NICHE_VALUE: [R::CType; N] = {
        assert_arr_has_non_zero_len::<N>();
        [R::NICHE_VALUE; N]
    };
}

impl<R, C: ReprC> Niche for Option<R>
where
    Self: ExternC<CType = COption<C>>,
{
    const NICHE_VALUE: COption<C> = COption::niche();
}

impl Niche for Option<bool> {
    const NICHE_VALUE: Self::CType = 3;
}

unsafe impl<R> StableNiche for &R where Self: Niche {}
unsafe impl<R> StableNiche for &mut R where Self: Niche {}
unsafe impl<R> StableNiche for Box<R> where Self: Niche {}
unsafe impl<R> StableNiche for core::ptr::NonNull<R> {}

impl WithNiche for WithStableNiche {}
impl WithNiche for WithCustomNiche {}
