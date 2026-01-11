//! Internal Representation (IR) of Rust types during conversion into FFI types.
//!
//! While you can implement [`crate::ExternC`] directly on your type, it is often
//! preferable to map it into IR by implementing [`Ir`]. This approach gives you
//! automatic, correct, and zero-cost conversions from IR to the equivalent C type.
use alloc::{boxed::Box, vec::Vec};
use disjoint_impls::disjoint_impls;

use crate::{
    ReprC,
    niche::{NicheFamily, WithCustomNiche, WithStableNiche, WithoutNiche},
};

/// Marker for a [`ReprFamily`] type that delegates to the pointed-to type when converting
/// the likes of `&Self` or `&[Self]` into an FFI-compatible representation
///
/// This type clones the pointed-to value to get owned value that has implemented
/// [`ExternC`]. This type therefore uses the store
pub trait Cloned {}

/// Marker for a type that is transparent with respect to its wrapped type.
pub enum Transparent {}

/// Marker for a robust [`ReprC`] type that does not require conversion
pub enum Robust {}

/// Marker for a type exported as an opaque pointer over FFI.
pub enum Opaque {}

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
        /// - If [`ReprFamily::Kind`] is [`Transparent`], `Self` automatically implements [`crate::ExternC`]
        ///   by delegating to its inner type via [`core::mem::transmute`].
        ///   If the inner type supports zero-copy conversion, then [`Transparent`] is also zero-copy.
        ///   See [`crate::transmute::CheckedTransmute`] for more details.
        ///
        /// - If [`ReprFamily::Kind`] is [`Opaque`], `T` is serialized as an opaque pointer.
        ///   Note that the type will be heap allocated during conversion if not already.
        ///   [`Opaque`] is the only family of types that transfer ownership across FFI.
        ///
        /// - If [`ReprFamily::Kind`] is [`Option<T>`], `Option<T>` is transmuted into the inner type,
        ///   using its *niche value* to represent [`None`].
        ///
        /// - If [`ReprFamily::Kind`] is [`Option<WithoutNiche>`], serialization is delegated to the
        ///   inner type, but represented explicitly as a `(discriminant, value)` tuple.
        ///
        /// - In the common case, set [`ReprFamily::Kind`] to `Self` and implement [`Cloned`].
        ///   This provides a default [`crate::ExternC`] implementation, but note that it will clone the type.
        type Kind;
    }

    impl<R: ReprFamily<Kind = Box<Robust>>> ReprFamily for &R {
        type Kind = Transparent;
    }
    impl<R: ReprFamily<Kind = Transparent>> ReprFamily for &R {
        type Kind = Transparent;
    }
    impl<R: ReprFamily<Kind = Robust>> ReprFamily for &R {
        type Kind = Transparent;
    }
    impl<R: ReprFamily<Kind = Opaque>> ReprFamily for &R {
        type Kind = Transparent;
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R: ReprFamily<Kind: Cloned + 'a>> ReprFamily for &'a R {
        type Kind = &'a R::Kind;
    }

    #[cfg(feature = "non_robust_ref_mut")]
    impl<R: ReprFamily<Kind = Box<Robust>>> ReprFamily for &mut R {
        type Kind = Transparent;
    }
    impl<
        'a,
        #[cfg(not(feature = "non_robust_ref_mut"))] R: ReprC,
        #[cfg(feature = "non_robust_ref_mut")] R,
    > ReprFamily for &'a mut R
    where
        R: ReprFamily<Kind = Transparent>,
    {
        type Kind = Transparent;
    }
    impl<R: ReprFamily<Kind = Robust>> ReprFamily for &mut R {
        type Kind = Transparent;
    }
    impl<R: ReprFamily<Kind = Opaque>> ReprFamily for &mut R {
        type Kind = Transparent;
    }

    #[cfg(feature = "owned_types")]
    impl<R: ReprFamily<Kind = Box<Robust>>> ReprFamily for Box<R> {
        type Kind = Transparent;
    }
    impl<R: ReprFamily<Kind = Transparent>> ReprFamily for Box<R> {
        type Kind = Transparent;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprFamily<Kind = Robust>> ReprFamily for Box<R> {
        type Kind = Box<Robust>;
    }
    impl<R: ReprFamily<Kind = Opaque>> ReprFamily for Box<R> {
        type Kind = Transparent;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprFamily<Kind: Cloned>> ReprFamily for Box<R> {
        type Kind = Box<R::Kind>;
    }

    impl<'a, R: ReprFamily<Kind = Box<Robust>>> ReprFamily for &'a [R] {
        type Kind = &'a [Transparent];
    }
    impl<'a, R: ReprFamily<Kind = Transparent>> ReprFamily for &'a [R] {
        type Kind = &'a [Transparent];
    }
    impl<'a, R: ReprFamily<Kind = Robust>> ReprFamily for &'a [R] {
        type Kind = &'a [Robust];
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R: ReprFamily<Kind = Opaque>> ReprFamily for &'a [R] {
        type Kind = &'a [Opaque];
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R: ReprFamily<Kind: Cloned + 'a>> ReprFamily for &'a [R] {
        type Kind = &'a [R::Kind];
    }

    #[cfg(feature = "non_robust_ref_mut")]
    impl<'a, R: ReprFamily<Kind = Box<Robust>>> ReprFamily for &'a mut [R] {
        type Kind = &'a mut [Transparent];
    }
    impl<
        'a,
        #[cfg(not(feature = "non_robust_ref_mut"))] R: ReprC,
        #[cfg(feature = "non_robust_ref_mut")] R,
    > ReprFamily for &'a mut [R]
    where
        R: ReprFamily<Kind = Transparent>,
    {
        type Kind = &'a mut [Transparent];
    }
    impl<'a, R: ReprFamily<Kind = Robust>> ReprFamily for &'a mut [R] {
        type Kind = &'a mut [Robust];
    }

    #[cfg(feature = "owned_types")]
    impl<R: ReprFamily<Kind = Box<Robust>>> ReprFamily for Box<[R]> {
        type Kind = Box<[Transparent]>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprFamily<Kind = Transparent>> ReprFamily for Box<[R]> {
        type Kind = Box<[Transparent]>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprFamily<Kind = Robust>> ReprFamily for Box<[R]> {
        type Kind = Box<[Robust]>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprFamily<Kind = Opaque>> ReprFamily for Box<[R]> {
        type Kind = Box<[Opaque]>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprFamily<Kind: Cloned>> ReprFamily for Box<[R]> {
        type Kind = Box<[R::Kind]>;
    }

    #[cfg(feature = "owned_types")]
    impl<R: ReprFamily<Kind = Box<Robust>>> ReprFamily for Vec<R> {
        type Kind = Vec<Transparent>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprFamily<Kind = Transparent>> ReprFamily for Vec<R> {
        type Kind = Vec<Transparent>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprFamily<Kind = Robust>> ReprFamily for Vec<R> {
        type Kind = Vec<Robust>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprFamily<Kind = Opaque>> ReprFamily for Vec<R> {
        type Kind = Vec<Opaque>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprFamily<Kind: Cloned>> ReprFamily for Vec<R> {
        type Kind = Vec<R::Kind>;
    }

    #[cfg(feature = "owned_types")]
    impl<R: ReprFamily<Kind = Box<Robust>>, const N: usize> ReprFamily for [R; N] {
        type Kind = Box<Robust>;
    }
    impl<R: ReprFamily<Kind = Transparent>, const N: usize> ReprFamily for [R; N] {
        type Kind = Transparent;
    }
    impl<R: ReprFamily<Kind = Robust>, const N: usize> ReprFamily for [R; N] {
        type Kind = Robust;
    }
    impl<R: ReprFamily<Kind = Opaque>, const N: usize> ReprFamily for [R; N] {
        type Kind = [Opaque; N];
    }
    impl<R: ReprFamily<Kind: Cloned>, const N: usize> ReprFamily for [R; N] {
        type Kind = [R::Kind; N];
    }

    // FIXME: Verify is correct
    //#[cfg(feature = "owned_types")]
    //impl<R: ReprFamily<Type = Box<Robust>>> ReprFamily for Option<R> {
    //    type Kind = Box<Robust>;
    //}
    impl<R: ReprFamily<Kind = Transparent> + NicheFamily<Kind = WithoutNiche>> ReprFamily for Option<R> {
        type Kind = Option<WithoutNiche>;
    }
    impl<R: ReprFamily<Kind = Transparent> + NicheFamily<Kind = WithStableNiche>> ReprFamily for Option<R> {
        type Kind = Transparent;
    }
    impl<R: ReprFamily<Kind = Transparent> + NicheFamily<Kind = WithCustomNiche>> ReprFamily for Option<R> {
        type Kind = Option<WithCustomNiche>;
    }
    impl<R: ReprFamily<Kind = Robust>> ReprFamily for Option<R> {
        type Kind = Option<WithoutNiche>;
    }
    impl<R: ReprFamily<Kind = Opaque>> ReprFamily for Option<R> {
        type Kind = Option<WithCustomNiche>;
    }
    impl<R: ReprFamily<Kind: Cloned> + NicheFamily<Kind = WithoutNiche>> ReprFamily for Option<R> {
        type Kind = Option<WithoutNiche>;
    }
    impl<R: ReprFamily<Kind: Cloned> + NicheFamily<Kind = WithCustomNiche>> ReprFamily for Option<R> {
        type Kind = Option<WithCustomNiche>;
    }
    impl<R: ReprFamily<Kind = Option<WithStableNiche>>> ReprFamily for Option<R> {
        type Kind = Option<WithoutNiche>;
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

impl Cloned for Option<WithoutNiche> {}
impl Cloned for Option<WithCustomNiche> {}

macro_rules! impl_fn_types {
    ( $( ( $( $arg:ident ),* ) ),* $(,)? ) => {$(
        // FIXME: I'm not sure if arguments are required to be ReprFamilyC, what if fn pointer is opaque?
        // or should we create new function with argument conversion?
        unsafe impl<$($arg: ReprC,)* R: ReprC> ReprC for unsafe extern "C" fn($($arg),*) -> R {}
        unsafe impl<$($arg: ReprC,)*> ReprC for unsafe extern "C" fn($($arg),*) {}

        impl<$($arg: ReprC,)* R: ReprC> ReprFamily for unsafe extern "C" fn($($arg),*) -> R {
            type Kind = Self;
        }
        //impl<$($arg: ReprC,)*> ReprFamily for unsafe extern "C" fn($($arg),*) {
        //    type Kind = Self;
        //}
        impl<$($arg: ReprC,)* R: ReprC> crate::ExternC for unsafe extern "C" fn($($arg),*) -> R {
            type CType = Self;
        }
        impl<$($arg: ReprC,)*> crate::ExternC for unsafe extern "C" fn($($arg),*) {
            type CType = Self;
        }
        //impl<$($arg: ReprC,)* R: ReprC> crate::Encode for unsafe extern "C" fn($($arg),*) -> R {
        //    type Store = ();

        //    fn encode<'itm>(self, _: &mut ()) -> Self::CType where Self: 'itm {
        //        self
        //    }
        //}
        impl<$($arg: ReprC,)*> crate::Encode for unsafe extern "C" fn($($arg),*) {
            type Store = ();

            fn encode<'itm>(self, _: &mut ()) -> Self::CType where Self: 'itm {
                self
            }
        }

        unsafe impl<$($arg: ReprC,)* R: ReprC> ReprC for Option<unsafe extern "C" fn($($arg),*) -> R> {}
        unsafe impl<$($arg: ReprC),*> ReprC for Option<unsafe extern "C" fn($($arg),*)> {}
        //crate::mineral! { impl<$($arg: ReprC,)* R: ReprC> Robust for Option<unsafe extern "C" fn($($arg),*) -> R> {} }
        //crate::mineral! { impl<$($arg: ReprC),*> Robust for Option<unsafe extern "C" fn($($arg),*)> {} }
        )*
    }
}

impl_fn_types! {
    (),
    (A),
    (A, B),
    (A, B, C),
    (A, B, C, D),
    (A, B, C, D, E),
    (A, B, C, D, E, F),
    (A, B, C, D, E, F, G),
    (A, B, C, D, E, F, G, H),
    (A, B, C, D, E, F, G, H, I),
    (A, B, C, D, E, F, G, H, I, J),
    (A, B, C, D, E, F, G, H, I, J, K),
    (A, B, C, D, E, F, G, H, I, J, K, L),
}
