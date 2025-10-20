//! Internal Representation (IR) of Rust types during conversion into FFI types.
//!
//! While you can implement [`crate::ExternC`] directly on your type, it is often
//! preferable to map it into IR by implementing [`Ir`]. This approach gives you
//! automatic, correct, and zero-cost conversions from IR to the equivalent C type.
use alloc::{boxed::Box, vec::Vec};
use disjoint_impls::disjoint_impls;

use crate::ReprC;
#[cfg(not(feature = "non_robust_ref_mut"))]
use crate::transmute::InfallibleTransmute;

/// Marker for a type that is transparent with respect to its wrapped type.
pub enum Transparent {}

/// Marker for a robust [`crate::ReprC`] type that does not require conversion
pub enum Robust {}

/// Marker for a type exported as an opaque pointer over FFI.
pub enum Opaque {}

/// Marker for a type imported as an opaque pointer over FFI.
pub enum Extern {}

/// Marker for an [`Ir`] type that delegates to the pointed-to type when converting
/// the likes of `&Self` or `&[Self]` into an FFI-compatible representation
///
/// This type clones the pointed-to value to get owned value that has implemented
/// [`ExternC`]. This type therefore uses the store
pub trait Cloned {}

disjoint_impls! {
    /// Designates a type that can be converted to and from an internal representation (IR).
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
        ///   See [`crate::Transmute`] for more details.
        ///
        /// - If [`Ir::Type`] is [`Opaque`], `T` is serialized as an opaque pointer.
        ///   Note that the type will be heap allocated during conversion if not already.
        ///   [`Opaque`] is the only family of types that transfer ownership across FFI.
        ///
        /// - If [`Ir::Type`] is [`Extern`], represents the pointee on the far side of an [`Opaque`] pointer
        ///
        /// - If [`Ir::Type`] is [`Option<T>`], `Option<T>` is transmuted into the inner type,
        ///   using its *niche value* to represent [`None`].
        ///
        /// - If [`Ir::Type`] is [`Option<WithoutNiche>`], serialization is delegated to the
        ///   inner type, but represented explicitly as a `(discriminant, value)` tuple.
        ///
        /// - In the common case, set [`Ir::Type`] to `Self` and implement [`Cloned`].
        ///   This provides a default [`crate::ExternC`] implementation, but note that it will clone the type.
        type Type;
    }

    impl<R: Ir<Type = Transparent>> Ir for &R {
        type Type = Transparent;
    }
    impl<R: Ir<Type = Robust> + ReprC> Ir for &R {
        type Type = Transparent;
    }
    impl<R: Ir<Type = Opaque>> Ir for &R {
        type Type = Transparent;
    }
    impl<'a, R: Ir<Type = Extern>> Ir for &'a R {
        type Type = &'a Extern;
    }
    impl<'a, R: Ir<Type = S>, S: Cloned + 'a> Ir for &'a R {
        type Type = &'a S;
    }

    impl<
        'a,
        #[cfg(not(feature = "non_robust_ref_mut"))] R: InfallibleTransmute,
        #[cfg(feature = "non_robust_ref_mut")] R,
    > Ir for &'a mut R
    where
        R: Ir<Type = Transparent>,
    {
        type Type = Transparent;
    }
    impl<'a, R: Ir<Type = Robust>> Ir for &'a mut R {
        type Type = Transparent;
    }
    impl<'a, R: Ir<Type = Opaque>> Ir for &'a mut R {
        type Type = Transparent;
    }
    impl<'a, R: Ir<Type = Extern>> Ir for &'a mut R {
        type Type = &'a mut Extern;
    }

    impl<'a, R: Ir<Type = Transparent>> Ir for &'a [R] {
        type Type = &'a [Transparent];
    }
    impl<'a, R: Ir<Type = Robust> + ReprC> Ir for &'a [R] {
        type Type = &'a [Robust];
    }
    impl<'a, R: Ir<Type = Opaque>> Ir for &'a [R] {
        type Type = &'a [Opaque];
    }
    impl<'a, R: Ir<Type = Extern>> Ir for &'a [R] {
        type Type = &'a [Extern];
    }
    impl<'a, R: Ir<Type = S>, S: Cloned + 'a> Ir for &'a [R] {
        type Type = &'a [S];
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

    impl<R: Ir<Type = Transparent>> Ir for Box<R> {
        type Type = Box<Transparent>;
    }
    impl<R: Ir<Type = Robust> + ReprC> Ir for Box<R> {
        type Type = Box<Robust>;
    }
    impl<R: Ir<Type = Opaque>> Ir for Box<R> {
        type Type = Transparent;
    }
    impl<R: Ir<Type = Extern>> Ir for Box<R> {
        type Type = Box<Extern>;
    }
    impl<R: Ir<Type = S>, S: Cloned> Ir for Box<R> {
        type Type = Box<S>;
    }

    impl<R: Ir<Type = Transparent>> Ir for Box<[R]> {
        type Type = Box<[Transparent]>;
    }
    impl<R: Ir<Type = Robust> + ReprC> Ir for Box<[R]> {
        type Type = Box<[Robust]>;
    }
    impl<R: Ir<Type = Opaque>> Ir for Box<[R]> {
        type Type = Box<[Opaque]>;
    }
    impl<R: Ir<Type = Extern>> Ir for Box<[R]> {
        type Type = Box<[Extern]>;
    }
    impl<R: Ir<Type = S>, S: Cloned> Ir for Box<[R]> {
        type Type = Box<[S]>;
    }

    impl<R: Ir<Type = Transparent>> Ir for Vec<R> {
        type Type = Vec<Transparent>;
    }
    impl<R: Ir<Type = Robust> + ReprC> Ir for Vec<R> {
        type Type = Vec<Robust>;
    }
    impl<R: Ir<Type = Opaque>> Ir for Vec<R> {
        type Type = Vec<Opaque>;
    }
    // FIXME: This seems suspicious when compared with Opaque
    impl<R: Ir<Type = Extern>> Ir for Vec<R> {
        type Type = Vec<Transparent>;
    }
    impl<R: Ir<Type = S>, S: Cloned> Ir for Vec<R> {
        type Type = Vec<S>;
    }

    impl<R: Ir<Type = Robust> + ReprC, const N: usize> Ir for [R; N] {
        // WARN: due to https://github.com/mversic/co3/issues/13 we can't yet implement
        // traits only for some const values (non-zero). Therefore, the user must make
        // sure they don't have any `[Robust; 0]` types crossing the FFI boundary
        type Type = Robust;
    }
    impl<R: Ir<Type = Transparent>, const N: usize> Ir for [R; N] {
        type Type = Transparent;
    }
    impl<R: Ir<Type = Opaque>, const N: usize> Ir for [R; N] {
        type Type = [Opaque; N];
    }
    // FIXME: This seems suspicious when compared with Opaque
    impl<R: Ir<Type = Extern>, const N: usize> Ir for [R; N] {
        type Type = Transparent;
    }
    impl<R: Ir<Type = S>, S: Cloned, const N: usize> Ir for [R; N] {
        type Type = [S; N];
    }

    impl<R> Ir for &Box<R> where Box<R>: Ir<Type = Box<Robust>> {
        type Type = Transparent;
    }
    impl<R> Ir for &mut Box<R> where Box<R>: Ir<Type = Box<Robust>> {
        type Type = Transparent;
    }
    impl<'a, R> Ir for &'a [Box<R>] where Box<R>: Ir<Type = Box<Robust>> {
        type Type = &'a [Transparent];
    }
    impl<'a, R> Ir for &'a mut [Box<R>] where Box<R>: Ir<Type = Box<Robust>> {
        type Type = &'a mut [Transparent];
    }
    impl<R> Ir for Box<Box<R>> where Box<R>: Ir<Type = Box<Robust>> {
        type Type = Box<Transparent>;
    }
    impl<R> Ir for Box<[Box<R>]> where Box<R>: Ir<Type = Box<Robust>> {
        type Type = Box<[Transparent]>;
    }
    impl<R> Ir for Vec<Box<R>> where Box<R>: Ir<Type = Box<Robust>> {
        type Type = Vec<Transparent>;
    }
    impl<R, const N: usize> Ir for [Box<R>; N] where Box<R>: Ir<Type = Box<Robust>> {
        type Type = Robust;
    }
    // TODO: What about Option<[Box<R>; N]> where R: Robust?
}

impl<R: Ir<Type: Cloned>> Cloned for &R {}
impl Cloned for &Extern {}
impl<R> Cloned for &[R] {}
impl<R: Ir<Type: Cloned>> Cloned for Box<R> {}
impl<R> Cloned for Vec<R> {}
impl<const N: usize> Cloned for [Opaque; N] {}
impl<R: Ir<Type: Cloned>, const N: usize> Cloned for [R; N] {}

impl<R> Ir for *const R {
    type Type = Robust;
}
impl<R> Ir for *mut R {
    type Type = Robust;
}
