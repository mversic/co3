//! Internal Representation (IR) of Rust types during conversion into FFI types.
//!
//! While you can implement [`crate::ExternC`] directly on your type, it is often
//! preferable to map it into IR by implementing [`Ir`]. This approach gives you
//! automatic, correct, and zero-cost conversions from IR to the equivalent C type.
use alloc::{boxed::Box, vec::Vec};
use disjoint_impls::disjoint_impls;

use crate::niche::{WithCustomNiche, WithStableNiche};
#[cfg(not(feature = "non_robust_ref_mut"))]
use crate::transmute::InfallibleTransmute;

/// Marker for an [`Ir`] type that delegates to the pointed-to type when converting
/// the likes of `&Self` or `&[Self]` into an FFI-compatible representation
///
/// This type clones the pointed-to value to get owned value that has implemented
/// [`ExternC`]. This type therefore uses the store
pub trait Cloned {}

/// Marker for a type that is transparent with respect to its wrapped type.
pub enum Transparent {}

/// Marker for a robust [`crate::ReprC`] type that does not require conversion
pub enum Robust {}

/// Marker for a type exported as an opaque pointer over FFI.
pub enum Opaque {}

disjoint_impls! {
    /// Type that can be converted to and from an internal representation (IR).
    ///
    /// Predefined IR types automatically implement [`crate::ExternC`] and related conversion traits.
    pub trait Ir {
        /// The internal representation (i.e. type family) of the type
        ///
        /// - If `Self` is [`crate::ReprC`], set [`Ir::Type`] to [`Robust`].
        ///   The type is passed to FFI functions as-is, without conversion.
        ///
        /// - If [`Ir::Type`] is [`Transparent`], `Self` automatically implements [`crate::ExternC`]
        ///   by delegating to its inner type via [`core::mem::transmute`].
        ///   If the inner type supports zero-copy conversion, then [`Transparent`] is also zero-copy.
        ///   See [`crate::transmute::CheckedTransmute`] for more details.
        ///
        /// - If [`Ir::Type`] is [`Opaque`], `T` is serialized as an opaque pointer.
        ///   Note that the type will be heap allocated during conversion if not already.
        ///   [`Opaque`] is the only family of types that transfer ownership across FFI.
        ///
        /// - If [`Ir::Type`] is [`Option<T>`], `Option<T>` is transmuted into the inner type,
        ///   using its *niche value* to represent [`None`].
        ///
        /// - If [`Ir::Type`] is [`Option<Robust>`], serialization is delegated to the
        ///   inner type, but represented explicitly as a `(discriminant, value)` tuple.
        ///
        /// - In the common case, set [`Ir::Type`] to `Self` and implement [`Cloned`].
        ///   This provides a default [`crate::ExternC`] implementation, but note that it will clone the type.
        type Type;
    }

    impl<R: Ir<Type = Box<Robust>>> Ir for &R {
        type Type = Transparent;
    }
    impl<R: Ir<Type = Transparent>> Ir for &R {
        type Type = Transparent;
    }
    impl<R: Ir<Type = Robust>> Ir for &R {
        type Type = Transparent;
    }
    impl<R: Ir<Type = Opaque>> Ir for &R {
        type Type = Transparent;
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R: Ir<Type: Cloned + 'a>> Ir for &'a R {
        type Type = &'a R::Type;
    }

    #[cfg(feature = "non_robust_ref_mut")]
    impl<R: Ir<Type = Box<Robust>>> Ir for &mut R {
        type Type = Transparent;
    }
    impl<
        #[cfg(not(feature = "non_robust_ref_mut"))] R: InfallibleTransmute,
        #[cfg(feature = "non_robust_ref_mut")] R,
    > Ir for &mut R
    where
        R: Ir<Type = Transparent>,
    {
        type Type = Transparent;
    }
    impl<R: Ir<Type = Robust>> Ir for &mut R {
        type Type = Transparent;
    }
    impl<R: Ir<Type = Opaque>> Ir for &mut R {
        type Type = Transparent;
    }

    #[cfg(feature = "owned_types")]
    impl<R: Ir<Type = Box<Robust>>> Ir for Box<R> {
        type Type = Transparent;
    }
    impl<R: Ir<Type = Transparent>> Ir for Box<R> {
        type Type = Transparent;
    }
    #[cfg(feature = "owned_types")]
    impl<R: Ir<Type = Robust>> Ir for Box<R> {
        type Type = Box<Robust>;
    }
    impl<R: Ir<Type = Opaque>> Ir for Box<R> {
        type Type = Transparent;
    }
    #[cfg(feature = "owned_types")]
    impl<R: Ir<Type: Cloned>> Ir for Box<R> {
        type Type = Box<R::Type>;
    }

    impl<'a, R: Ir<Type = Box<Robust>>> Ir for &'a [R] {
        type Type = &'a [Transparent];
    }
    impl<'a, R: Ir<Type = Transparent>> Ir for &'a [R] {
        type Type = &'a [Transparent];
    }
    impl<'a, R: Ir<Type = Robust>> Ir for &'a [R] {
        type Type = &'a [Robust];
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R: Ir<Type = Opaque>> Ir for &'a [R] {
        type Type = &'a [Opaque];
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R: Ir<Type: Cloned + 'a>> Ir for &'a [R] {
        type Type = &'a [R::Type];
    }

    #[cfg(feature = "non_robust_ref_mut")]
    impl<'a, R: Ir<Type = Box<Robust>>> Ir for &'a mut [R] {
        type Type = &'a mut [Transparent];
    }
    impl<
        'a,
        #[cfg(not(feature = "non_robust_ref_mut"))] R: InfallibleTransmute,
        #[cfg(feature = "non_robust_ref_mut")] R,
    > Ir for &'a mut [R]
    where
        R: Ir<Type = Transparent>,
    {
        type Type = &'a mut [Transparent];
    }
    impl<'a, R: Ir<Type = Robust>> Ir for &'a mut [R] {
        type Type = &'a mut [Robust];
    }

    #[cfg(feature = "owned_types")]
    impl<R: Ir<Type = Box<Robust>>> Ir for Box<[R]> {
        type Type = Box<[Transparent]>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: Ir<Type = Transparent>> Ir for Box<[R]> {
        type Type = Box<[Transparent]>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: Ir<Type = Robust>> Ir for Box<[R]> {
        type Type = Box<[Robust]>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: Ir<Type = Opaque>> Ir for Box<[R]> {
        type Type = Box<[Opaque]>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: Ir<Type: Cloned>> Ir for Box<[R]> {
        type Type = Box<[R::Type]>;
    }

    #[cfg(feature = "owned_types")]
    impl<R: Ir<Type = Box<Robust>>> Ir for Vec<R> {
        type Type = Vec<Transparent>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: Ir<Type = Transparent>> Ir for Vec<R> {
        type Type = Vec<Transparent>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: Ir<Type = Robust>> Ir for Vec<R> {
        type Type = Vec<Robust>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: Ir<Type = Opaque>> Ir for Vec<R> {
        type Type = Vec<Opaque>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: Ir<Type: Cloned>> Ir for Vec<R> {
        type Type = Vec<R::Type>;
    }

    #[cfg(feature = "owned_types")]
    impl<R: Ir<Type = Box<Robust>>, const N: usize> Ir for [R; N] {
        type Type = Box<Robust>;
    }
    impl<R: Ir<Type = Transparent>, const N: usize> Ir for [R; N] {
        type Type = Transparent;
    }
    impl<R: Ir<Type = Robust>, const N: usize> Ir for [R; N] {
        type Type = Robust;
    }
    impl<R: Ir<Type = Opaque>, const N: usize> Ir for [R; N] {
        type Type = [Opaque; N];
    }
    impl<R: Ir<Type: Cloned>, const N: usize> Ir for [R; N] {
        type Type = [R::Type; N];
    }

    // FIXME: Verify is correct
    //#[cfg(feature = "owned_types")]
    //impl<R: Ir<Type = Box<Robust>>> Ir for Option<R> {
    //    type Type = Box<Robust>;
    //}
    impl<R: Ir<Type = Transparent> + crate::niche::Ir<Type = Robust>> Ir for Option<R> {
        type Type = Option<Robust>;
    }
    impl<R: Ir<Type = Transparent> + crate::niche::Ir<Type = WithCustomNiche>> Ir for Option<R> {
        type Type = Option<WithCustomNiche>;
    }
    impl<R: Ir<Type = Transparent> + crate::niche::Ir<Type = WithStableNiche>> Ir for Option<R> {
        type Type = Option<WithStableNiche>;
    }
    impl<R: Ir<Type = Robust>> Ir for Option<R> {
        type Type = Option<Robust>;
    }
    impl<R: Ir<Type = Opaque>> Ir for Option<R> {
        type Type = Option<WithCustomNiche>;
    }
    impl<R: Ir<Type: Cloned> + crate::niche::Ir<Type = Robust>> Ir for Option<R> {
        type Type = Option<Robust>;
    }
    impl<R: Ir<Type: Cloned> + crate::niche::Ir<Type = WithCustomNiche>> Ir for Option<R> {
        type Type = Option<WithCustomNiche>;
    }

    impl<R: Ir<Type = Option<WithStableNiche>>> Ir for &R {
        type Type = Option<WithStableNiche>;
    }
    impl<R: Ir<Type = Option<WithStableNiche>>> Ir for &mut R {
        type Type = Option<WithStableNiche>;
    }
    impl<R: Ir<Type = Option<WithStableNiche>>> Ir for Box<R> {
        type Type = Option<WithStableNiche>;
    }
    impl<R: Ir<Type = Option<WithStableNiche>>, const N: usize> Ir for [R; N] {
        type Type = Option<WithStableNiche>;
    }
}

impl<S: Cloned> Cloned for &S {}
impl<S: Cloned> Cloned for Box<S> {}
impl<S> Cloned for &[S] {}
#[cfg(feature = "owned_types")]
impl<S> Cloned for Box<[S]> {}
#[cfg(feature = "owned_types")]
impl<S> Cloned for Vec<S> {}
impl<const N: usize> Cloned for [Opaque; N] {}
impl<S: Cloned, const N: usize> Cloned for [S; N] {}

impl Cloned for Option<Robust> {}
impl Cloned for Option<WithCustomNiche> {}

impl<R> Ir for *const R {
    type Type = Robust;
}
impl<R> Ir for *mut R {
    type Type = Robust;
}
