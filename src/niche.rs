//! Logic related to the conversion of [`Option<T>`] to and from FFI-compatible representation

use core::ops::Add;

#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};

use disjoint_impls::disjoint_impls;

#[cfg(feature = "alloc")]
use crate::boxed::{CBox, CBoxedSlice};
use crate::{
    ExternC, assert_arr_has_non_zero_len,
    dst::{DstFamily, ExternTypeLike, Sized_, UnSized},
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

disjoint_impls! {
    /// Type that has a trap representation that can be used as a niche value.
    ///
    /// # Example
    ///
    /// [`Option<bool>`]     - will be serilized into one byte
    /// [`Option<*const T>`] - will take the size of the pointer
    pub trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<R, C> Niche for &R
    where
        Self: ExternC<CType = *const C>,
    {
        const NICHE_VALUE: Self::CType = core::ptr::null();
    }
    impl<R: ?Sized, C> Niche for &R
    where
        Self: ExternC<CType = CSlice<C>>,
    {
        const NICHE_VALUE: Self::CType = CSlice::none();
    }

    impl<R, C> Niche for &mut R
    where
        Self: ExternC<CType = *mut C>,
    {
        const NICHE_VALUE: Self::CType = core::ptr::null_mut();
    }
    impl<R: ?Sized, C> Niche for &mut R
    where
        Self: ExternC<CType = CSliceMut<C>>,
    {
        const NICHE_VALUE: Self::CType = CSliceMut::none();
    }

    #[cfg(feature = "alloc")]
    impl<R, C> Niche for Box<R>
    where
        Self: ExternC<CType = CBox<C>>,
    {
        const NICHE_VALUE: Self::CType = CBox::none();
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C> Niche for Box<R>
    where
        Self: ExternC<CType = CBoxedSlice<C>>,
    {
        const NICHE_VALUE: Self::CType = CBoxedSlice::none();
    }
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
    ///
    /// # Safety
    ///
    /// - if the type has `ReprFamily<Kind = Robust>` it must not be incorrectly marked as `WithoutNiche`
    // FIXME: Should we make this trait unsafe? Because if bool is marked as WithoutNiche, `&mut bool` will be transmuted and may produce UB
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

    impl<R: DstFamily<Kind = Sized_>> NicheFamily for &R {
        type Kind = WithStableNiche;
    }
    impl<R: DstFamily<Kind: UnSized> + ?Sized> NicheFamily for &R {
        type Kind = WithCustomNiche;
    }
    impl<R: DstFamily<Kind = ExternTypeLike>> NicheFamily for &R {
        type Kind = WithCustomNiche;
    }

    impl<R: DstFamily<Kind = Sized_>> NicheFamily for &mut R {
        type Kind = WithStableNiche;
    }
    impl<R: DstFamily<Kind: UnSized> + ?Sized> NicheFamily for &mut R {
        type Kind = WithCustomNiche;
    }
    impl<R: DstFamily<Kind = ExternTypeLike>> NicheFamily for &mut R {
        type Kind = WithCustomNiche;
    }

    #[cfg(feature = "alloc")]
    impl<R: DstFamily<Kind = Sized_>> NicheFamily for Box<R> {
        type Kind = WithStableNiche;
    }
    #[cfg(feature = "alloc")]
    impl<R: DstFamily<Kind: UnSized> + ?Sized> NicheFamily for Box<R> {
        type Kind = WithCustomNiche;
    }
    #[cfg(feature = "alloc")]
    impl<R: DstFamily<Kind = ExternTypeLike>> NicheFamily for Box<R> {
        type Kind = WithCustomNiche;
    }

    impl<R: NicheFamily<Kind = WithoutNiche>, const N: usize> NicheFamily for [R; N] {
        type Kind = WithoutNiche;
    }
    impl<R: NicheFamily<Kind = WithStableNiche>, const N: usize> NicheFamily for [R; N] {
        type Kind = WithCustomNiche;
    }
    impl<R: NicheFamily<Kind = WithCustomNiche>, const N: usize> NicheFamily for [R; N] {
        type Kind = WithCustomNiche;
    }

    impl<R: NicheFamily<Kind = WithoutNiche>> NicheFamily for Option<R> {
        type Kind = WithCustomNiche;
    }
    impl<R: NicheFamily<Kind = WithStableNiche>> NicheFamily for Option<R> {
        type Kind = WithoutNiche;
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
    impl<R: NicheFamily<Kind = WithCustomNiche>> NicheFamily for Option<R> where Self: Niche {
        type Kind = WithCustomNiche;
    }
}

impl<R> NicheFamily for [R] {
    type Kind = WithCustomNiche;
}

#[cfg(feature = "alloc")]
impl<R> NicheFamily for Vec<R> {
    type Kind = WithCustomNiche;
}

#[cfg(feature = "alloc")]
impl<R, C> Niche for Vec<R>
where
    Self: ExternC<CType = CBoxedSlice<C>>,
{
    const NICHE_VALUE: Self::CType = CBoxedSlice::none();
}

impl<R: Niche, const N: usize> Niche for [R; N]
where
    Self: ExternC<CType = [R::CType; N]>,
{
    const NICHE_VALUE: Self::CType = {
        assert_arr_has_non_zero_len::<N>();
        [R::NICHE_VALUE; N]
    };
}

impl<R, C> Niche for Option<R>
where
    Self: ExternC<CType = COption<C>>,
{
    const NICHE_VALUE: Self::CType = COption::none();
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

impl Add for WithoutNiche {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}
impl<T: WithNiche> Add<T> for WithoutNiche {
    type Output = WithCustomNiche;

    fn add(self, _: T) -> Self::Output {
        unreachable!()
    }
}
impl Add<WithoutNiche> for WithCustomNiche {
    type Output = Self;

    fn add(self, _: WithoutNiche) -> Self::Output {
        unreachable!()
    }
}
impl Add<WithoutNiche> for WithStableNiche {
    type Output = WithCustomNiche;

    fn add(self, _: WithoutNiche) -> Self::Output {
        unreachable!()
    }
}
impl Add<WithCustomNiche> for WithStableNiche {
    type Output = WithCustomNiche;

    fn add(self, _: WithCustomNiche) -> Self::Output {
        unreachable!()
    }
}
impl Add for WithStableNiche {
    type Output = WithCustomNiche;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}
impl Add<WithStableNiche> for WithCustomNiche {
    type Output = Self;

    fn add(self, _: WithStableNiche) -> Self::Output {
        unreachable!()
    }
}
impl Add for WithCustomNiche {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc_crate::string::String;
    use core::{mem::ManuallyDrop, ptr::NonNull};

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    use crate::{DecodeWithStore, EncodeWithStore, ReprC, ir::ReprFamily, slice::CSlice};

    #[test]
    fn nested_option_niche_family() {
        assert_impl_all!(Option<bool>:
            ReprFamily<Kind = Option<WithCustomNiche>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = u8>,
            DecodeWithStore<'static>,

            EncodeWithStore,

        );
        assert_impl_all!(Option<Option<bool>>:
            NicheFamily<Kind = WithCustomNiche>,
            ReprFamily<Kind = Option<WithCustomNiche>>,
            Niche<CType = u8>,
            DecodeWithStore<'static>,

            EncodeWithStore,

        );
        // TODO: Depends on: https://github.com/mversic/co3/issues/33
        //assert_impl_all!(Option<(u8, NonZeroU8)>:
        //    NicheFamily<Kind = WithoutNiche>,
        //    ReprFamily<Kind = Option<WithoutNiche>>,
        //    ExternC<CType = CTuple2<u8, u8>>
        //);

        assert_not_impl_any!(Option<bool>: ReprC);
        assert_not_impl_any!(Option<Option<bool>>: ReprC);
    }

    #[test]
    fn niche_values() {
        assert_eq!(core::ptr::null::<u8>(), None::<&bool>.encode(&mut ()));
        #[cfg(all(feature = "alloc", feature = "unsafe-optimizations"))]
        assert_eq!(core::ptr::null::<u8>(), None::<&mut bool>.encode(&mut ()));

        #[cfg(feature = "alloc")]
        assert_eq!(
            CBoxedSlice::<u8>::none(),
            None::<String>.encode(&mut Default::default())
        );
        #[cfg(feature = "alloc")]
        assert_eq!(
            CBoxedSlice::<u8>::none(),
            None::<Box<str>>.encode(&mut Default::default())
        );

        assert_eq!(CSlice::<u8>::none(), None::<&str>.encode(&mut ()));

        #[cfg(all(feature = "alloc", feature = "unsafe-optimizations"))]
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
            CBoxedSlice::<u8>::none(),
            None::<ManuallyDrop<String>>.encode(&mut Default::default())
        );

        assert_eq!(2_u8, None::<ManuallyDrop<bool>>.encode(&mut ()));
    }
}
