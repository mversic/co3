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
//! use co3::{option::COption, tuple::CTuple3, ExternC, Encode};
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

use crate::{
    ExternC, ReprC, Store,
    cloned::DecodeCloned,
    ir::Cloned,
    niche::{Niche, NicheFamily, WithCustomNiche, WithNiche, WithoutNiche},
};

macro_rules! impl_tuple {
    ( ($( $ty:ident ),+) -> $ffi_ty:ident ) => {
        unsafe impl<$($ty: ReprC),+> ReprC for $ffi_ty<$($ty),+> {}

        /// FFI-safe tuple with a stable `repr(C)` memory layout.
        ///
        /// See [the module level documentation](self) for more.
        #[repr(C)]
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $ffi_ty<$($ty: ReprC),+>($(pub $ty),+);

        impl<$($ty),+> Cloned for ($($ty,)+) {}
        impl<$($ty),+> crate::ir::ReprFamily for ($($ty,)+) {
            type Kind = Self;
        }

        impl<$($ty: ExternC),+> crate::ExternC for ($($ty,)+) {
            type CType = $ffi_ty<$($ty::CType),+>;
        }

        #[expect(non_snake_case)]
        impl<$($ty: Store),+> Store for ($($ty,)+) {
            fn sync(self) -> Option<()> {
                let ($($ty,)+) = self;
                $( $ty.sync()?; )+
                Some(())
            }
        }

        impl<$($ty: crate::out_ptr::OutPtr),+> crate::out_ptr::OutPtr for ($($ty,)+) {
            type OutPtr = $ffi_ty<$($ty::OutPtr),+>;
        }

        impl<$($ty: crate::out_ptr::OutPtrWrite),+> crate::out_ptr::OutPtrWrite for ($($ty,)+) {
            #[expect(non_snake_case)]
            unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
                impl_tuple! {@decl_priv_out_ptr $($ty),+}
                let mut field_out_ptrs = ($(core::mem::MaybeUninit::<$ty::OutPtr>::uninit(),)+);

                let ($($ty,)+) = self;
                let field_out_ptrs: private_out_ptr::OutPtr<$($ty),+> = (&mut field_out_ptrs).into();

                unsafe {
                    $( crate::out_ptr::OutPtrWrite::write_out($ty, field_out_ptrs.$ty.as_mut_ptr()); )+
                    out_ptr.write($ffi_ty($( field_out_ptrs.$ty.assume_init() ),+));
                }
            }
        }
        impl<$($ty: crate::out_ptr::OutPtrRead),+> crate::out_ptr::OutPtrRead for ($($ty,)+) {
            #[expect(non_snake_case)]
            unsafe fn try_read_out(source: Self::OutPtr) -> Option<Self> {
                impl_tuple! {@decl_priv_out_ptr $($ty),+}

                let $ffi_ty($($ty,)+) = source;
                Some(unsafe {($( crate::out_ptr::OutPtrRead::try_read_out($ty)?, )+)})
            }
        }

        impl<$($ty: crate::Encode),+> crate::Encode for ($($ty,)+) {
            type Store = ($( $ty::Store, )+);

            #[expect(non_snake_case)]
            fn encode<'itm>(self, store: &mut Self::Store) -> Self::CType where Self: 'itm {
                impl_tuple! {@decl_priv_store $($ty),+ for crate::Encode : Store}

                let ($($ty,)+) = self;
                let store: private_store::Store<$($ty),+> = store.into();
                $ffi_ty($( $ty::encode($ty, store.$ty), )+)
            }
        }
        impl<'d, $($ty: crate::Decode<'d>),+> crate::Decode<'d> for ($($ty,)+) {
            type Store = ($( $ty::Store, )+);

            #[expect(non_snake_case)]
            unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                impl_tuple! {@decl_priv_store $($ty),+ for crate::Decode<'itm> : Store}

                let $ffi_ty($($ty,)+) = source;
                let store: private_store::Store<$($ty),+> = store.into();
                Some(unsafe {($( $ty::decode($ty, store.$ty)?, )+)})
            }
        }

        impl<'d, $($ty: DecodeCloned<'d>),+> DecodeCloned<'d> for ($($ty,)+) {
            #[expect(non_snake_case)]
            unsafe fn decode_cloned<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                impl_tuple! {@decl_priv_store $($ty),+ for crate::Decode<'itm> : Store}

                let $ffi_ty($($ty,)+) = source;
                let store: private_store::Store<$($ty),+> = store.into();
                Some(unsafe {($( $ty::decode_cloned($ty, store.$ty)?, )+)})
            }
        }

        unsafe impl<$($ty: crate::out_ptr::NonLocal),+> crate::out_ptr::NonLocal for ($($ty,)+) {}
        unsafe impl<$($ty: crate::out_ptr::Zst),+> crate::out_ptr::Zst for ($($ty,)+) {}

        impl<$($ty: crate::ReprC),+> From<($( $ty, )+)> for $ffi_ty<$($ty),+> {
            #[expect(non_snake_case)]
            fn from(source: ($( $ty, )+)) -> Self {
                let ($($ty,)+) = source;
                Self($( $ty ),+)
            }
        }
    };

    // NOTE: This is a trick to index tuples
    ( @decl_priv_store $( $ty:ident ),+ for $trait:path : $store:ident) => {
        mod private_store {
            #[allow(dead_code)]
            pub struct Store<'itm, $($ty: $trait),+> {
                $(pub $ty: &'itm mut $ty::$store),+
            }

            impl<'itm, $($ty: $trait),+> From<&'itm mut ($($ty::$store,)+)> for Store<'itm, $($ty,)+> {
                fn from(($($ty,)+): &'itm mut ($($ty::$store,)+)) -> Self {
                    Self {$($ty,)+}
                }
            }
        }
    };

    // NOTE: This is a trick to index tuples
    ( @decl_priv_out_ptr $( $ty:ident ),+ $(,)? ) => {
        mod private_out_ptr {
            #[allow(dead_code)]
            pub struct OutPtr<'itm, $($ty: crate::out_ptr::OutPtrWrite),+> {
                $(pub $ty: &'itm mut core::mem::MaybeUninit::<$ty::OutPtr>),+
            }

            impl<'itm, $($ty: crate::out_ptr::OutPtrWrite),+> From<&'itm mut ($(core::mem::MaybeUninit::<$ty::OutPtr>,)+)> for OutPtr<'itm, $($ty),+> {
                fn from(($($ty,)+): &'itm mut ($(core::mem::MaybeUninit::<$ty::OutPtr>,)+)) -> Self {
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
    pub trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<A, B> Niche for (A, B)
    where
        A: NicheFamily<Kind: WithNiche> + Niche,
        B: NicheFamily<Kind = WithoutNiche> + ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple2(A::NICHE_VALUE, unsafe { core::mem::zeroed() });
    }

    impl<A, B> Niche for (A, B)
    where
        A: NicheFamily<Kind = WithoutNiche> + ExternC,
        B: NicheFamily<Kind: WithNiche> + Niche,
    {
        const NICHE_VALUE: Self::CType = CTuple2(unsafe { core::mem::zeroed() }, B::NICHE_VALUE);
    }

    impl<A, B> Niche for (A, B)
    where
        A: NicheFamily<Kind: WithNiche> + Niche,
        B: NicheFamily<Kind: WithNiche> + ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple2(A::NICHE_VALUE, unsafe { core::mem::zeroed() });
    }
}

impl<A: ExternC, B: ExternC, C: ExternC> Niche for (A, B, C)
where
    (A, (B, C)): Niche<CType = CTuple2<<A as ExternC>::CType, <(B, C) as ExternC>::CType>>,
{
    const NICHE_VALUE: Self::CType = CTuple3(
        <(A, (B, C))>::NICHE_VALUE.0,
        <(A, (B, C))>::NICHE_VALUE.1.0,
        <(A, (B, C))>::NICHE_VALUE.1.1,
    );
}

impl<A: ExternC, B: ExternC, C: ExternC, D: ExternC> Niche for (A, B, C, D)
where
    ((A, B), (C, D)):
        Niche<CType = CTuple2<<(A, B) as ExternC>::CType, <(C, D) as ExternC>::CType>>,
{
    const NICHE_VALUE: Self::CType = CTuple4(
        <((A, B), (C, D))>::NICHE_VALUE.0.0,
        <((A, B), (C, D))>::NICHE_VALUE.0.1,
        <((A, B), (C, D))>::NICHE_VALUE.1.0,
        <((A, B), (C, D))>::NICHE_VALUE.1.1,
    );
}

impl<A: ExternC, B: ExternC, C: ExternC, D: ExternC, E: ExternC> Niche for (A, B, C, D, E)
where
    ((A, B), (C, D, E)):
        Niche<CType = CTuple2<<(A, B) as ExternC>::CType, <(C, D, E) as ExternC>::CType>>,
{
    const NICHE_VALUE: Self::CType = CTuple5(
        <((A, B), (C, D, E))>::NICHE_VALUE.0.0,
        <((A, B), (C, D, E))>::NICHE_VALUE.0.1,
        <((A, B), (C, D, E))>::NICHE_VALUE.1.0,
        <((A, B), (C, D, E))>::NICHE_VALUE.1.1,
        <((A, B), (C, D, E))>::NICHE_VALUE.1.2,
    );
}

impl<A: ExternC, B: ExternC, C: ExternC, D: ExternC, E: ExternC, F: ExternC> Niche
    for (A, B, C, D, E, F)
where
    ((A, B, C), (D, E, F)):
        Niche<CType = CTuple2<<(A, B, C) as ExternC>::CType, <(D, E, F) as ExternC>::CType>>,
{
    const NICHE_VALUE: Self::CType = CTuple6(
        <((A, B, C), (D, E, F))>::NICHE_VALUE.0.0,
        <((A, B, C), (D, E, F))>::NICHE_VALUE.0.1,
        <((A, B, C), (D, E, F))>::NICHE_VALUE.0.2,
        <((A, B, C), (D, E, F))>::NICHE_VALUE.1.0,
        <((A, B, C), (D, E, F))>::NICHE_VALUE.1.1,
        <((A, B, C), (D, E, F))>::NICHE_VALUE.1.2,
    );
}

impl<A: ExternC, B: ExternC, C: ExternC, D: ExternC, E: ExternC, F: ExternC, G: ExternC> Niche
    for (A, B, C, D, E, F, G)
where
    ((A, B, C), (D, E, F, G)):
        Niche<CType = CTuple2<<(A, B, C) as ExternC>::CType, <(D, E, F, G) as ExternC>::CType>>,
{
    const NICHE_VALUE: Self::CType = CTuple7(
        <((A, B, C), (D, E, F, G))>::NICHE_VALUE.0.0,
        <((A, B, C), (D, E, F, G))>::NICHE_VALUE.0.1,
        <((A, B, C), (D, E, F, G))>::NICHE_VALUE.0.2,
        <((A, B, C), (D, E, F, G))>::NICHE_VALUE.1.0,
        <((A, B, C), (D, E, F, G))>::NICHE_VALUE.1.1,
        <((A, B, C), (D, E, F, G))>::NICHE_VALUE.1.2,
        <((A, B, C), (D, E, F, G))>::NICHE_VALUE.1.3,
    );
}

impl<A: ExternC, B: ExternC, C: ExternC, D: ExternC, E: ExternC, F: ExternC, G: ExternC, H: ExternC>
    Niche for (A, B, C, D, E, F, G, H)
where
    ((A, B, C, D), (E, F, G, H)):
        Niche<CType = CTuple2<<(A, B, C, D) as ExternC>::CType, <(E, F, G, H) as ExternC>::CType>>,
{
    const NICHE_VALUE: Self::CType = CTuple8(
        <((A, B, C, D), (E, F, G, H))>::NICHE_VALUE.0.0,
        <((A, B, C, D), (E, F, G, H))>::NICHE_VALUE.0.1,
        <((A, B, C, D), (E, F, G, H))>::NICHE_VALUE.0.2,
        <((A, B, C, D), (E, F, G, H))>::NICHE_VALUE.0.3,
        <((A, B, C, D), (E, F, G, H))>::NICHE_VALUE.1.0,
        <((A, B, C, D), (E, F, G, H))>::NICHE_VALUE.1.1,
        <((A, B, C, D), (E, F, G, H))>::NICHE_VALUE.1.2,
        <((A, B, C, D), (E, F, G, H))>::NICHE_VALUE.1.3,
    );
}

impl<
    A: ExternC,
    B: ExternC,
    C: ExternC,
    D: ExternC,
    E: ExternC,
    F: ExternC,
    G: ExternC,
    H: ExternC,
    I: ExternC,
> Niche for (A, B, C, D, E, F, G, H, I)
where
    ((A, B, C, D), (E, F, G, H, I)): Niche<
        CType = CTuple2<<(A, B, C, D) as ExternC>::CType, <(E, F, G, H, I) as ExternC>::CType>,
    >,
{
    const NICHE_VALUE: Self::CType = CTuple9(
        <((A, B, C, D), (E, F, G, H, I))>::NICHE_VALUE.0.0,
        <((A, B, C, D), (E, F, G, H, I))>::NICHE_VALUE.0.1,
        <((A, B, C, D), (E, F, G, H, I))>::NICHE_VALUE.0.2,
        <((A, B, C, D), (E, F, G, H, I))>::NICHE_VALUE.0.3,
        <((A, B, C, D), (E, F, G, H, I))>::NICHE_VALUE.1.0,
        <((A, B, C, D), (E, F, G, H, I))>::NICHE_VALUE.1.1,
        <((A, B, C, D), (E, F, G, H, I))>::NICHE_VALUE.1.2,
        <((A, B, C, D), (E, F, G, H, I))>::NICHE_VALUE.1.3,
        <((A, B, C, D), (E, F, G, H, I))>::NICHE_VALUE.1.4,
    );
}

impl<
    A: ExternC,
    B: ExternC,
    C: ExternC,
    D: ExternC,
    E: ExternC,
    F: ExternC,
    G: ExternC,
    H: ExternC,
    I: ExternC,
    J: ExternC,
> Niche for (A, B, C, D, E, F, G, H, I, J)
where
    ((A, B, C, D, E), (F, G, H, I, J)): Niche<
        CType = CTuple2<<(A, B, C, D, E) as ExternC>::CType, <(F, G, H, I, J) as ExternC>::CType>,
    >,
{
    const NICHE_VALUE: Self::CType = CTuple10(
        <((A, B, C, D, E), (F, G, H, I, J))>::NICHE_VALUE.0.0,
        <((A, B, C, D, E), (F, G, H, I, J))>::NICHE_VALUE.0.1,
        <((A, B, C, D, E), (F, G, H, I, J))>::NICHE_VALUE.0.2,
        <((A, B, C, D, E), (F, G, H, I, J))>::NICHE_VALUE.0.3,
        <((A, B, C, D, E), (F, G, H, I, J))>::NICHE_VALUE.0.4,
        <((A, B, C, D, E), (F, G, H, I, J))>::NICHE_VALUE.1.0,
        <((A, B, C, D, E), (F, G, H, I, J))>::NICHE_VALUE.1.1,
        <((A, B, C, D, E), (F, G, H, I, J))>::NICHE_VALUE.1.2,
        <((A, B, C, D, E), (F, G, H, I, J))>::NICHE_VALUE.1.3,
        <((A, B, C, D, E), (F, G, H, I, J))>::NICHE_VALUE.1.4,
    );
}

impl<
    A: ExternC,
    B: ExternC,
    C: ExternC,
    D: ExternC,
    E: ExternC,
    F: ExternC,
    G: ExternC,
    H: ExternC,
    I: ExternC,
    J: ExternC,
    K: ExternC,
> Niche for (A, B, C, D, E, F, G, H, I, J, K)
where
    ((A, B, C, D, E), (F, G, H, I, J, K)): Niche<
        CType = CTuple2<
            <(A, B, C, D, E) as ExternC>::CType,
            <(F, G, H, I, J, K) as ExternC>::CType,
        >,
    >,
{
    const NICHE_VALUE: Self::CType = CTuple11(
        <((A, B, C, D, E), (F, G, H, I, J, K))>::NICHE_VALUE.0.0,
        <((A, B, C, D, E), (F, G, H, I, J, K))>::NICHE_VALUE.0.1,
        <((A, B, C, D, E), (F, G, H, I, J, K))>::NICHE_VALUE.0.2,
        <((A, B, C, D, E), (F, G, H, I, J, K))>::NICHE_VALUE.0.3,
        <((A, B, C, D, E), (F, G, H, I, J, K))>::NICHE_VALUE.0.4,
        <((A, B, C, D, E), (F, G, H, I, J, K))>::NICHE_VALUE.1.0,
        <((A, B, C, D, E), (F, G, H, I, J, K))>::NICHE_VALUE.1.1,
        <((A, B, C, D, E), (F, G, H, I, J, K))>::NICHE_VALUE.1.2,
        <((A, B, C, D, E), (F, G, H, I, J, K))>::NICHE_VALUE.1.3,
        <((A, B, C, D, E), (F, G, H, I, J, K))>::NICHE_VALUE.1.4,
        <((A, B, C, D, E), (F, G, H, I, J, K))>::NICHE_VALUE.1.5,
    );
}

impl<
    A: ExternC,
    B: ExternC,
    C: ExternC,
    D: ExternC,
    E: ExternC,
    F: ExternC,
    G: ExternC,
    H: ExternC,
    I: ExternC,
    J: ExternC,
    K: ExternC,
    L: ExternC,
> Niche for (A, B, C, D, E, F, G, H, I, J, K, L)
where
    ((A, B, C, D, E, F), (G, H, I, J, K, L)): Niche<
        CType = CTuple2<
            <(A, B, C, D, E, F) as ExternC>::CType,
            <(G, H, I, J, K, L) as ExternC>::CType,
        >,
    >,
{
    const NICHE_VALUE: Self::CType = CTuple12(
        <((A, B, C, D, E, F), (G, H, I, J, K, L))>::NICHE_VALUE.0.0,
        <((A, B, C, D, E, F), (G, H, I, J, K, L))>::NICHE_VALUE.0.1,
        <((A, B, C, D, E, F), (G, H, I, J, K, L))>::NICHE_VALUE.0.2,
        <((A, B, C, D, E, F), (G, H, I, J, K, L))>::NICHE_VALUE.0.3,
        <((A, B, C, D, E, F), (G, H, I, J, K, L))>::NICHE_VALUE.0.4,
        <((A, B, C, D, E, F), (G, H, I, J, K, L))>::NICHE_VALUE.0.5,
        <((A, B, C, D, E, F), (G, H, I, J, K, L))>::NICHE_VALUE.1.0,
        <((A, B, C, D, E, F), (G, H, I, J, K, L))>::NICHE_VALUE.1.1,
        <((A, B, C, D, E, F), (G, H, I, J, K, L))>::NICHE_VALUE.1.2,
        <((A, B, C, D, E, F), (G, H, I, J, K, L))>::NICHE_VALUE.1.3,
        <((A, B, C, D, E, F), (G, H, I, J, K, L))>::NICHE_VALUE.1.4,
        <((A, B, C, D, E, F), (G, H, I, J, K, L))>::NICHE_VALUE.1.5,
    );
}

impl<A: NicheFamily> NicheFamily for (A,) {
    type Kind = A::Kind;
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    pub trait NicheFamily {
        type Kind;
    }

    impl<A, B> NicheFamily for (A, B)
    where
        A: NicheFamily<Kind = WithoutNiche>,
        B: NicheFamily<Kind = WithoutNiche>,
    {
        type Kind = WithoutNiche;
    }
    impl<A, B> NicheFamily for (A, B)
    where
        A: NicheFamily<Kind: WithNiche>,
        B: NicheFamily<Kind = WithoutNiche>,
    {
        type Kind = WithCustomNiche;
    }
    impl<A, B> NicheFamily for (A, B)
    where
        A: NicheFamily<Kind = WithoutNiche>,
        B: NicheFamily<Kind: WithNiche>,
    {
        type Kind = WithCustomNiche;
    }
    impl<A, B> NicheFamily for (A, B)
    where
        A: NicheFamily<Kind: WithNiche>,
        B: NicheFamily<Kind: WithNiche>,
    {
        type Kind = WithCustomNiche;
    }
}

impl<A, B, C> NicheFamily for (A, B, C)
where
    (A, (B, C)): NicheFamily,
{
    type Kind = <(A, (B, C)) as NicheFamily>::Kind;
}

impl<A, B, C, D> NicheFamily for (A, B, C, D)
where
    ((A, B), (C, D)): NicheFamily,
{
    type Kind = <((A, B), (C, D)) as NicheFamily>::Kind;
}

impl<A, B, C, D, E> NicheFamily for (A, B, C, D, E)
where
    ((A, B), (C, D, E)): NicheFamily,
{
    type Kind = <((A, B), (C, D, E)) as NicheFamily>::Kind;
}

impl<A, B, C, D, E, F> NicheFamily for (A, B, C, D, E, F)
where
    ((A, B, C), (D, E, F)): NicheFamily,
{
    type Kind = <((A, B, C), (D, E, F)) as NicheFamily>::Kind;
}

impl<A, B, C, D, E, F, G> NicheFamily for (A, B, C, D, E, F, G)
where
    ((A, B, C), (D, E, F, G)): NicheFamily,
{
    type Kind = <((A, B, C), (D, E, F, G)) as NicheFamily>::Kind;
}

impl<A, B, C, D, E, F, G, H> NicheFamily for (A, B, C, D, E, F, G, H)
where
    ((A, B, C, D), (E, F, G, H)): NicheFamily,
{
    type Kind = <((A, B, C, D), (E, F, G, H)) as NicheFamily>::Kind;
}

impl<A, B, C, D, E, F, G, H, I> NicheFamily for (A, B, C, D, E, F, G, H, I)
where
    ((A, B, C, D), (E, F, G, H, I)): NicheFamily,
{
    type Kind = <((A, B, C, D), (E, F, G, H, I)) as NicheFamily>::Kind;
}

impl<A, B, C, D, E, F, G, H, I, J> NicheFamily for (A, B, C, D, E, F, G, H, I, J)
where
    ((A, B, C, D, E), (F, G, H, I, J)): NicheFamily,
{
    type Kind = <((A, B, C, D, E), (F, G, H, I, J)) as NicheFamily>::Kind;
}

impl<A, B, C, D, E, F, G, H, I, J, K> NicheFamily for (A, B, C, D, E, F, G, H, I, J, K)
where
    ((A, B, C, D, E), (F, G, H, I, J, K)): NicheFamily,
{
    type Kind = <((A, B, C, D, E), (F, G, H, I, J, K)) as NicheFamily>::Kind;
}

impl<A, B, C, D, E, F, G, H, I, J, K, L> NicheFamily for (A, B, C, D, E, F, G, H, I, J, K, L)
where
    ((A, B, C, D, E, F), (G, H, I, J, K, L)): NicheFamily,
{
    type Kind = <((A, B, C, D, E, F), (G, H, I, J, K, L)) as NicheFamily>::Kind;
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "unstable-refs")]
    use crate::slice::CSlice;
    use crate::{
        ir::{ReprFamily, Robust},
        niche::StableNiche,
        option::COption,
        transmute::{CheckedTransmute, FlatTransmute},
    };

    use super::*;

    use alloc::boxed::Box;
    use static_assertions::{assert_impl_all, assert_not_impl_any};

    #[test]
    fn cloned_tuple_3_without_niche() {
        assert_impl_all!((u8, u8, u8): ExternC<CType = CTuple3<u8, u8, u8>>);
        #[cfg(feature = "unstable-refs")]
        assert_impl_all!(&(u8, u8, u8): StableNiche<CType = *const CTuple3<u8, u8, u8>>);
        assert_impl_all!(Box<(u8, u8, u8)>: StableNiche<CType = *mut CTuple3<u8, u8, u8>>);
        #[cfg(feature = "unstable-refs")]
        assert_impl_all!(&[(u8, u8, u8)]: Niche<CType = CSlice<CTuple3<u8, u8, u8>>>);
        assert_impl_all!([(u8, u8, u8); 2]: ExternC<CType = [CTuple3<u8, u8, u8>; 2]>);
        assert_impl_all!(Option<(u8, u8, u8)>: Niche<CType = COption<CTuple3<u8, u8, u8>>>);

        assert_not_impl_any!((u8, u8, u8): ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>, Niche);
        assert_not_impl_any!(&(u8, u8, u8): ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>);
        assert_not_impl_any!(&mut (u8, u8, u8): ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>);
        assert_not_impl_any!(Box<(u8, u8, u8)>: ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>);
        assert_not_impl_any!(&[(u8, u8, u8)]: ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>, StableNiche);
        assert_not_impl_any!(&mut [(u8, u8, u8)]: ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>, StableNiche);
        assert_not_impl_any!([(u8, u8, u8); 2]: ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>, Niche);
        assert_not_impl_any!(Option<(u8, u8, u8)>: ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>, StableNiche);

        #[cfg(not(feature = "unstable-refs"))]
        assert_not_impl_any!(&(u8, u8, u8): ExternC);
        #[cfg(not(feature = "unstable-refs"))]
        assert_not_impl_any!(&[(u8, u8, u8)]: ExternC);
        #[cfg(not(feature = "unstable-refs"))]
        assert_not_impl_any!(&mut (u8, u8, u8): ExternC);
        #[cfg(not(feature = "unstable-refs"))]
        assert_not_impl_any!(&mut [(u8, u8, u8)]: ExternC);

        #[cfg(not(feature = "unstable-refs"))]
        assert_not_impl_any!(&mut (u8, u8, u8): ReprFamily);
        //#[cfg(not(feature = "unstable-refs"))]
        //assert_not_impl_any!(&mut [(u8, u8, u8)]: ReprFamily);
    }

    #[test]
    fn cloned_tuple_3_with_niche() {
        assert_impl_all!((u8, bool, u8): Niche<CType = CTuple3<u8, u8, u8>>);
        #[cfg(feature = "unstable-refs")]
        assert_impl_all!(&(u8, bool, u8): StableNiche<CType = *const CTuple3<u8, u8, u8>>);
        assert_impl_all!(Box<(u8, bool, u8)>: StableNiche<CType = *mut CTuple3<u8, u8, u8>>);
        #[cfg(feature = "unstable-refs")]
        assert_impl_all!(&[(u8, bool, u8)]: Niche<CType = CSlice<CTuple3<u8, u8, u8>>>);
        assert_impl_all!([(u8, bool, u8); 2]: Niche<CType = [CTuple3<u8, u8, u8>; 2]>);
        // TODO: Depends on: https://github.com/mversic/co3/issues/33
        //assert_impl_all!(Option<(u8, bool, u8)>: Niche<CType = CTuple3<u8, u8, u8>>);

        assert_not_impl_any!((u8, bool, u8): ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>, StableNiche);
        assert_not_impl_any!(&(u8, bool, u8): ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>);
        assert_not_impl_any!(&mut (u8, bool, u8): ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>);
        assert_not_impl_any!(Box<(u8, bool, u8)>: ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>);
        assert_not_impl_any!(&[(u8, bool, u8)]: ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>, StableNiche);
        assert_not_impl_any!(&mut [(u8, bool, u8)]: ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>, StableNiche);
        assert_not_impl_any!([(u8, bool, u8); 2]: ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>, StableNiche);
        assert_not_impl_any!(Option<(u8, bool, u8)>: ReprC, CheckedTransmute, FlatTransmute<Target: ReprFamily<Kind = Robust>>, StableNiche);

        #[cfg(not(feature = "unstable-refs"))]
        assert_not_impl_any!(&(u8, bool, u8): ExternC);
        #[cfg(not(feature = "unstable-refs"))]
        assert_not_impl_any!(&[(u8, bool, u8)]: ExternC);

        #[cfg(not(feature = "unstable-refs"))]
        assert_not_impl_any!(&mut (u8, bool, u8): ReprFamily, ExternC);
        #[cfg(not(feature = "unstable-refs"))]
        assert_not_impl_any!(&mut [(u8, bool, u8)]: ExternC);
    }
}
