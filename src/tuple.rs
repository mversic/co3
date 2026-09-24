//! Provides `repr(C)` tuple types that can be safely passed across FFI boundaries.
//!
//! # Memory Layout
//!
//! Unlike Rust's native tuples, these have a guaranteed C-compatible
//! memory layout with fields in the order of declaration.
//!
//! # Niche Optimization
//!
//! When one of the tuple elements has a niche value (trap representations), `Option<(A, B, ...)>` is
//! optimized to use niche value of the **first element with a niche** to represent [`None`] value.
//! Values of all the other tuple elements of the niche are zeroed (NOT a stable guarantee yet)
//!
//! When none of the tuple elements have a niche value, `Option<(A, B, ...)>` is represented as a
//! `ReprCOption<ReprCTupleN<A, B, ...>>`.
//!
//! # Example
//!
//! ```rust
//! use core::mem::size_of;
//!
//! use co3::{encode, option::ReprCOption, tuple::ReprCTuple3, ExternC};
//!
//! type TupleWithNiche1<'a> = (u8, bool, &'a bool);
//! type TupleWithNiche2<'a> = (u8, &'a bool, bool);
//! type TupleWithoutNiche = (u64, u32, u8);
//!
//! assert_eq!(
//!     size_of::<<TupleWithNiche1 as ExternC>::CType>(),
//!     size_of::<<Option::<TupleWithNiche1> as ExternC>::CType>()
//! );
//! assert_eq!(
//!     size_of::<<TupleWithNiche2 as ExternC>::CType>(),
//!     size_of::<<Option::<TupleWithNiche2> as ExternC>::CType>());
//!
//! assert_eq!(
//!     8 + size_of::<<TupleWithoutNiche as ExternC>::CType>(),
//!     size_of::<<Option::<TupleWithoutNiche> as ExternC>::CType>()
//! );
//!
//! let none_value_1: Option<TupleWithNiche1> = None;
//! assert_eq!(encode(none_value_1), ReprCTuple3(0, 2, core::ptr::null()));
//!
//! let none_value_2: Option<TupleWithNiche2> = None;
//! assert_eq!(encode(none_value_2), ReprCTuple3(0, core::ptr::null(), 0));
//!
//! let none_value_3: Option<TupleWithoutNiche> = None;
//! assert_eq!(encode(none_value_3), ReprCOption::None());
//! ```

use disjoint_impls::disjoint_impls;
use rust_spec::{
    RustSpec,
    niche::{NicheStabilityKind, WithNiche, WithoutNiche},
};

use crate::{
    CFnArg, CFnReturn, Decode, Encode, ExternC, ReprC, Store,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    niche::Niche,
    slice::Unpack2,
    stored::{DecodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
};

macro_rules! impl_tuple {
    ( ($( $ty:ident ),+) -> $ffi_ty:ident ) => {
        impl<$($ty: Store),+> Store for ($($ty,)+) {
            #[expect(non_snake_case)]
            fn sync(self) -> Option<()> {
                let ($($ty,)+) = self;
                $( $ty.sync()?; )+
                Some(())
            }
        }

        unsafe impl<$($ty: Borrow),+> Borrow for ($($ty,)+) {
            type Borrowed<'itm>
                = ($( $ty::Borrowed<'itm>, )+)
            where
                Self: 'itm;

            type Owner = ($( $ty::Owner, )+);

            #[inline(always)]
            #[expect(non_snake_case)]
            fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                impl_tuple! {@decl_priv_store $($ty),+}

                let ($($ty,)+) = self;
                let owner: private_store::Store<$($ty::Owner),+> = owner.into();

                ($( $ty.borrow(owner.$ty), )+)
            }
        }
        unsafe impl<$($ty: Borrow),*> Borrow for $ffi_ty<$($ty),*> {
            type Borrowed<'itm>
                = $ffi_ty<$( $ty::Borrowed<'itm> ),+>
            where
                Self: 'itm;

            type Owner = ($( $ty::Owner, )+);

            #[inline(always)]
            #[expect(non_snake_case)]
            fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                impl_tuple! {@decl_priv_store $($ty),+}

                let $ffi_ty($($ty),*) = self;
                let owner: private_store::Store<$($ty::Owner),+> = owner.into();

                $ffi_ty($( $ty.borrow(owner.$ty) ),+)
            }
        }
        impl<'itm, $($ty: FromBorrow<'itm>),+> FromBorrow<'itm> for ($($ty,)+) {
            #[inline(always)]
            #[expect(non_snake_case)]
            fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
                let ($($ty,)+) = source;
                ($( $ty::from_borrow($ty), )+)
            }
        }
        impl<'itm, $($ty: FromBorrow<'itm>),+> FromBorrow<'itm> for $ffi_ty<$($ty),*> {
            #[inline(always)]
            #[expect(non_snake_case)]
            fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
                let $ffi_ty($($ty),+) = source;
                $ffi_ty($( $ty::from_borrow($ty) ),+)
            }
        }

        unsafe impl<$($ty: EncodeOwned<CType: Copy>),*> EncodeOwned for ($($ty,)*) {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
            where
                Self: 'itm
            {
                impl_tuple! {@decl_priv_store $($ty),*}

                let ($($ty,)*) = self;
                let store: private_store::Store<$($ty::Store,)*> = store.into();
                $ffi_ty($( $ty.soft_encode(store.$ty) ),*)
            }
        }
        unsafe impl<$($ty: EncodeOwned<CType: Copy>),*> EncodeOwned for $ffi_ty<$($ty),*> {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
            where
                Self: 'itm,
            {
                impl_tuple! {@decl_priv_store $($ty),*}

                let $ffi_ty($($ty),*) = self;
                let store: private_store::Store<$($ty::Store,)*> = store.into();
                $ffi_ty($( $ty.soft_encode(store.$ty) ),*)
            }
        }

        unsafe impl<'d, $($ty: DecodeOwned<'d, CType: Copy>),*> DecodeOwned<'d> for ($($ty,)*) {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                impl_tuple! {@decl_priv_store $($ty),*}

                let $ffi_ty($($ty),*) = source;
                let store: private_store::Store<$($ty::Store),*> = store.into();
                Some(unsafe { ($( $ty::soft_decode($ty, store.$ty)?, )*) })
            }
        }
        unsafe impl<'d, $($ty: DecodeOwned<'d, CType: Copy>),*> DecodeOwned<'d> for $ffi_ty<$($ty),*> {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                impl_tuple! {@decl_priv_store $($ty),*}

                let $ffi_ty($($ty),*) = source;
                let store: private_store::Store<$($ty::Store,)*> = store.into();
                Some(unsafe { $ffi_ty($( $ty::soft_decode($ty, store.$ty)? ),*) })
            }
        }

        impl<$($ty: Encode<CType: Copy>),*> Encode for ($($ty,)*) {}
        impl<$($ty: Encode<CType: Copy>),*> Encode for $ffi_ty<$($ty),*> {}

        impl<'d, $($ty: Decode<'d, CType: Copy>),*> Decode<'d> for ($($ty,)*) {}
        impl<'d, $($ty: Decode<'d, CType: Copy>),*> Decode<'d> for $ffi_ty<$($ty),*> {}

        unsafe impl<$($ty: ReprC + Copy),*> CFnArg for $ffi_ty<$($ty),*>
        where
            Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        {}
        unsafe impl<$($ty: ReprC + Copy),*> CFnReturn for $ffi_ty<$($ty),*>
        where
            Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        {}

        impl<$($ty),*> From<($( $ty, )*)> for $ffi_ty<$($ty),*> {
            #[expect(non_snake_case)]
            fn from(source: ($( $ty, )*)) -> Self {
                let ($($ty,)*) = source;
                Self($( $ty ),*)
            }
        }

        impl_tuple!(@split [] $($ty),+ -> $ffi_ty);
    };

    (@split [$($head:ident,)*] $last:ident -> $ffi_ty:ident) => {
        /// FFI-safe tuple with a stable `repr(C)` memory layout.
        ///
        /// See [the module level documentation](self) for more.
        #[repr(C)]
        #[derive(RustSpec, Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $ffi_ty<$($head,)* $last: ?Sized>($(pub $head,)* pub $last);

        impl<$($head: ExternC<CType: Sized>,)* $last: ExternC + ?Sized> ExternC for ($($head,)* $last,) {
            type CType = $ffi_ty<$($head::CType,)* $last::CType>;
        }
        impl<$($head: ExternC<CType: Sized>,)* $last: ExternC + ?Sized> ExternC for $ffi_ty<$($head,)* $last> {
            type CType = $ffi_ty<$($head::CType,)* $last::CType>;
        }

        unsafe impl<$($head: CheckedTransmute<CType: Copy>,)* $last: CheckedTransmute + ?Sized> CheckedTransmute for $ffi_ty<$($head,)* $last> {
            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                let $ffi_ty($($head,)* $last) = target;
                true $(&& unsafe { $head::is_valid($head) })* && unsafe { $last::is_valid($last) }
            }
        }

        unsafe impl<$($head: BorrowCast<AsConst: Sized>,)* $last: BorrowCast + ?Sized> BorrowCast for $ffi_ty<$($head,)* $last> {
            type AsConst = $ffi_ty<$($head::AsConst,)* $last::AsConst>;
        }

        unsafe impl<$($head: BorrowCastMut<AsMut: Sized>,)* $last: BorrowCastMut + ?Sized> BorrowCastMut for $ffi_ty<$($head,)* $last> {
            type AsMut = $ffi_ty<$($head::AsMut,)* $last::AsMut>;
        }

        unsafe impl<$($head: ReprC,)* $last: ReprC + ?Sized> ReprC for $ffi_ty<$($head,)* $last> {}

        unsafe impl<$($head: crate::stored::EmptyStore,)* $last: crate::stored::EmptyStore> crate::stored::EmptyStore for ($($head,)* $last,) {}
        unsafe impl<$($head: crate::stored::EmptyStore,)* $last: crate::stored::EmptyStore> crate::stored::EmptyStore for $ffi_ty<$($head,)* $last> {}
    };

    (@split [$($head:ident,)*] $next:ident, $($tail:ident),+ -> $ffi_ty:ident) => {
        impl_tuple!(@split [$($head,)* $next,] $($tail),+ -> $ffi_ty);
    };

    // NOTE: This is a trick to index tuples
    ( @decl_priv_store $( $ty:ident ),+) => {
        mod private_store {
            pub struct Store<'itm, $($ty),+> {
                $(pub $ty: &'itm mut $ty),+
            }

            impl<'itm, $($ty),+> From<&'itm mut ($($ty,)+)> for Store<'itm, $($ty,)+> {
                fn from(($($ty,)+): &'itm mut ($($ty,)+)) -> Self {
                    Self {$($ty,)+}
                }
            }
        }
    };
}

impl_tuple! {(A) -> ReprCTuple1}
impl_tuple! {(A, B) -> ReprCTuple2}
impl_tuple! {(A, B, C) -> ReprCTuple3}
impl_tuple! {(A, B, C, D) -> ReprCTuple4}
impl_tuple! {(A, B, C, D, E) -> ReprCTuple5}
impl_tuple! {(A, B, C, D, E, F) -> ReprCTuple6}
impl_tuple! {(A, B, C, D, E, F, G) -> ReprCTuple7}
impl_tuple! {(A, B, C, D, E, F, G, H) -> ReprCTuple8}
impl_tuple! {(A, B, C, D, E, F, G, H, I) -> ReprCTuple9}
impl_tuple! {(A, B, C, D, E, F, G, H, I, J) -> ReprCTuple10}
impl_tuple! {(A, B, C, D, E, F, G, H, I, J, K) -> ReprCTuple11}
impl_tuple! {(A, B, C, D, E, F, G, H, I, J, K, L) -> ReprCTuple12}

impl<A, B, Part1: ReprC, Part2: ReprC> Unpack2<Part1, Part2> for (A, B)
where
    Self: ExternC<CType = ReprCTuple2<Part1, Part2>>,
{
    type Error = core::convert::Infallible;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error> {
        Ok((value.0, value.1))
    }
}

impl<A: Niche> Niche for (A,) {
    const NICHE_VALUE: Self::CType = ReprCTuple1(A::NICHE_VALUE);
}

disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Niche: ExternC<CType: Copy> + Sized {
        const NICHE_VALUE: Self::CType;
    }

    impl<A: Niche<CType: Copy>, B: ExternC<CType: Copy>, N: NicheStabilityKind> Niche for (A, B)
    where
        A: RustSpec<Niche = WithNiche<N>>,
    {
        const NICHE_VALUE: Self::CType = ReprCTuple2(A::NICHE_VALUE, unsafe { core::mem::zeroed() });
    }

    impl<A: ExternC<CType: Copy>, B: Niche<CType: Copy>, N: NicheStabilityKind> Niche for (A, B)
    where
        A: RustSpec<Niche = WithoutNiche>,
        B: RustSpec<Niche = WithNiche<N>>,
    {
        const NICHE_VALUE: Self::CType = ReprCTuple2(unsafe { core::mem::zeroed() }, B::NICHE_VALUE);
    }
}

impl<A: ExternC<CType: Copy>, B: ExternC<CType: Copy>, C: ExternC<CType: Copy>> Niche for (A, B, C)
where
    (A, (B, C)): Niche<CType = ReprCTuple2<A::CType, <(B, C) as ExternC>::CType>>,
{
    const NICHE_VALUE: Self::CType = ReprCTuple3(
        <(A, (B, C))>::NICHE_VALUE.0,
        <(A, (B, C))>::NICHE_VALUE.1.0,
        <(A, (B, C))>::NICHE_VALUE.1.1,
    );
}

macro_rules! impl_tuple_niche_recursive {
    ($(($($all:ident),+) => ($left:ty, $right:ty) : $ffi_ty:ident($($field:tt),+)),+ $(,)?) => {
        $(
            impl<$($all: ExternC<CType: Copy>),+> Niche for ($($all,)+)
            where
                ($left, $right): Niche<CType = ReprCTuple2<<$left as ExternC>::CType, <$right as ExternC>::CType>>,
            {
                const NICHE_VALUE: Self::CType = $ffi_ty(
                    $(<($left, $right)>::NICHE_VALUE.$field),+
                );
            }
        )+
    };
}

impl_tuple_niche_recursive! {
    (A, B, C, D) => ((A, B), (C, D)) : ReprCTuple4(0.0, 0.1, 1.0, 1.1),
    (A, B, C, D, E) => ((A, B), (C, D, E)) : ReprCTuple5(0.0, 0.1, 1.0, 1.1, 1.2),
    (A, B, C, D, E, F) => ((A, B, C), (D, E, F)) : ReprCTuple6(0.0, 0.1, 0.2, 1.0, 1.1, 1.2),
    (A, B, C, D, E, F, G) => ((A, B, C), (D, E, F, G)) : ReprCTuple7(0.0, 0.1, 0.2, 1.0, 1.1, 1.2, 1.3),
    (A, B, C, D, E, F, G, H) => ((A, B, C, D), (E, F, G, H)) : ReprCTuple8(0.0, 0.1, 0.2, 0.3, 1.0, 1.1, 1.2, 1.3),
    (A, B, C, D, E, F, G, H, I) => ((A, B, C, D), (E, F, G, H, I)) : ReprCTuple9(0.0, 0.1, 0.2, 0.3, 1.0, 1.1, 1.2, 1.3, 1.4),
    (A, B, C, D, E, F, G, H, I, J) => ((A, B, C, D, E), (F, G, H, I, J)) : ReprCTuple10(0.0, 0.1, 0.2, 0.3, 0.4, 1.0, 1.1, 1.2, 1.3, 1.4),
    (A, B, C, D, E, F, G, H, I, J, K) => ((A, B, C, D, E), (F, G, H, I, J, K)) : ReprCTuple11(0.0, 0.1, 0.2, 0.3, 0.4, 1.0, 1.1, 1.2, 1.3, 1.4, 1.5),
    (A, B, C, D, E, F, G, H, I, J, K, L) => ((A, B, C, D, E, F), (G, H, I, J, K, L)) : ReprCTuple12(0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 1.0, 1.1, 1.2, 1.3, 1.4, 1.5)
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, vec::Vec};
    use core::num::NonZero as StdNonZero;

    use rust_spec::size::{Sized as Co3Sized, Zero};
    use static_assertions::assert_impl_all;
    #[cfg(feature = "alloc")]
    use static_assertions::assert_not_impl_any;

    use super::*;
    use crate::{Decode, Encode, option::ReprCOption};
    #[cfg(feature = "alloc")]
    use crate::{
        boxed::{CBox, CBoxedSlice},
        slice::{CSlice, CSliceMut},
        stored::{DecodeOwned, EncodeOwned},
    };

    #[test]
    fn tuple_size_family_tracks_zst_fields() {
        assert_impl_all!(ReprCTuple2<(), ()>: RustSpec<Size = Co3Sized<Zero>>);
        assert_impl_all!(ReprCTuple3<(), (), ()>: RustSpec<Size = Co3Sized<Zero>>);
    }

    #[test]
    fn tuple_size_family_tracks_non_zst_fields() {
        assert_impl_all!(ReprCTuple2<(), u8>: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>);
        assert_impl_all!(ReprCTuple2<u8, ()>: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>);
        assert_impl_all!(ReprCTuple3<(), u8, ()>: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>);
    }

    #[test]
    fn stored_tuple_3_without_niche() {
        assert_impl_all!((u8, u8, u8):
            ExternC<CType = ReprCTuple3<u8, u8, u8>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(&(u8, u8, u8):
            Niche<CType = *const ReprCTuple3<u8, u8, u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut (u8, u8, u8):
        Niche<CType = *mut ReprCTuple3<u8, u8, u8>>,
        Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<(u8, u8, u8)>:
            Niche<CType = CBox<ReprCTuple3<u8, u8, u8>>>,
            EncodeOwned,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&[(u8, u8, u8)]:
            Niche<CType = CSlice<ReprCTuple3<u8, u8, u8>>>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [(u8, u8, u8)]:
            Niche<CType = CSliceMut<ReprCTuple3<u8, u8, u8>>>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[(u8, u8, u8)]>:
            Niche<CType = CBoxedSlice<ReprCTuple3<u8, u8, u8>>>,
            EncodeOwned,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, u8, u8)>:
            Niche<CType = CBoxedSlice<ReprCTuple3<u8, u8, u8>>>,
            DecodeOwned<'static>,
            EncodeOwned,
        );
        assert_impl_all!([(u8, u8, u8); 2]:
            ExternC<CType = [ReprCTuple3<u8, u8, u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<(u8, u8, u8)>:
            Niche<CType = ReprCOption<ReprCTuple3<u8, u8, u8>>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<[(u8, u8, u8)]>: Encode, Decode<'static>);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Vec<(u8, u8, u8)>: Encode, Decode<'static>);
    }

    #[test]
    fn stored_tuple_3_with_niche() {
        // NOTE: Confirms niche is taken from the first available element
        assert_eq!(
            <(u8, StdNonZero<u8>, bool)>::NICHE_VALUE,
            ReprCTuple3(0, 0, 0)
        );

        assert_impl_all!((u8, StdNonZero<u8>, bool):
            Niche<CType = ReprCTuple3<u8, u8, u8>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(&(u8, StdNonZero<u8>, bool):
            Niche<CType = *const ReprCTuple3<u8, u8, u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut (u8, StdNonZero<u8>, bool):
            Niche<CType = *mut ReprCTuple3<u8, u8, u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<(u8, StdNonZero<u8>, bool)>:
            Niche<CType = CBox<ReprCTuple3<u8, u8, u8>>>,
            DecodeOwned<'static>,
            EncodeOwned,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&[(u8, StdNonZero<u8>, bool)]:
            Niche<CType = CSlice<ReprCTuple3<u8, u8, u8>>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [(u8, StdNonZero<u8>, bool)]:
            Niche<CType = CSliceMut<ReprCTuple3<u8, u8, u8>>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[(u8, StdNonZero<u8>, bool)]>:
            Niche<CType = CBoxedSlice<ReprCTuple3<u8, u8, u8>>>,
            DecodeOwned<'static>,
            EncodeOwned,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, StdNonZero<u8>, bool)>:
            Niche<CType = CBoxedSlice<ReprCTuple3<u8, u8, u8>>>,
            DecodeOwned<'static>,
            EncodeOwned,
        );
        assert_impl_all!([(u8, StdNonZero<u8>, bool); 2]:
            Niche<CType = [ReprCTuple3<u8, u8, u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<(u8, StdNonZero<u8>, bool)>:
            // TODO: Depends on: https://github.com/mversic/co3/issues/33
            //Niche<CType = ReprCTuple3<u8, u8, u8>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<[(u8, StdNonZero<u8>, u8)]>: Encode, Decode<'static>);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Vec<(u8, StdNonZero<u8>, u8)>: Encode, Decode<'static>);
    }
}
