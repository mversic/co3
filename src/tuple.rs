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
//! When none of the tuple elements have a niche value [`Option<(A, B, ...)>`] is represented as a
//! [`COption<CTupleN<A, B, ...>>`]
//!
//! # Example
//!
//! ```rust
//! use core::mem::size_of;
//!
//! use co3::{option::COption, tuple::CTuple3, ExternC, SoftEncode};
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
//! let none_value_2: Option<TupleWithNiche2> = None;
//! let none_value_3: Option<TupleWithoutNiche> = None;
//!
//! let mut store1 = Default::default();
//! let mut store2 = Default::default();
//! let mut store3 = Default::default();
//!
//! assert_eq!(
//!     none_value_1.encode(&mut store1),
//!     CTuple3(0, 2, core::ptr::null())
//! );
//! assert_eq!(
//!     none_value_2.encode(&mut store2),
//!     CTuple3(0, core::ptr::null(), 0)
//! );
//! assert_eq!(
//!     none_value_3.encode(&mut store3),
//!     COption::None()
//! );
//! ```

use core::ops::Add;

use crate::{
    ExternC, ReprC, SoftDecode, SoftEncode, Store,
    borrow::{Borrow, BorrowCast, ToOwned},
    handle::Erase,
    niche::{Niche, NicheFamily, WithNiche, WithoutNiche},
    size::SizeFamily,
    stored::{SoftDecodeOwned, SoftEncodeOwned},
};

macro_rules! impl_tuple {
    ( ($( $ty:ident ),+) -> $ffi_ty:ident ) => {
        unsafe impl<$($ty: crate::out_ptr::Zst),+> crate::out_ptr::Zst for ($($ty,)+) {}

        crate::reprC! {
            impl($($ty),+) Stored for ($($ty,)+) {}
        }

        impl<$($ty: Store),+> Store for ($($ty,)+) {
            #[expect(non_snake_case)]
            fn sync(self) -> Option<()> {
                let ($($ty,)+) = self;
                $( $ty.sync()?; )+
                Some(())
            }
        }

        impl<$($ty: ExternC<CType: BorrowCast + Copy> + Borrow),+> Borrow for ($($ty,)+)
        where
            Self: ExternC<CType = $ffi_ty<$($ty::CType),+>>,
        {
            type Borrowed<'itm>
                = ($( $ty::Borrowed<'itm>, )+)
            where
                Self: 'itm;

            type Owner = ($( $ty::Owner, )+);

            #[inline(always)]
            #[expect(non_snake_case)]
            fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                impl_tuple! {@decl_priv_store $($ty),+}

                let ($($ty,)+) = self;
                let store: private_store::Store<$($ty::Owner),+> = store.into();

                ($( $ty.borrow(store.$ty), )+)
            }
        }

        impl<'itm, $($ty: ExternC<CType: BorrowCast + Copy> + ToOwned<'itm>),+> ToOwned<'itm> for ($($ty,)+)
        where
            Self: ExternC<CType = $ffi_ty<$($ty::CType),+>>,
        {
            #[inline(always)]
            #[expect(non_snake_case)]
            fn to_owned(source: Self::Borrowed<'itm>) -> Self {
                let ($($ty,)+) = source;
                ($( $ty::to_owned($ty), )+)
            }
        }

        impl<$($ty: SoftEncode),*> SoftEncode for ($($ty,)*) {}
        impl<$($ty: SoftEncodeOwned),*> SoftEncodeOwned for ($($ty,)*) {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            fn encode<'itm>(self, store: &mut Self::Store) -> Self::CType where Self: 'itm {
                impl_tuple! {@decl_priv_store $($ty),*}

                let ($($ty,)*) = self;
                let store: private_store::Store<$($ty::Store,)*> = store.into();
                $ffi_ty($( $ty::encode($ty, store.$ty), )*)
            }
        }

        impl<'d, $($ty: SoftDecode<'d, CType: Copy>),*> SoftDecode<'d> for ($($ty,)*) {}
        impl<'d, $($ty: SoftDecodeOwned<'d, CType: Copy>),*> SoftDecodeOwned<'d> for ($($ty,)*) {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                impl_tuple! {@decl_priv_store $($ty),*}

                let $ffi_ty($($ty),*) = source;
                let store: private_store::Store<$($ty::Store),*> = store.into();
                Some(unsafe { ($( SoftDecodeOwned::decode($ty, store.$ty)?, )*) })
            }
        }

        unsafe impl<$($ty: Erase<Erased: Sized>),*> Erase for ($($ty,)*) {
            type Erased = ($($ty::Erased,)*);
        }

        impl_tuple!(@split [] $($ty),+ -> $ffi_ty);
    };

    (@split [$($head:ident,)*] $last:ident -> $ffi_ty:ident) => {
        /// FFI-safe tuple with a stable `repr(C)` memory layout.
        ///
        /// See [the module level documentation](self) for more.
        #[repr(C)]
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $ffi_ty<$($head,)* $last: ?Sized>($(pub $head,)* pub $last);

        impl<$($head,)* $last: SizeFamily> SizeFamily for ($($head,)* $last,) {
            type Kind = $last::Kind;
        }

        crate::reprC! {
            unsafe impl($($head: ReprC + Copy,)* $last: ReprC + ?Sized) Robust for $ffi_ty<$($head,)* $last> {}
        }

        unsafe impl<
            $($head: BorrowCast<AsConst: Copy, AsMut: Copy> + Copy,)*
            $last: BorrowCast<AsConst: Copy, AsMut: Copy> + Copy
        > BorrowCast for $ffi_ty<$($head,)* $last> {
            type AsConst = $ffi_ty<$($head::AsConst,)* $last::AsConst>;
            type AsMut = $ffi_ty<$($head::AsMut,)* $last::AsMut>;
        }

        impl<$($head: ReprC + Copy,)* $last: ReprC + Copy> From<($( $head, )* $last,)> for $ffi_ty<$($head,)* $last> {
            #[expect(non_snake_case)]
            fn from(source: ($( $head, )* $last,)) -> Self {
                let ($($head,)* $last,) = source;
                Self($( $head, )* $last)
            }
        }

        impl<$($head: ExternC<CType: Copy>,)* $last: ExternC + ?Sized> ExternC for ($($head,)* $last,) {
            type CType = $ffi_ty<$($head::CType,)* $last::CType>;
        }
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

    // NOTE: This is a trick to index tuples
    ( @decl_priv_out_ptr $( $ty:ident ),+ $(,)? ) => {
        mod private_out_ptr {
            pub struct OutPtr<'itm, $($ty),+> {
                $(pub $ty: &'itm mut core::mem::MaybeUninit::<$ty>),+
            }

            impl<'itm, $($ty),+> From<&'itm mut ($(core::mem::MaybeUninit::<$ty>,)+)> for OutPtr<'itm, $($ty),+> {
                fn from(($($ty,)+): &'itm mut ($(core::mem::MaybeUninit::<$ty>,)+)) -> Self {
                    Self {$($ty,)+}
                }
            }
        }
    };
}

impl_tuple! {(A) -> CTuple1}
impl_tuple! {(A, B) -> CTuple2}
impl_tuple! {(A, B, C) -> CTuple3}
impl_tuple! {(A, B, C, D) -> CTuple4}
impl_tuple! {(A, B, C, D, E) -> CTuple5}
impl_tuple! {(A, B, C, D, E, F) -> CTuple6}
impl_tuple! {(A, B, C, D, E, F, G) -> CTuple7}
impl_tuple! {(A, B, C, D, E, F, G, H) -> CTuple8}
impl_tuple! {(A, B, C, D, E, F, G, H, I) -> CTuple9}
impl_tuple! {(A, B, C, D, E, F, G, H, I, J) -> CTuple10}
impl_tuple! {(A, B, C, D, E, F, G, H, I, J, K) -> CTuple11}
impl_tuple! {(A, B, C, D, E, F, G, H, I, J, K, L) -> CTuple12}

impl<A: Niche> Niche for (A,) {
    const NICHE_VALUE: Self::CType = CTuple1(A::NICHE_VALUE);
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<A: Niche, B: ExternC> Niche for (A, B)
    where
        A: NicheFamily<Kind: WithNiche>,
        <A as ExternC>::CType: Copy,
        <B as ExternC>::CType: Copy,
    {
        const NICHE_VALUE: Self::CType = CTuple2(A::NICHE_VALUE, unsafe { core::mem::zeroed() });
    }

    impl<A: ExternC, B: Niche> Niche for (A, B)
    where
        A: NicheFamily<Kind = WithoutNiche>,
        B: NicheFamily<Kind: WithNiche>,
        <A as ExternC>::CType: Copy,
        <B as ExternC>::CType: Copy,
    {
        const NICHE_VALUE: Self::CType = CTuple2(unsafe { core::mem::zeroed() }, B::NICHE_VALUE);
    }
}

impl<A: ExternC<CType: Copy>, B: ExternC<CType: Copy>, C: ExternC<CType: Copy>> Niche for (A, B, C)
where
    (A, (B, C)): Niche<CType = CTuple2<A::CType, <(B, C) as ExternC>::CType>>,
{
    const NICHE_VALUE: Self::CType = CTuple3(
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
                ($left, $right): Niche<CType = CTuple2<<$left as ExternC>::CType, <$right as ExternC>::CType>>,
            {
                const NICHE_VALUE: Self::CType = $ffi_ty(
                    $(<($left, $right)>::NICHE_VALUE.$field),+
                );
            }
        )+
    };
}

impl_tuple_niche_recursive! {
    (A, B, C, D) => ((A, B), (C, D)) : CTuple4(0.0, 0.1, 1.0, 1.1),
    (A, B, C, D, E) => ((A, B), (C, D, E)) : CTuple5(0.0, 0.1, 1.0, 1.1, 1.2),
    (A, B, C, D, E, F) => ((A, B, C), (D, E, F)) : CTuple6(0.0, 0.1, 0.2, 1.0, 1.1, 1.2),
    (A, B, C, D, E, F, G) => ((A, B, C), (D, E, F, G)) : CTuple7(0.0, 0.1, 0.2, 1.0, 1.1, 1.2, 1.3),
    (A, B, C, D, E, F, G, H) => ((A, B, C, D), (E, F, G, H)) : CTuple8(0.0, 0.1, 0.2, 0.3, 1.0, 1.1, 1.2, 1.3),
    (A, B, C, D, E, F, G, H, I) => ((A, B, C, D), (E, F, G, H, I)) : CTuple9(0.0, 0.1, 0.2, 0.3, 1.0, 1.1, 1.2, 1.3, 1.4),
    (A, B, C, D, E, F, G, H, I, J) => ((A, B, C, D, E), (F, G, H, I, J)) : CTuple10(0.0, 0.1, 0.2, 0.3, 0.4, 1.0, 1.1, 1.2, 1.3, 1.4),
    (A, B, C, D, E, F, G, H, I, J, K) => ((A, B, C, D, E), (F, G, H, I, J, K)) : CTuple11(0.0, 0.1, 0.2, 0.3, 0.4, 1.0, 1.1, 1.2, 1.3, 1.4, 1.5),
    (A, B, C, D, E, F, G, H, I, J, K, L) => ((A, B, C, D, E, F), (G, H, I, J, K, L)) : CTuple12(0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 1.0, 1.1, 1.2, 1.3, 1.4, 1.5)
}

macro_rules! impl_tuple_family_recursive {
    ($(($($all:ident),+) => $split:ty : $family:ident),+ $(,)?) => {
        $(
            impl<$($all),+> $family for ($($all,)+)
            where
                $split: $family,
            {
                type Kind = <$split as $family>::Kind;
            }
        )+
    };
}

impl<A: NicheFamily> NicheFamily for (A,) {
    type Kind = A::Kind;
}

impl<A: NicheFamily<Kind: Add<B::Kind>>, B: NicheFamily> NicheFamily for (A, B) {
    type Kind = <A::Kind as Add<B::Kind>>::Output;
}

impl_tuple_family_recursive! {
    (A, B, C) => (A, (B, C)) : NicheFamily,
    (A, B, C, D) => ((A, B), (C, D)) : NicheFamily,
    (A, B, C, D, E) => ((A, B), (C, D, E)) : NicheFamily,
    (A, B, C, D, E, F) => ((A, B, C), (D, E, F)) : NicheFamily,
    (A, B, C, D, E, F, G) => ((A, B, C), (D, E, F, G)) : NicheFamily,
    (A, B, C, D, E, F, G, H) => ((A, B, C, D), (E, F, G, H)) : NicheFamily,
    (A, B, C, D, E, F, G, H, I) => ((A, B, C, D), (E, F, G, H, I)) : NicheFamily,
    (A, B, C, D, E, F, G, H, I, J) => ((A, B, C, D, E), (F, G, H, I, J)) : NicheFamily,
    (A, B, C, D, E, F, G, H, I, J, K) => ((A, B, C, D, E), (F, G, H, I, J, K)) : NicheFamily,
    (A, B, C, D, E, F, G, H, I, J, K, L) => ((A, B, C, D, E, F), (G, H, I, J, K, L)) : NicheFamily
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc_crate::{boxed::Box, vec::Vec};
    use core::num::NonZeroU8;

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::{CBox, CBoxedSlice};
    use crate::{
        CSlice, CSliceMut, Decode, Encode,
        ir::ReprFamily,
        niche::{StableNiche, WithCustomNiche, WithStableNiche},
        option::COption,
    };

    #[test]
    fn stored_tuple_3_without_niche() {
        assert_impl_all!((u8, u8, u8):
            ReprFamily<Kind = (u8, u8, u8)>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = CTuple3<u8, u8, u8>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(&(u8, u8, u8):
            ReprFamily<Kind = &'static (u8, u8, u8)>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const CTuple3<u8, u8, u8>>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&mut (u8, u8, u8):
            ReprFamily<Kind = &'static mut (u8, u8, u8)>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut CTuple3<u8, u8, u8>>,
            SoftDecode<'static>,
            SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<(u8, u8, u8)>:
            ReprFamily<Kind = Box<(u8, u8, u8)>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<CTuple3<u8, u8, u8>>>,
            SoftDecodeOwned<'static>,
            SoftEncodeOwned,
        );
        assert_impl_all!(&[(u8, u8, u8)]:
            ReprFamily<Kind = &'static [(u8, u8, u8)]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<CTuple3<u8, u8, u8>>>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&mut [(u8, u8, u8)]:
            ReprFamily<Kind = &'static mut [(u8, u8, u8)]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<CTuple3<u8, u8, u8>>>,
            // FIXME:
            //SoftDecode<'static>,
            //SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[(u8, u8, u8)]>:
            ReprFamily<Kind = Box<[(u8, u8, u8)]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<CTuple3<u8, u8, u8>>>,
            SoftDecodeOwned<'static>,
            SoftEncodeOwned,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, u8, u8)>:
            ReprFamily<Kind = Vec<(u8, u8, u8)>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<CTuple3<u8, u8, u8>>>,
            SoftDecodeOwned<'static>,
            SoftEncodeOwned,
        );
        assert_impl_all!([(u8, u8, u8); 2]:
            ReprFamily<Kind = [(u8, u8, u8); 2]>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = [CTuple3<u8, u8, u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<(u8, u8, u8)>:
            ReprFamily<Kind = Option<(u8, u8, u8)>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = COption<CTuple3<u8, u8, u8>>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(&(u8, u8, u8): Encode, Decode<'static>);
        assert_not_impl_any!(&[(u8, u8, u8)]: Encode, Decode<'static>);
        assert_not_impl_any!(&mut (u8, u8, u8): Encode, Decode<'static>);
        assert_not_impl_any!(&mut [(u8, u8, u8)]: Encode, Decode<'static>);

        assert_not_impl_any!(Box<[(u8, u8, u8)]>: SoftEncode, SoftDecode<'static>);
        assert_not_impl_any!(Box<(u8, u8, u8)>: SoftEncode, SoftDecode<'static>);
        assert_not_impl_any!(Vec<(u8, u8, u8)>: SoftEncode, SoftDecode<'static>);
    }

    #[test]
    fn stored_tuple_3_with_niche() {
        // NOTE: Confirms niche is taken from the first available element
        assert_eq!(<(u8, NonZeroU8, bool)>::NICHE_VALUE, CTuple3(0, 0, 0));

        assert_impl_all!((u8, NonZeroU8, bool):
            ReprFamily<Kind = (u8, NonZeroU8, bool)>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CTuple3<u8, u8, u8>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(&(u8, NonZeroU8, bool):
            ReprFamily<Kind = &'static (u8, NonZeroU8, bool)>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const CTuple3<u8, u8, u8>>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&mut (u8, NonZeroU8, bool):
            ReprFamily<Kind = &'static mut (u8, NonZeroU8, bool)>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut CTuple3<u8, u8, u8>>,
            SoftDecode<'static>,
            SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<(u8, NonZeroU8, bool)>:
            ReprFamily<Kind = Box<(u8, NonZeroU8, bool)>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<CTuple3<u8, u8, u8>>>,
            SoftDecodeOwned<'static>,
            SoftEncodeOwned,
        );
        assert_impl_all!(&[(u8, NonZeroU8, bool)]:
            ReprFamily<Kind = &'static [(u8, NonZeroU8, bool)]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<CTuple3<u8, u8, u8>>>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&mut [(u8, NonZeroU8, bool)]:
            ReprFamily<Kind = &'static mut [(u8, NonZeroU8, bool)]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<CTuple3<u8, u8, u8>>>,
            // FIxme:
            //SoftDecode<'static>,
            //SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[(u8, NonZeroU8, bool)]>:
            ReprFamily<Kind = Box<[(u8, NonZeroU8, bool)]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<CTuple3<u8, u8, u8>>>,
            SoftDecodeOwned<'static>,
            SoftEncodeOwned,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, NonZeroU8, bool)>:
            ReprFamily<Kind = Vec<(u8, NonZeroU8, bool)>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<CTuple3<u8, u8, u8>>>,
            SoftDecodeOwned<'static>,
            SoftEncodeOwned,
        );
        assert_impl_all!([(u8, NonZeroU8, bool); 2]:
            ReprFamily<Kind = [(u8, NonZeroU8, bool); 2]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [CTuple3<u8, u8, u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<(u8, NonZeroU8, bool)>:
            ReprFamily<Kind = Option<(u8, NonZeroU8, bool)>>,
            // TODO: Depends on: https://github.com/mversic/co3/issues/33
            //NicheFamily<Kind = WithCustomNiche>,
            //Niche<CType = CTuple3<u8, u8, u8>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(&(u8, NonZeroU8, bool): Encode, Decode<'static>);
        assert_not_impl_any!(&[(u8, NonZeroU8, bool)]: Encode, Decode<'static>);
        assert_not_impl_any!(&mut (u8, NonZeroU8, bool): Encode, Decode<'static>);
        assert_not_impl_any!(&mut [(u8, NonZeroU8, bool)]: Encode, Decode<'static>);

        assert_not_impl_any!(Box<[(u8, NonZeroU8, u8)]>: SoftEncode, SoftDecode<'static>);
        assert_not_impl_any!(Box<(u8, NonZeroU8, u8)>: SoftEncode, SoftDecode<'static>);
        assert_not_impl_any!(Vec<(u8, NonZeroU8, u8)>: SoftEncode, SoftDecode<'static>);
    }
}
