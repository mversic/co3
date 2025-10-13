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

    impl<R: ReprC> Ir for &R
    where
        R: Ir<Type = Robust>,
    {
        type Type = Transparent;
    }
    impl<R> Ir for &R
    where
        R: Ir<Type = Opaque>,
    {
        type Type = Transparent;
    }
    impl<R> Ir for &R
    where
        R: Ir<Type = Transparent>,
    {
        type Type = Transparent;
    }
    impl<'itm, R: Clone, S: Cloned + 'itm> Ir for &'itm R
    where
        R: Ir<Type = S>,
    {
        type Type = &'itm S;
    }
    //impl<'itm, R> Ir for &'itm R where R: Ir<Type = &Extern> {
    //    type Type = &'itm &Extern;
    //}
    //impl<'itm, R> Ir for &'itm R where R: Ir<Type = &mut Extern> {
    //    type Type = &'itm &mut Extern;
    //}

    impl<'itm, R> Ir for &'itm mut R
    where
        R: Ir<Type = Robust>,
    {
        type Type = Transparent;
    }
    impl<'itm, R> Ir for &'itm mut R
    where
        R: Ir<Type = Opaque>,
    {
        type Type = Transparent;
    }
    //impl<'itm, R> Ir for &'itm mut R where R: Ir<Type = &Extern> {
    //    type Type = &'itm mut &Extern;
    //}
    //impl<'itm, R> Ir for &'itm mut R where R: Ir<Type = &mut Extern> {
    //    type Type = &'itm mut &mut Extern;
    //}

    impl<'itm, R: ReprC> Ir for &'itm [R]
    where
        R: Ir<Type = Robust>,
    {
        type Type = &'itm [Robust];
    }
    impl<'itm, R> Ir for &'itm [R]
    where
        R: Ir<Type = Opaque>,
    {
        type Type = &'itm [Opaque];
    }
    impl<'itm, R> Ir for &'itm [R]
    where
        R: Ir<Type = Transparent>,
    {
        type Type = &'itm [Transparent];
    }
    impl<'itm, R: Clone, S: Cloned + 'itm> Ir for &'itm [R]
    where
        R: Ir<Type = S>,
    {
        type Type = &'itm [S];
    }
    //impl<'itm, R> Ir for &'itm [R] where R: Ir<Type = &Extern> {
    //    type Type = &'itm [&Extern];
    //}
    //impl<'itm, R> Ir for &'itm [R] where R: Ir<Type = &mut Extern> {
    //    type Type = &'itm [&mut Extern];
    //}

    impl<'itm, R> Ir for &'itm mut [R]
    where
        R: Ir<Type = Robust>,
    {
        type Type = &'itm mut [Robust];
    }
    impl<'itm, R> Ir for &'itm mut [R]
    where
        R: Ir<Type = Opaque>,
    {
        type Type = &'itm mut [Opaque];
    }
    //impl<'itm, R> Ir for &'itm mut [R] where R: Ir<Type = &Extern> {
    //    type Type = &'itm mut [&Extern];
    //}
    //impl<'itm, R> Ir for &'itm mut [R] where R: Ir<Type = &mut Extern> {
    //    type Type = &'itm mut [&mut Extern];
    //}

    impl<R: ReprC> Ir for Box<R>
    where
        R: Ir<Type = Robust>,
    {
        type Type = Box<Robust>;
    }
    impl<R> Ir for Box<R>
    where
        R: Ir<Type = Opaque>,
    {
        type Type = Box<Opaque>;
    }
    impl<R> Ir for Box<R>
    where
        R: Ir<Type = Transparent>,
    {
        type Type = Box<Transparent>;
    }
    impl<R: Clone, S: Cloned> Ir for Box<R>
    where
        R: Ir<Type = S>,
    {
        type Type = Box<S>;
    }
    //impl<R> Ir for Box<R> where R: Ir<Type = &Extern> {
    //    type Type = Box<&Extern>;
    //}
    //impl<R> Ir for Box<R> where R: Ir<Type = &mut Extern> {
    //    type Type = Box<&mut Extern>;
    //}

    impl<R: ReprC> Ir for Box<[R]>
    where
        R: Ir<Type = Robust>,
    {
        type Type = Box<[Robust]>;
    }
    impl<R> Ir for Box<[R]>
    where
        R: Ir<Type = Opaque>,
    {
        type Type = Box<[Opaque]>;
    }
    impl<R> Ir for Box<[R]>
    where
        R: Ir<Type = Transparent>,
    {
        type Type = Box<[Transparent]>;
    }
    impl<R: Clone, S: Cloned> Ir for Box<[R]>
    where
        R: Ir<Type = S>,
    {
        type Type = Box<[S]>;
    }
    //impl<R> Ir for Box<[R]> where R: Ir<Type = &Extern> {
    //    type Type = Box<[&Extern]>;
    //}
    //impl<R> Ir for Box<[R]> where R: Ir<Type = &mut Extern> {
    //    type Type = Box<[&mut Extern]>;
    //}

    impl<R: ReprC> Ir for Vec<R>
    where
        R: Ir<Type = Robust>,
    {
        type Type = Vec<Robust>;
    }
    impl<R> Ir for Vec<R>
    where
        R: Ir<Type = Opaque>,
    {
        type Type = Vec<Opaque>;
    }
    impl<R> Ir for Vec<R>
    where
        R: Ir<Type = Transparent>,
    {
        type Type = Vec<Transparent>;
    }
    impl<R: Clone, S: Cloned> Ir for Vec<R>
    where
        R: Ir<Type = S>,
    {
        type Type = Vec<S>;
    }
    impl<'itm, R> Ir for Vec<R>
    where
        R: Ir<Type = &'itm Extern>,
    {
        type Type = Vec<&'itm Extern>;
    }
    impl<R, 'itm> Ir for Vec<R>
    where
        R: Ir<Type = &'itm mut Extern>,
    {
        type Type = Vec<&'itm mut Extern>;
    }

    impl<R: ReprC, const N: usize> Ir for [R; N]
    where
        R: Ir<Type = Robust>,
    {
        type Type = Robust;
    }
    impl<R, const N: usize> Ir for [R; N]
    where
        R: Ir<Type = Opaque>,
    {
        type Type = [Opaque; N];
    }
    impl<R, const N: usize> Ir for [R; N]
    where
        R: Ir<Type = Transparent>,
    {
        type Type = Transparent;
    }
    impl<R: Clone, S: Cloned, const N: usize> Ir for [R; N]
    where
        R: Ir<Type = S>,
    {
        type Type = [S; N];
    }
    //impl<R> Ir for [R; N] where R: Ir<Type = &Extern> {
    //    type Type = [&Extern; N];
    //}
    //impl<R> Ir for [R; N] where R: Ir<Type = &mut Extern> {
    //    type Type = [&mut Extern; N];
    //}

    impl<'itm, R> Ir for &'itm R
    where
        R: Ir<Type = crate::Extern>,
    {
        type Type = &'itm crate::Extern;
    }
    impl<'itm, R> Ir for &'itm mut R
    where
        R: Ir<Type = crate::Extern>,
    {
        type Type = &'itm mut crate::Extern;
    }
    impl<'itm, R> Ir for &'itm [R]
    where
        R: Ir<Type = crate::Extern>,
    {
        type Type = &'itm [crate::Extern];
    }
    impl<'itm, R> Ir for &'itm mut [R]
    where
        R: Ir<Type = crate::Extern>,
    {
        type Type = &'itm mut [crate::Extern];
    }
    impl<R> Ir for Box<R>
    where
        R: Ir<Type = crate::Extern>,
    {
        type Type = Box<crate::Extern>;
    }
    impl<R> Ir for Box<[R]>
    where
        R: Ir<Type = crate::Extern>,
    {
        type Type = Box<[crate::Extern]>;
    }
    impl<R> Ir for Vec<R>
    where
        R: Ir<Type = crate::Extern>,
    {
        type Type = Vec<Transparent>;
    }
    impl<R, const N: usize> Ir for [R; N]
    where
        R: Ir<Type = crate::Extern>,
    {
        type Type = Transparent;
    }

    impl<'itm, R, const N: usize> Ir for &'itm mut [R; N]
    where
        [R; N]: Ir<Type = [Robust; N]>,
    {
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
