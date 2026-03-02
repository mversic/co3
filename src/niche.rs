//! Logic related to the conversion of [`Option<T>`] to and from FFI-compatible representation

#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};

use disjoint_impls::disjoint_impls;

use crate::{
    ExternC, assert_arr_has_non_zero_len,
    option::COption,
    slice::{CSlice, CSliceMut},
};
#[cfg(feature = "alloc")]
use crate::{
    boxed::{CBox, CBoxedSlice},
    vec::CVec,
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

// FIXME: Should we make this trait unsafe? Because if bool is marked as WithoutNiche, `&mut bool` will be transmuted and may produce UB
disjoint_impls! {
    /// Niche kind of the type in the internal representation [IR](`crate::ir::Repr`)
    ///
    /// # Safety
    ///
    /// - if the type has `ReprFamily<Kind = Robust>` it must not be incorrectly marked as `WithoutNiche`
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
    impl<R: NicheFamily<Kind = WithCustomNiche>> NicheFamily for Option<R> where Self: Niche {
        type Kind = WithCustomNiche;
    }
    // TODO: IMHO compiler should be able to resolve circular dependencies here, but it doesn't work for now so I've bounded previous with Niche
    // This issue could be of some help: https://github.com/mversic/co3/issues/33. This seems to be a limitation of the compiler known as
    // circular/cyclic resolution or (co)inductive cycle. The case shown here creates a cycle but only one solution is possible afaik
    //impl<R: NicheFamily<Kind = WithCustomNiche>> NicheFamily for Option<R> where Option<Self>: ReprFamily<Kind = Option<WithCustomNiche>> {
    //    type Kind = WithCustomNiche;
    //}
    //impl<R: NicheFamily<Kind = WithCustomNiche>> NicheFamily for Option<R> where Option<Self>: ReprFamily<Kind = Option<WithoutNiche>> {
    //    type Kind = WithoutNiche;
    //}
}

impl<R> NicheFamily for &R {
    type Kind = WithStableNiche;
}
impl<R> NicheFamily for &mut R {
    type Kind = WithStableNiche;
}
#[cfg(feature = "alloc")]
impl<R> NicheFamily for Box<R> {
    type Kind = WithStableNiche;
}
impl<R> NicheFamily for &[R] {
    type Kind = WithCustomNiche;
}
impl<R> NicheFamily for &mut [R] {
    type Kind = WithCustomNiche;
}
#[cfg(feature = "alloc")]
impl<R> NicheFamily for Box<[R]> {
    type Kind = WithCustomNiche;
}
#[cfg(feature = "alloc")]
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

#[cfg(feature = "alloc")]
impl<R, C> Niche for Box<R>
where
    Self: ExternC<CType = CBox<C>>,
{
    const NICHE_VALUE: CBox<C> = CBox::none();
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

#[cfg(feature = "alloc")]
impl<R, C> Niche for Box<[R]>
where
    Self: ExternC<CType = CBoxedSlice<C>>,
{
    const NICHE_VALUE: Self::CType = CBoxedSlice::none();
}

#[cfg(feature = "alloc")]
impl<R, C> Niche for Vec<R>
where
    Self: ExternC<CType = CVec<C>>,
{
    const NICHE_VALUE: Self::CType = CVec::none();
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

impl<R, C> Niche for Option<R>
where
    Self: ExternC<CType = COption<C>>,
{
    const NICHE_VALUE: COption<C> = COption::niche();
}

// TODO: Depends on: https://github.com/mversic/co3/issues/33
impl Niche for Option<bool> {
    const NICHE_VALUE: Self::CType = 3;
}
impl Niche for Option<Option<bool>> {
    const NICHE_VALUE: Self::CType = 4;
}

unsafe impl<R> StableNiche for &R where Self: Niche {}
unsafe impl<R> StableNiche for &mut R where Self: Niche {}
#[cfg(feature = "alloc")]
unsafe impl<R> StableNiche for Box<R> where Self: Niche {}
unsafe impl<R> StableNiche for core::ptr::NonNull<R> {}

impl WithNiche for WithStableNiche {}
impl WithNiche for WithCustomNiche {}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc_crate::string::String;
    use core::{mem::ManuallyDrop, ptr::NonNull};

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    use crate::{Decode, Encode, ReprC, ir::ReprFamily, slice::CSlice};

    #[test]
    fn nested_option_niche_family() {
        assert_impl_all!(Option<bool>:
            ReprFamily<Kind = Option<WithCustomNiche>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = u8>,
            Decode<'static>,
            Encode
        );
        assert_impl_all!(Option<Option<bool>>:
            NicheFamily<Kind = WithCustomNiche>,
            ReprFamily<Kind = Option<WithCustomNiche>>,
            Niche<CType = u8>,
            Decode<'static>,
            Encode
        );
        // TODO: Depends on: https://github.com/mversic/co3/issues/33
        //assert_impl_all!(Option<(u8, NonZeroU8)>: NicheFamily<Kind = WithoutNiche>, ReprFamily<Kind = Option<WithoutNiche>>, ExternC<CType = CTuple2<u8, u8>>);

        assert_not_impl_any!(Option<bool>: ReprC, StableNiche);
        assert_not_impl_any!(Option<Option<bool>>: ReprC, StableNiche);
    }

    #[test]
    fn niche_values() {
        assert_eq!(core::ptr::null::<u8>(), None::<&bool>.encode(&mut ()));
        #[cfg(any(
            feature = "unstable-refs",
            all(feature = "alloc", feature = "unsafe-optimizations")
        ))]
        assert_eq!(core::ptr::null::<u8>(), None::<&mut bool>.encode(&mut ()));

        #[cfg(feature = "alloc")]
        assert_eq!(
            CVec::<u8>::none(),
            None::<String>.encode(&mut Default::default())
        );
        #[cfg(feature = "alloc")]
        assert_eq!(
            CBoxedSlice::<u8>::none(),
            None::<Box<str>>.encode(&mut Default::default())
        );

        assert_eq!(CSlice::<u8>::none(), None::<&str>.encode(&mut ()));

        #[cfg(any(
            feature = "unstable-refs",
            all(feature = "alloc", feature = "unsafe-optimizations")
        ))]
        assert_eq!(
            co3::slice::CSliceMut::<u8>::none(),
            None::<&mut str>.encode(&mut ())
        );
        #[cfg(feature = "alloc")]
        assert_eq!(
            core::ptr::null_mut(),
            None::<NonNull<String>>.encode(&mut ())
        );
        #[cfg(feature = "alloc")]
        assert_eq!(
            CVec::<u8>::none(),
            None::<ManuallyDrop<String>>.encode(&mut Default::default())
        );

        assert_eq!(2_u8, None::<ManuallyDrop<bool>>.encode(&mut ()));
    }
}
