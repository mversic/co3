//! Internal Representation (IR) of Rust types during conversion into FFI types.
//!
//! While you can implement [`crate::ExternC`] directly on your type, it is often
//! preferable to map it into IR by implementing [`Ir`]. This approach gives you
//! automatic, correct, and zero-cost conversions from IR to the equivalent C type.
#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};
use core::{cell::UnsafeCell, ops::Add};

use disjoint_impls::disjoint_impls;

use crate::{
    niche::{NicheFamily, WithCustomNiche, WithNiche, WithStableNiche, WithoutNiche},
    size::{MetaSized, SizeFamily, Thin},
};

/// Marker for a robust [`ReprC`] type that does not require conversion
pub enum Robust {}

/// Marker for a type that can is transmuted to another type and thus delegates it's conversion
pub enum Transmuted {}

disjoint_impls! {
    /// Type that can be converted to and from an internal representation (IR).
    ///
    /// Predefined IR types automatically implement [`crate::ExternC`] and related conversion traits.
    pub trait ReprFamily {
        /// The internal representation (i.e. type family) of the type
        ///
        /// - If `Self` is [`ReprC`], set [`ReprFamily::Kind`] to [`Robust`].
        ///   The type is passed to FFI functions as-is, without conversion.
        ///
        /// - If [`ReprFamily::Kind`] is [`Transmuted`], `Self` automatically implements [`crate::ExternC`]
        ///   by delegating to its inner type via [`core::mem::transmute`].
        ///   If the inner type supports zero-copy conversion, then [`Transmuted`] is also zero-copy.
        ///   See [`crate::transmute::CheckedTransmute`] for more details.
        ///
        /// - If [`ReprFamily::Kind`] is [`Option<T>`], `Option<T>` is transmuted into the inner type,
        ///   using its *niche value* to represent [`None`].
        ///
        /// - If [`ReprFamily::Kind`] is [`Option<WithoutNiche>`], serialization is delegated to the
        ///   inner type, but represented explicitly as a `(discriminant, value)` tuple.
        ///
        /// - In the common case, set [`ReprFamily::Kind`] to `Self`
        ///   This provides a default [`crate::ExternC`] implementation, but note that it will store the type.
        type Kind: ?Sized;
    }

    impl<R: ReprFamily<Kind = Robust>> ReprFamily for [R] {
        type Kind = Robust;
    }
    impl<R: ReprFamily<Kind = Transmuted>> ReprFamily for [R] {
        type Kind = Transmuted;
    }
    impl<R: ReprFamily<Kind = R>> ReprFamily for [R] {
        type Kind = Self;
    }

    impl<R: ReprFamily<Kind = Robust> + SizeFamily<Kind: Thin>> ReprFamily for &R {
        type Kind = Transmuted;
    }
    impl<R: ReprFamily<Kind = Robust> + SizeFamily<Kind = MetaSized<K>> + ?Sized, K> ReprFamily for &R {
        type Kind = Self;
    }
    impl<R: ReprFamily<Kind = Transmuted> + SizeFamily<Kind: Thin>> ReprFamily for &R {
        type Kind = Transmuted;
    }
    impl<R: ReprFamily<Kind = Transmuted> + SizeFamily<Kind = MetaSized<K>> + ?Sized, K> ReprFamily for &R {
        type Kind = Self;
    }
    impl<R: ReprFamily<Kind = R> + ?Sized> ReprFamily for &R {
        type Kind = Self;
    }

    impl<R: ReprFamily<Kind = Robust> + SizeFamily<Kind: Thin>> ReprFamily for &mut R {
        type Kind = Transmuted;
    }
    impl<R: ReprFamily<Kind = Robust> + SizeFamily<Kind = MetaSized<K>> + ?Sized, K> ReprFamily for &mut R {
        type Kind = Self;
    }
    impl<R: ReprFamily<Kind = Transmuted> + SizeFamily<Kind: Thin>> ReprFamily for &mut R {
        type Kind = Transmuted;
    }
    impl<R: ReprFamily<Kind = Transmuted> + SizeFamily<Kind = MetaSized<K>> + ?Sized, K> ReprFamily for &mut R {
        type Kind = Self;
    }
    impl<R: ReprFamily<Kind = R> + ?Sized> ReprFamily for &mut R {
        type Kind = Self;
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Robust> + SizeFamily<Kind: Thin>> ReprFamily for Box<R> {
        type Kind = Transmuted;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Robust> + SizeFamily<Kind = MetaSized<K>> + ?Sized, K> ReprFamily for Box<R> {
        type Kind = Self;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted> + SizeFamily<Kind: Thin>> ReprFamily for Box<R> {
        type Kind = Transmuted;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted> + SizeFamily<Kind = MetaSized<K>> + ?Sized, K> ReprFamily for Box<R> {
        type Kind = Self;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = R> + ?Sized> ReprFamily for Box<R> {
        type Kind = Self;
    }

    impl<R: ReprFamily<Kind = Robust>, const N: usize> ReprFamily for [R; N] {
        type Kind = Robust;
    }
    impl<R: ReprFamily<Kind = Transmuted>, const N: usize> ReprFamily for [R; N] {
        type Kind = Transmuted;
    }
    impl<R: ReprFamily<Kind = R>, const N: usize> ReprFamily for [R; N] {
        type Kind = Self;
    }

    impl<R: ReprFamily<Kind = Robust>> ReprFamily for Option<R> {
        type Kind = Self;
    }
    impl<R: ReprFamily<Kind = Transmuted> + NicheFamily<Kind = WithoutNiche>> ReprFamily for Option<R> {
        type Kind = Self;
    }
    impl<R: ReprFamily<Kind = Transmuted> + NicheFamily<Kind = WithCustomNiche>> ReprFamily for Option<R> {
        type Kind = Self;
    }
    impl<R: ReprFamily<Kind = Transmuted> + NicheFamily<Kind = WithStableNiche>> ReprFamily for Option<R> {
        type Kind = Transmuted;
    }
    impl<R: ReprFamily<Kind = R>> ReprFamily for Option<R> {
        type Kind = Self;
    }
    // TODO: Make a test. I think the example type is &Box<u8>
    // Should it become Stored in this case? Likewise for Result
    //impl<R: ReprFamily<Kind = R> + NicheFamily<Kind = WithStableNiche>> ReprFamily for Option<R> {
    //    type Kind = Self;
    //}

    impl<R: NicheFamily<Kind = WithoutNiche>, E: NicheFamily<Kind = WithoutNiche>> ReprFamily for Result<R, E> {
        type Kind = Self;
    }
    // TODO: Implement for niche optimized Results
}

disjoint_impls! {
    pub trait EncodeReprFamily: ReprFamily {
        type Kind: ?Sized;
    }

    impl<R: ReprFamily<Kind = Robust> + ?Sized> EncodeReprFamily for R {
        type Kind = Robust;
    }
    impl<R: ReprFamily<Kind = Self> + ?Sized> EncodeReprFamily for R {
        type Kind = Self;
    }

    impl<R: EncodeReprFamily> EncodeReprFamily for [R]
    where
        Self: ReprFamily<Kind = Transmuted>,
    {
        type Kind = <R as EncodeReprFamily>::Kind;
    }

    impl<R: EncodeReprFamily + ?Sized> EncodeReprFamily for &R
    where
        Self: ReprFamily<Kind = Transmuted>,
    {
        type Kind = <R as EncodeReprFamily>::Kind;
    }

    // FIXME: &mut UnsafeCell<R> will go through here and be wrong. This
    // means that using niche family to disambiguate is not correct
    impl<R: NicheFamily<Kind = WithoutNiche> + ?Sized> EncodeReprFamily for &mut R
    where
        Self: ReprFamily<Kind = Transmuted>,
    {
        type Kind = Transmuted;
    }
    impl<R: NicheFamily<Kind: WithNiche> + ?Sized> EncodeReprFamily for &mut R
    where
        Self: ReprFamily<Kind = Transmuted>,
    {
        // TODO: With a `Unchecked<T>` wrapper we can transmute this
        type Kind = Self;
    }

    // FIXME: &UnsafeCell should be distinguished as a mutable reference type
    //impl<R: NicheFamily<Kind = WithoutNiche> + ?Sized> EncodeReprFamily for &UnsafeCell<R>
    //where
    //    Self: ReprFamily<Kind = Transmuted>,
    //{
    //    type Kind = Transmuted;
    //}
    //impl<R: NicheFamily<Kind: WithNiche> + ?Sized> EncodeReprFamily for &UnsafeCell<R>
    //where
    //    Self: ReprFamily<Kind = Transmuted>,
    //{
    //    // TODO: With a `Unchecked<T>` wrapper we can transmute this
    //    type Kind = Self;
    //}

    #[cfg(feature = "alloc")]
    impl<R: EncodeReprFamily + ?Sized> EncodeReprFamily for Box<R>
    where
        Self: ReprFamily<Kind = Transmuted>,
    {
        type Kind = <R as EncodeReprFamily>::Kind;
    }

    impl<R: EncodeReprFamily, const N: usize> EncodeReprFamily for [R; N]
    where
        Self: ReprFamily<Kind = Transmuted>,
    {
        type Kind = <R as EncodeReprFamily>::Kind;
    }

    impl<R: EncodeReprFamily> EncodeReprFamily for Option<R>
    where
        Self: ReprFamily<Kind = Transmuted>,
    {
        type Kind = <R as EncodeReprFamily>::Kind;
    }
}

#[cfg(feature = "alloc")]
impl<R> ReprFamily for Vec<R> {
    type Kind = Self;
}

impl Add for Robust {
    type Output = Robust;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}

impl Add<Transmuted> for Robust {
    type Output = Transmuted;

    fn add(self, _: Transmuted) -> Self::Output {
        unreachable!()
    }
}

impl<T: ReprFamily<Kind = T>> Add<T> for Robust {
    type Output = T;

    fn add(self, _: T) -> Self::Output {
        unreachable!()
    }
}

impl Add<Robust> for Transmuted {
    type Output = Transmuted;

    fn add(self, _: Robust) -> Self::Output {
        unreachable!()
    }
}

impl Add for Transmuted {
    type Output = Transmuted;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}

impl<T: ReprFamily<Kind = T>> Add<T> for Transmuted {
    type Output = T;

    fn add(self, _: T) -> Self::Output {
        unreachable!()
    }
}
