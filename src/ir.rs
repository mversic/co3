//! Internal Representation (IR) of Rust types during conversion into FFI types.
//!
//! While you can implement [`crate::ExternC`] directly on your type, it is often
//! preferable to map it into IR by implementing [`Ir`]. This approach gives you
//! automatic, correct, and zero-cost conversions from IR to the equivalent C type.
#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};

use disjoint_impls::disjoint_impls;

use crate::{
    FnArg, ReprC,
    dst::{DstFamily, ExternTypeLike, Sized_, SliceLike, TraitObjectLike},
    niche::{NicheFamily, WithCustomNiche, WithStableNiche, WithoutNiche},
};

/// Marker for a [`ReprFamily`] type that delegates to the pointed-to type when converting
/// the likes of `&Self` or `&[Self]` into an FFI-compatible representation
///
/// This type clones the pointed-to value to get owned value that has implemented
/// [`ExternC`]. This type therefore uses the store
pub trait Cloned {}

/// Marker for a type that can is transmuted to another type and thus delegates it's conversion
pub enum Transmuted {}

/// Marker for a robust [`ReprC`] type that does not require conversion
pub enum Robust {}

/// Marker for a type exported as an opaque pointer over FFI.
pub enum Opaque {}

pub(crate) trait NonRobust {}
impl NonRobust for Transmuted {}
impl NonRobust for Opaque {}
impl<S: Cloned> NonRobust for S {}

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
        type Kind: ?Sized;
    }

    impl<R: ReprFamily<Kind = Robust>> ReprFamily for [R] {
        type Kind = Robust;
    }
    impl<R: ReprFamily<Kind = Opaque>> ReprFamily for [R] {
        type Kind = [Opaque];
    }
    impl<R: ReprFamily<Kind = Transmuted>> ReprFamily for [R] {
        type Kind = Transmuted;
    }
    impl<R: ReprFamily<Kind: Cloned + Sized>> ReprFamily for [R] {
        type Kind = [R::Kind];
    }

    impl<R: ReprFamily<Kind = Robust> + DstFamily<Kind = Sized_>> ReprFamily for &R {
        type Kind = Transmuted;
    }
    impl<'a, R: ReprFamily<Kind = Robust> + DstFamily<Kind = SliceLike> + ?Sized> ReprFamily for &'a R {
        type Kind = &'a Robust;
    }
    impl<R: ReprFamily<Kind = Opaque> + DstFamily<Kind = Sized_>> ReprFamily for &R {
        type Kind = Transmuted;
    }
    impl<'a, R: ReprFamily<Kind = Opaque> + DstFamily<Kind = SliceLike> + ?Sized> ReprFamily for &'a R {
        type Kind = &'a Opaque;
    }
    impl<'a, R: ReprFamily<Kind = Opaque> + DstFamily<Kind = TraitObjectLike> + ?Sized> ReprFamily for &'a R {
        type Kind = &'a Opaque;
    }
    impl<R: ReprFamily<Kind = Transmuted> + DstFamily<Kind = Sized_>> ReprFamily for &R {
        type Kind = Transmuted;
    }
    impl<'a, R: ReprFamily<Kind = Transmuted> + DstFamily<Kind = SliceLike> + ?Sized> ReprFamily for &'a R {
        type Kind = &'a Transmuted;
    }
    impl<'a, R: ReprFamily<Kind = Transmuted> + DstFamily<Kind = ExternTypeLike>> ReprFamily for &'a R {
        type Kind = &'a Transmuted;
    }
    impl<'a, R: ReprFamily<Kind: Cloned + 'a> + DstFamily<Kind = Sized_>> ReprFamily for &'a R {
        type Kind = &'a <R as ReprFamily>::Kind;
    }
    impl<'a, R: ReprFamily<Kind: Cloned + 'a> + DstFamily<Kind = SliceLike> + ?Sized> ReprFamily for &'a R {
        type Kind = &'a <R as ReprFamily>::Kind;
    }

    impl<R: ReprFamily<Kind = Robust> + DstFamily<Kind = Sized_>> ReprFamily for &mut R {
        type Kind = Transmuted;
    }
    impl<'a, R: ReprFamily<Kind = Robust> + DstFamily<Kind = SliceLike> + ?Sized> ReprFamily for &'a mut R {
        type Kind = &'a mut Robust;
    }
    impl<R: ReprFamily<Kind = Opaque> + DstFamily<Kind = Sized_>> ReprFamily for &mut R {
        type Kind = Transmuted;
    }
    impl<'a, R: ReprFamily<Kind = Opaque> + DstFamily<Kind = SliceLike> + ?Sized> ReprFamily for &'a mut R {
        type Kind = &'a mut Opaque;
    }
    impl<'a, R: ReprFamily<Kind = Opaque> + DstFamily<Kind = TraitObjectLike> + ?Sized> ReprFamily for &'a mut R {
        type Kind = &'a mut Opaque;
    }
    impl<R: ReprFamily<Kind = Transmuted> + DstFamily<Kind = Sized_>> ReprFamily for &mut R {
        type Kind = Transmuted;
    }
    impl<'a, R: ReprFamily<Kind = Transmuted> + DstFamily<Kind = SliceLike> + ?Sized> ReprFamily for &'a mut R {
        type Kind = &'a mut Transmuted;
    }
    impl<'a, R: ReprFamily<Kind = Transmuted> + DstFamily<Kind = ExternTypeLike>> ReprFamily for &'a mut R {
        type Kind = &'a mut Transmuted;
    }
    impl<'a, R: ReprFamily<Kind: Cloned + 'a> + DstFamily<Kind = Sized_>> ReprFamily for &'a mut R {
        type Kind = &'a mut <R as ReprFamily>::Kind;
    }
    impl<'a, R: ReprFamily<Kind: Cloned + 'a> + DstFamily<Kind = SliceLike> + ?Sized> ReprFamily for &'a mut R {
        type Kind = &'a mut <R as ReprFamily>::Kind;
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Robust> + DstFamily<Kind = Sized_>> ReprFamily for Box<R> {
        type Kind = Transmuted;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Robust> + DstFamily<Kind = SliceLike> + ?Sized> ReprFamily for Box<R> {
        type Kind = Box<Robust>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Opaque> + DstFamily<Kind = Sized_>> ReprFamily for Box<R> {
        type Kind = Transmuted;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Opaque> + DstFamily<Kind = SliceLike> + ?Sized> ReprFamily for Box<R> {
        type Kind = Box<Opaque>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Opaque> + DstFamily<Kind = TraitObjectLike> + ?Sized> ReprFamily for Box<R> {
        type Kind = Box<Opaque>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted> + DstFamily<Kind = Sized_>> ReprFamily for Box<R> {
        type Kind = Transmuted;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted> + DstFamily<Kind = SliceLike> + ?Sized> ReprFamily for Box<R> {
        type Kind = Box<Transmuted>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted> + DstFamily<Kind = ExternTypeLike>> ReprFamily for Box<R> {
        type Kind = Box<Transmuted>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind: Cloned> + DstFamily<Kind = Sized_>> ReprFamily for Box<R> {
        type Kind = Box<<R as ReprFamily>::Kind>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind: Cloned> + DstFamily<Kind = SliceLike> + ?Sized> ReprFamily for Box<R> {
        type Kind = Box<<R as ReprFamily>::Kind>;
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Robust>> ReprFamily for Vec<R> {
        type Kind = Vec<Robust>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Opaque>> ReprFamily for Vec<R> {
        // FIXME: Add a test to verify this type indeed works
        // This should delegate to Box<[R]>
        type Kind = Vec<Opaque>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted>> ReprFamily for Vec<R> {
        type Kind = Vec<Transmuted>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind: Cloned + Sized>> ReprFamily for Vec<R> {
        type Kind = Vec<R::Kind>;
    }

    impl<R: ReprFamily<Kind = Robust>, const N: usize> ReprFamily for [R; N] {
        type Kind = Robust;
    }
    impl<R: ReprFamily<Kind = Opaque>, const N: usize> ReprFamily for [R; N] {
        type Kind = [Opaque; N];
    }
    impl<R: ReprFamily<Kind = Transmuted>, const N: usize> ReprFamily for [R; N] {
        type Kind = Transmuted;
    }
    impl<R: ReprFamily<Kind: Cloned + Sized>, const N: usize> ReprFamily for [R; N] {
        type Kind = [R::Kind; N];
    }

    impl<R: ReprFamily<Kind = Robust>> ReprFamily for Option<R> {
        type Kind = Option<WithoutNiche>;
    }
    impl<R: ReprFamily<Kind = Opaque>> ReprFamily for Option<R> {
        type Kind = Option<WithCustomNiche>;
    }
    impl<R: ReprFamily<Kind = Transmuted> + NicheFamily<Kind = WithoutNiche>> ReprFamily for Option<R> {
        type Kind = Option<WithoutNiche>;
    }
    impl<R: ReprFamily<Kind = Transmuted> + NicheFamily<Kind = WithCustomNiche>> ReprFamily for Option<R> {
        type Kind = Option<WithCustomNiche>;
    }
    impl<R: ReprFamily<Kind = Transmuted> + NicheFamily<Kind = WithStableNiche>> ReprFamily for Option<R> {
        type Kind = Transmuted;
    }
    impl<R: ReprFamily<Kind: Cloned> + NicheFamily<Kind = WithoutNiche>> ReprFamily for Option<R> {
        type Kind = Option<WithoutNiche>;
    }
    impl<R: ReprFamily<Kind: Cloned> + NicheFamily<Kind = WithCustomNiche>> ReprFamily for Option<R> {
        type Kind = Option<WithCustomNiche>;
    }
    // TODO: Make a test. I think the example type is &Box<u8>
    // Should it become Cloned in this case?
    //#[cfg(any(feature = "owned_types", feature = "cloned_refs"))]
    //impl<R: ReprFamily<Kind: Cloned> + NicheFamily<Kind = WithStableNiche>> ReprFamily for Option<R> {
    //    type Kind = Option<WithCustomNiche>;
    //}
}

impl<S> Cloned for [S] {}
impl<S: ?Sized> Cloned for &S {}
impl<S: ?Sized> Cloned for &mut S {}
#[cfg(feature = "alloc")]
impl<S: ?Sized> Cloned for Box<S> {}
#[cfg(feature = "alloc")]
impl<S> Cloned for Vec<S> {}
impl<S, const N: usize> Cloned for [S; N] {}
impl Cloned for Option<WithoutNiche> {}
impl Cloned for Option<WithCustomNiche> {}

macro_rules! impl_fn_types {
    ( $( ( $( $arg:ident ),* ) ),* $(,)? ) => {$(
        // FIXME: I'm not sure if arguments are required to be ReprC, what if fn pointer is opaque?
        // or should we create new function with argument conversion?
        unsafe impl<$($arg: FnArg,)* R: FnArg> ReprC for unsafe extern "C" fn($($arg),*) -> R {}
        unsafe impl<$($arg: FnArg,)*> ReprC for unsafe extern "C" fn($($arg),*) {}

        unsafe impl<$($arg: FnArg,)* R: FnArg> FnArg for unsafe extern "C" fn($($arg),*) -> R {}
        unsafe impl<$($arg: FnArg,)*> FnArg for unsafe extern "C" fn($($arg),*) {}

        impl<$($arg,)* R> ReprFamily for unsafe extern "C" fn($($arg),*) -> R {
            type Kind = Self;
        }
        //impl<$($arg),*> ReprFamily for unsafe extern "C" fn($($arg),*) {
        //    type Kind = Self;
        //}

        impl<$($arg: FnArg,)* R: FnArg> crate::ExternC for unsafe extern "C" fn($($arg),*) -> R {
            type CType = Self;
        }
        impl<$($arg: FnArg,)*> crate::ExternC for unsafe extern "C" fn($($arg),*) {
            type CType = Self;
        }
        //impl<const AS_REF: bool, $($arg: FnArg,)* R: FnArg> crate::Encode<false> for unsafe extern "C" fn($($arg),*) -> R {
        //    type Store = ();

        //    fn encode<'itm>(self, _: &mut ()) -> Self::CType where Self: 'itm {
        //        self
        //    }
        //}
        impl<$($arg: FnArg,)*> crate::EncodeWithStore<false> for unsafe extern "C" fn($($arg),*) {
            type Store = ();

            #[inline(always)]
            fn encode<'itm>(self, _: &mut ()) -> Self::CType where Self: 'itm {
                self
            }
        }

        unsafe impl<$($arg: FnArg,)* R: FnArg> ReprC for Option<unsafe extern "C" fn($($arg),*) -> R> {}
        //unsafe impl<$($arg: FnArg),*> ReprC for Option<unsafe extern "C" fn($($arg),*)> {}
        //crate::reprC! { impl<$($arg: FnArg,)* R: FnArg> SizedRobust for Option<unsafe extern "C" fn($($arg),*) -> R> {} }
        //crate::reprC! { impl<$($arg: FnArg),*> SizedRobust for Option<unsafe extern "C" fn($($arg),*)> {} }
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

#[cfg(test)]
mod tests {
    use static_assertions::assert_impl_all;

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::{CBox, CBoxedSlice};
    use crate::{
        DecodeWithStore, EncodeWithStore, ExternC,
        niche::{Niche, WithStableNiche},
    };

    #[test]
    fn opaque_collections_are_cloned() {
        #[derive(Clone, PartialEq, Eq)]
        struct OpaqueData {
            value: i32,
        }

        impl DstFamily for OpaqueData {
            type Kind = Sized_;
        }
        impl ReprFamily for OpaqueData {
            type Kind = Opaque;
        }
        impl NicheFamily for OpaqueData {
            type Kind = WithCustomNiche;
        }

        #[cfg(feature = "alloc")]
        assert_impl_all!(OpaqueData:
            ReprFamily<Kind = Opaque>,
            NicheFamily<Kind = WithCustomNiche>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );

        assert_impl_all!(&OpaqueData:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            Niche<CType = *const OpaqueData>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );

        assert_impl_all!(&mut OpaqueData:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            Niche<CType = *mut OpaqueData>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );

        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<OpaqueData>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    Niche<CType = CBox<OpaqueData>>,
        //    Decode<'static>,
        //    Encode,
        //);

        //#[cfg(feature = "alloc")]
        //assert_impl_all!(&[OpaqueData]:
        //    ReprFamily<Kind = &'static [Opaque]>,
        //    NicheFamily<Kind = WithCustomNiche>,
        //    Niche<CType = CSlice<CBox<OpaqueData>>>,
        //    Decode<'static>,
        //    Encode,
        //);

        //#[cfg(feature = "alloc")]
        //assert_impl_all!(&mut [OpaqueData]:
        //    ReprFamily<Kind = &'static mut [Opaque]>,
        //    NicheFamily<Kind = WithCustomNiche>,
        //    Niche<CType = CSliceMut<CBox<OpaqueData>>>,
        //    Decode<'static>,
        //    Encode,
        //);

        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[OpaqueData]>:
            ReprFamily<Kind = Box<[Opaque]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<CBox<OpaqueData>>>,
            // FIXME:
            //Decode<'static>,
            EncodeWithStore,
        );

        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<OpaqueData>:
            ReprFamily<Kind = Vec<Box<Opaque>>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<CBox<OpaqueData>>>,
            // FIXME:
            //Decode<'static>,
            EncodeWithStore,
        );

        #[cfg(feature = "alloc")]
        assert_impl_all!([OpaqueData; 2]:
            ReprFamily<Kind = [Opaque; 2]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [CBox<OpaqueData>; 2]>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );

        #[cfg(feature = "alloc")]
        assert_impl_all!(Option<OpaqueData>:
            ReprFamily<Kind = Option<WithCustomNiche>>,
            // FIXME:
            //NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = CBox<OpaqueData>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
    }
}
