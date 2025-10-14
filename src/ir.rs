//! Internal Representation (IR) of Rust types during conversion into FFI types.
//!
//! While you can implement [`crate::ExternC`] directly on your type, it is often
//! preferable to map it into IR by implementing [`Ir`]. This approach gives you
//! automatic, correct, and zero-cost conversions from IR to the equivalent C type.
use alloc::{boxed::Box, vec::Vec};
use disjoint_impls::disjoint_impls;

#[cfg(not(feature = "non_robust_ref_mut"))]
use crate::transmute::InfallibleTransmute;
use crate::{Extern, LocalRef, LocalSlice, ReprC, repr_c::Cloned};

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

    impl<R: Ir<Type = Robust> + ReprC> Ir for &R {
        type Type = Transparent;
    }
    impl<R: Ir<Type = Opaque>> Ir for &R {
        type Type = Transparent;
    }
    impl<R: Ir<Type = Transparent>> Ir for &R {
        type Type = Transparent;
    }
    impl<'itm, R: Ir<Type = S> + Clone, S: Cloned + 'itm> Ir for &'itm R {
        type Type = &'itm S;
    }
    impl<'itm, R: Ir<Type = Extern>> Ir for &'itm R {
        type Type = &'itm Extern;
    }

    impl<'itm, R: Ir<Type = Robust>> Ir for &'itm mut R {
        type Type = Transparent;
    }
    impl<'itm, R: Ir<Type = Opaque>> Ir for &'itm mut R {
        type Type = Transparent;
    }
    impl<
        'itm,
        #[cfg(not(feature = "non_robust_ref_mut"))] R: InfallibleTransmute,
        #[cfg(feature = "non_robust_ref_mut")] R,
    > Ir for &'itm mut R
    where
        R: Ir<Type = Transparent>,
    {
        type Type = Transparent;
    }
    impl<'itm, R: Ir<Type = Extern>> Ir for &'itm mut R {
        type Type = &'itm mut Extern;
    }

    impl<'itm, R: Ir<Type = Robust> + ReprC> Ir for &'itm [R] {
        type Type = &'itm [Robust];
    }
    impl<'itm, R: Ir<Type = Opaque>> Ir for &'itm [R] {
        type Type = &'itm [Opaque];
    }
    impl<'itm, R: Ir<Type = Transparent>> Ir for &'itm [R] {
        type Type = &'itm [Transparent];
    }
    impl<'itm, R: Ir<Type = S> + Clone, S: Cloned + 'itm> Ir for &'itm [R] {
        type Type = &'itm [S];
    }
    impl<'itm, R: Ir<Type = Extern>> Ir for &'itm [R] {
        type Type = &'itm [Extern];
    }

    impl<'itm, R: Ir<Type = Robust>> Ir for &'itm mut [R] {
        type Type = &'itm mut [Robust];
    }
    impl<'itm, R: Ir<Type = Opaque>> Ir for &'itm mut [R] {
        type Type = &'itm mut [Opaque];
    }
    impl<
        'itm,
        #[cfg(not(feature = "non_robust_ref_mut"))] R: InfallibleTransmute,
        #[cfg(feature = "non_robust_ref_mut")] R,
    > Ir for &'itm mut [R]
    where
        R: Ir<Type = Transparent>,
    {
        type Type = &'itm mut [Transparent];
    }
    impl<'itm, R: Ir<Type = Extern>> Ir for &'itm mut [R] {
        type Type = &'itm mut [Extern];
    }

    impl<R: Ir<Type = Robust> + ReprC> Ir for Box<R> {
        type Type = Box<Robust>;
    }
    impl<R: Ir<Type = Opaque>> Ir for Box<R> {
        type Type = Box<Opaque>;
    }
    impl<R: Ir<Type = Transparent>> Ir for Box<R> {
        type Type = Box<Transparent>;
    }
    impl<R: Ir<Type = S> + Clone, S: Cloned> Ir for Box<R> {
        type Type = Box<S>;
    }
    impl<R: Ir<Type = Extern>> Ir for Box<R> {
        type Type = Box<Extern>;
    }

    impl<R: Ir<Type = Robust> + ReprC> Ir for Box<[R]> {
        type Type = Box<[Robust]>;
    }
    impl<R: Ir<Type = Opaque>> Ir for Box<[R]> {
        type Type = Box<[Opaque]>;
    }
    impl<R: Ir<Type = Transparent>> Ir for Box<[R]> {
        type Type = Box<[Transparent]>;
    }
    impl<R: Ir<Type = S> + Clone, S: Cloned> Ir for Box<[R]> {
        type Type = Box<[S]>;
    }
    impl<R: Ir<Type = Extern>> Ir for Box<[R]> {
        type Type = Box<[Extern]>;
    }

    impl<R: Ir<Type = Robust> + ReprC> Ir for Vec<R> {
        type Type = Vec<Robust>;
    }
    impl<R: Ir<Type = Opaque>> Ir for Vec<R> {
        type Type = Vec<Opaque>;
    }
    impl<R: Ir<Type = Transparent>> Ir for Vec<R> {
        type Type = Vec<Transparent>;
    }
    impl<R: Ir<Type = S> + Clone, S: Cloned> Ir for Vec<R> {
        type Type = Vec<S>;
    }
    // FIXME: This seems suspicious when compared with Opaque
    impl<R: Ir<Type = Extern>> Ir for Vec<R> {
        type Type = Vec<Transparent>;
    }

    impl<R: Ir<Type = Opaque>, const N: usize> Ir for [R; N] {
        type Type = [Opaque; N];
    }
    impl<R: Ir<Type = Transparent>, const N: usize> Ir for [R; N] {
        type Type = Transparent;
    }
    impl<R: Ir<Type = S> + Clone, S: Cloned, const N: usize> Ir for [R; N] {
        type Type = [S; N];
    }
    // FIXME: This seems suspicious when compared with Opaque
    impl<R: Ir<Type = Extern>, const N: usize> Ir for [R; N] {
        type Type = Transparent;
    }

    // TODO: due to https://github.com/mversic/co3/issues/13 we can't yet implement
    // traits only for some const values (non-zero). Otherwise, it should be just:
    // R: Ir<Type = Robust>,
    impl<R: Ir<Type = Robust> + ReprC, const N: usize> Ir for [R; N] {
        type Type = [Robust; N];
    }
    impl<R, const N: usize> Ir for &[R; N]
    where
        [R; N]: Ir<Type = [Robust; N]>,
    {
        type Type = Transparent;
    }
    impl<R, const N: usize> Ir for &mut [R; N]
    where
        [R; N]: Ir<Type = [Robust; N]>,
    {
        type Type = Transparent;
    }
    impl<'itm, R, const N: usize> Ir for &'itm [[R; N]]
    where
        [R; N]: Ir<Type = [Robust; N]>,
    {
        type Type = &'itm [Robust];
    }
    impl<'itm, R, const N: usize> Ir for &'itm mut [[R; N]]
    where
        [R; N]: Ir<Type = [Robust; N]>,
    {
        type Type = &'itm mut [Robust];
    }
    impl<R, const N: usize> Ir for Box<[R; N]>
    where
        [R; N]: Ir<Type = [Robust; N]>,
    {
        type Type = Box<Robust>;
    }
    impl<R, const N: usize> Ir for Box<[[R; N]]>
    where
        [R; N]: Ir<Type = [Robust; N]>,
    {
        type Type = Box<[Robust]>;
    }
    impl<R, const N: usize> Ir for Vec<[R; N]>
    where
        [R; N]: Ir<Type = [Robust; N]>,
    {
        type Type = Vec<Robust>;
    }
}

/// Represents the pointee on the far side of an exported opaque pointer at the FFI boundary.
///
/// # Safety
///
/// Implementors must guarantee that:
/// - `Self` has the same representation as `*mut` [`Extern`].
/// - [`External::RefType`] has the same representation as `*const` [`Extern`].
/// - [`External::RefMutType`] has the same representation as `*mut` [`Extern`].
pub unsafe trait External {
    /// Type that replaces `&T` when imported over FFI.
    type RefType<'itm>;

    /// Type that replaces `&mut T` when imported over FFI.
    type RefMutType<'itm>;

    /// Returns a shared opaque pointer.
    fn as_extern_ptr(&self) -> *const Extern;

    /// Returns a mutable opaque pointer.
    fn as_extern_ptr_mut(&mut self) -> *mut Extern;

    /// Constructs `Self` from an opaque pointer.
    ///
    /// # Safety
    ///
    /// The pointer argument must be valid.
    unsafe fn from_extern_ptr(source: *mut Extern) -> Self;
}

/// Marker for a type exported as an opaque pointer over FFI.
pub enum Opaque {}

/// Marker for a type that is transparent with respect to its wrapped type.
pub enum Transparent {}

/// Marker for a robust [`crate::ReprC`] type that does not require conversion
pub enum Robust {}

impl<R> Ir for *const R {
    type Type = Robust;
}
impl<R> Ir for *mut R {
    type Type = Robust;
}

impl<'itm, R: 'itm> Ir for LocalRef<'itm, R>
where
    &'itm R: Ir,
{
    type Type = <&'itm R as Ir>::Type;
}
impl<'itm, R: 'itm> Ir for LocalSlice<'itm, R>
where
    &'itm [R]: Ir,
{
    type Type = <&'itm [R] as Ir>::Type;
}
