//! This module provides `repr(C)` tuple types that can be safely passed across FFI boundaries.
//!
//! # Memory Layout
//!
//! Unlike Rust's native tuples, these have a guaranteed C-compatible
//! memory layout with fields in the order of declaration.
//!
//! # Niche Optimization
//!
//! When one of the tuple elements has a niche value (trap representations), `Option<(A, B, ...)>`
//! is optimized to use niche value of the **first element with a niche**. Values of all the other
//! elements in the niche representation are zeroed, though this is not not a stable guarantee
//! at the moment.
//!
//! When none of the tuple elements have a niche value [`Option<(A, B, ...)>`] is represented as a
//! [`CTuple2<u8, CTupleN<A, B, ...>>`] where first element corresponds to the discriminant
//!
//! # Example
//!
//! ```rust
//! use core::mem::size_of;
//!
//! use co3::{ExternC, Encode, CTuple3};
//!
//! type TupleWithNiche1 = (u8, bool, &bool);
//! type TupleWithNiche2 = (u8, &bool, bool);
//! type TupleWithoutNiche = (u64, u32, u8);
//!
//! assert_eq!(size_of::<TupleWithNiche1::CType>(), size_of::<Option::<TupleWithNiche1>::CType>());
//! assert_eq!(size_of::<TupleWithNiche2::CType>(), size_of::<Option::<TupleWithNiche2>::CType>());
//!
//! assert_eq!(
//!     1 + size_of::<TupleWithoutNiche::CType>(),
//!     size_of::<Option::<TupleWithoutNiche>::CType>()
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
//! assert_eq!(none_value_1.encode(&mut store1), TupleWithNiche1(0, 2, core::ptr::null()));
//! assert_eq!(none_value_2.encode(&mut store2), TupleWithNiche2(0, core::ptr::null(), 0));
//!
//! assert_eq!(
//!     none_value_3.encode(&mut store3),
//!     CTuple2(0, TupleWithoutNiche(0, core::ptr::null(), 0))
//! );
//! ```

use crate::{
    ExternC, ReprC, Result,
    ir::Cloned,
    niche::{Ir, Niche},
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
        impl<$($ty),+> crate::ir::Ir for ($($ty,)+) {
            type Type = Self;
        }

        impl<$($ty: ExternC),+> crate::ExternC for ($($ty,)+) {
            type CType = $ffi_ty<$($ty::CType),+>;
        }

        impl<$($ty: crate::out_ptr::OutPtr),+> crate::out_ptr::OutPtr for ($($ty,)+) {
            type OutPtr = $ffi_ty<$($ty::OutPtr),+>;
        }

        #[expect(non_snake_case)]
        impl<$($ty: crate::out_ptr::OutPtrWrite),+> crate::out_ptr::OutPtrWrite for ($($ty,)+) {
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
        #[expect(non_snake_case)]
        impl<$($ty: crate::out_ptr::OutPtrRead),+> crate::out_ptr::OutPtrRead for ($($ty,)+) {
            unsafe fn try_read_out(source: Self::OutPtr) -> Result<Self> {
                impl_tuple! {@decl_priv_out_ptr $($ty),+}

                let $ffi_ty($($ty,)+) = source;
                Ok(unsafe {($( crate::out_ptr::OutPtrRead::try_read_out($ty)?, )+)})
            }
        }

        impl<$($ty: crate::Encode),+> crate::Encode for ($($ty,)+) {
            type Store = ($( $ty::Store, )+);

            #[expect(non_snake_case)]
            fn encode<'itm>(self, store: &mut Self::Store) -> Self::CType where Self: 'itm {
                impl_tuple! {@decl_priv_store $($ty),+ for crate::Encode : Store}

                let ($($ty,)+) = self;
                let store: private_store::Store<$($ty),+> = store.into();
                $ffi_ty($( <$ty as crate::Encode>::encode($ty, store.$ty),)+)
            }
        }
        impl<'d, $($ty: crate::Decode<'d>),+> crate::Decode<'d> for ($($ty,)+) {
            type Store = ($( $ty::Store, )+);

            #[expect(non_snake_case)]
            unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Result<Self> {
                impl_tuple! {@decl_priv_store $($ty),+ for crate::Decode<'itm> : Store}

                let $ffi_ty($($ty,)+) = source;
                let store: private_store::Store<$($ty),+> = store.into();
                Ok(unsafe {($( <$ty as crate::Decode<'d>>::decode($ty, store.$ty)?, )+)})
            }
        }

        unsafe impl<$($ty: crate::out_ptr::NonLocal),+> crate::out_ptr::NonLocal for ($($ty,)+) {}
        unsafe impl<$($ty: crate::out_ptr::Zst),+> crate::out_ptr::Zst for ($($ty,)+) {}

        #[expect(non_snake_case)]
        impl<$($ty: crate::ReprC),+> From<($( $ty, )+)> for $ffi_ty<$($ty),+> {
            fn from(source: ($( $ty, )+)) -> Self {
                let ($($ty,)+) = source;
                Self($( $ty ),+)
            }
        }
    };

    // NOTE: This is a trick to index tuples
    ( @decl_priv_store $( $ty:ident ),+ for $trait:path : $store:ident) => {
        mod private_store {
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
    trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<A, B> Niche for (A, B)
    where
        A: Ir<Type: crate::niche::WithNiche> + Niche,
        B: Ir<Type = crate::niche::WithoutNiche> + ExternC,
    {
        // TODO: Instead of using `core::mem::zeroed`, memory can be left uninitialized. Use MaybeUninit?
        const NICHE_VALUE: Self::CType = CTuple2(A::NICHE_VALUE, unsafe { core::mem::zeroed() });
    }

    impl<A, B> Niche for (A, B)
    where
        A: Ir<Type = crate::niche::WithoutNiche> + ExternC,
        B: Ir<Type: crate::niche::WithNiche> + Niche,
    {
        const NICHE_VALUE: Self::CType = CTuple2(unsafe { core::mem::zeroed() }, B::NICHE_VALUE);
    }

    impl<A, B> Niche for (A, B)
    where
        A: Ir<Type: crate::niche::WithNiche> + Niche,
        B: Ir<Type: crate::niche::WithNiche> + ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple2(A::NICHE_VALUE, unsafe { core::mem::zeroed() });
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<A, B, C> Niche for (A, B, C)
    where
        A: Ir<Type: crate::niche::WithNiche> + Niche,
        (B, C): Ir<Type = crate::niche::WithoutNiche>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple3(
            <A as Niche>::NICHE_VALUE,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }

    impl<A, B, C> Niche for (A, B, C)
    where
        A: Ir<Type = crate::niche::WithoutNiche>,
        (B, C): Ir<Type: crate::niche::WithNiche>
            + Niche<CType = CTuple2<<B as ExternC>::CType, <C as ExternC>::CType>>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple3(
            unsafe { core::mem::zeroed() },
            <(B, C) as Niche>::NICHE_VALUE.0,
            <(B, C) as Niche>::NICHE_VALUE.1,
        );
    }

    impl<A, B, C> Niche for (A, B, C)
    where
        A: Ir<Type: crate::niche::WithNiche> + Niche,
        (B, C): Ir<Type: crate::niche::WithNiche>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple3(
            <A as Niche>::NICHE_VALUE,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<A, B, C, D> Niche for (A, B, C, D)
    where
        (A, B): Ir<Type: crate::niche::WithNiche>
            + Niche<CType = CTuple2<<A as ExternC>::CType, <B as ExternC>::CType>>,
        (C, D): Ir<Type = crate::niche::WithoutNiche>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple4(
            <(A, B) as Niche>::NICHE_VALUE.0,
            <(A, B) as Niche>::NICHE_VALUE.1,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }

    impl<A, B, C, D> Niche for (A, B, C, D)
    where
        (A, B): Ir<Type = crate::niche::WithoutNiche>,
        (C, D): Ir<Type: crate::niche::WithNiche>
            + Niche<CType = CTuple2<<C as ExternC>::CType, <D as ExternC>::CType>>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple4(
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            <(C, D) as Niche>::NICHE_VALUE.0,
            <(C, D) as Niche>::NICHE_VALUE.1,
        );
    }

    impl<A, B, C, D> Niche for (A, B, C, D)
    where
        (A, B): Ir<Type: crate::niche::WithNiche>
            + Niche<CType = CTuple2<<A as ExternC>::CType, <B as ExternC>::CType>>,
        (C, D): Ir<Type: crate::niche::WithNiche>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple4(
            <(A, B) as Niche>::NICHE_VALUE.0,
            <(A, B) as Niche>::NICHE_VALUE.1,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<A, B, C, D, E> Niche for (A, B, C, D, E)
    where
        (A, B): Ir<Type: crate::niche::WithNiche>
            + Niche<CType = CTuple2<<A as ExternC>::CType, <B as ExternC>::CType>>,
        (C, D, E): Ir<Type = crate::niche::WithoutNiche>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple5(
            <(A, B) as Niche>::NICHE_VALUE.0,
            <(A, B) as Niche>::NICHE_VALUE.1,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }

    impl<A, B, C, D, E> Niche for (A, B, C, D, E)
    where
        (A, B): Ir<Type = crate::niche::WithoutNiche>,
        (C, D, E): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple3<
                    <C as ExternC>::CType,
                    <D as ExternC>::CType,
                    <E as ExternC>::CType,
                >,
            >,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple5(
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            <(C, D, E) as Niche>::NICHE_VALUE.0,
            <(C, D, E) as Niche>::NICHE_VALUE.1,
            <(C, D, E) as Niche>::NICHE_VALUE.2,
        );
    }

    impl<A, B, C, D, E> Niche for (A, B, C, D, E)
    where
        (A, B): Ir<Type: crate::niche::WithNiche>
            + Niche<CType = CTuple2<<A as ExternC>::CType, <B as ExternC>::CType>>,
        (C, D, E): Ir<Type: crate::niche::WithNiche>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple5(
            <(A, B) as Niche>::NICHE_VALUE.0,
            <(A, B) as Niche>::NICHE_VALUE.1,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<A, B, C, D, E, F> Niche for (A, B, C, D, E, F)
    where
        (A, B, C): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple3<
                    <A as ExternC>::CType,
                    <B as ExternC>::CType,
                    <C as ExternC>::CType,
                >,
            >,
        (D, E, F): Ir<Type = crate::niche::WithoutNiche>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
        F: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple6(
            <(A, B, C) as Niche>::NICHE_VALUE.0,
            <(A, B, C) as Niche>::NICHE_VALUE.1,
            <(A, B, C) as Niche>::NICHE_VALUE.2,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }

    impl<A, B, C, D, E, F> Niche for (A, B, C, D, E, F)
    where
        (A, B, C): Ir<Type = crate::niche::WithoutNiche>,
        (D, E, F): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple3<
                    <D as ExternC>::CType,
                    <E as ExternC>::CType,
                    <F as ExternC>::CType,
                >,
            >,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
        F: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple6(
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            <(D, E, F) as Niche>::NICHE_VALUE.0,
            <(D, E, F) as Niche>::NICHE_VALUE.1,
            <(D, E, F) as Niche>::NICHE_VALUE.2,
        );
    }

    impl<A, B, C, D, E, F> Niche for (A, B, C, D, E, F)
    where
        (A, B, C): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple3<
                    <A as ExternC>::CType,
                    <B as ExternC>::CType,
                    <C as ExternC>::CType,
                >,
            >,
        (D, E, F): Ir<Type: crate::niche::WithNiche>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
        F: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple6(
            <(A, B, C) as Niche>::NICHE_VALUE.0,
            <(A, B, C) as Niche>::NICHE_VALUE.1,
            <(A, B, C) as Niche>::NICHE_VALUE.2,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<A, B, C, D, E, F, G> Niche for (A, B, C, D, E, F, G)
    where
        (A, B, C): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple3<
                    <A as ExternC>::CType,
                    <B as ExternC>::CType,
                    <C as ExternC>::CType,
                >,
            >,
        (D, E, F, G): Ir<Type = crate::niche::WithoutNiche>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
        F: ExternC,
        G: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple7(
            <(A, B, C) as Niche>::NICHE_VALUE.0,
            <(A, B, C) as Niche>::NICHE_VALUE.1,
            <(A, B, C) as Niche>::NICHE_VALUE.2,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }

    impl<A, B, C, D, E, F, G> Niche for (A, B, C, D, E, F, G)
    where
        (A, B, C): Ir<Type = crate::niche::WithoutNiche>,
        (D, E, F, G): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple4<
                    <D as ExternC>::CType,
                    <E as ExternC>::CType,
                    <F as ExternC>::CType,
                    <G as ExternC>::CType,
                >,
            >,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
        F: ExternC,
        G: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple7(
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            <(D, E, F, G) as Niche>::NICHE_VALUE.0,
            <(D, E, F, G) as Niche>::NICHE_VALUE.1,
            <(D, E, F, G) as Niche>::NICHE_VALUE.2,
            <(D, E, F, G) as Niche>::NICHE_VALUE.3,
        );
    }

    impl<A, B, C, D, E, F, G> Niche for (A, B, C, D, E, F, G)
    where
        (A, B, C): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple3<
                    <A as ExternC>::CType,
                    <B as ExternC>::CType,
                    <C as ExternC>::CType,
                >,
            >,
        (D, E, F, G): Ir<Type: crate::niche::WithNiche>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
        F: ExternC,
        G: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple7(
            <(A, B, C) as Niche>::NICHE_VALUE.0,
            <(A, B, C) as Niche>::NICHE_VALUE.1,
            <(A, B, C) as Niche>::NICHE_VALUE.2,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<A, B, C, D, E, F, G, H> Niche for (A, B, C, D, E, F, G, H)
    where
        (A, B, C, D): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple4<
                    <A as ExternC>::CType,
                    <B as ExternC>::CType,
                    <C as ExternC>::CType,
                    <D as ExternC>::CType,
                >,
            >,
        (E, F, G, H): Ir<Type = crate::niche::WithoutNiche>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
        F: ExternC,
        G: ExternC,
        H: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple8(
            <(A, B, C, D) as Niche>::NICHE_VALUE.0,
            <(A, B, C, D) as Niche>::NICHE_VALUE.1,
            <(A, B, C, D) as Niche>::NICHE_VALUE.2,
            <(A, B, C, D) as Niche>::NICHE_VALUE.3,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }

    impl<A, B, C, D, E, F, G, H> Niche for (A, B, C, D, E, F, G, H)
    where
        (A, B, C, D): Ir<Type = crate::niche::WithoutNiche>,
        (E, F, G, H): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple4<
                    <E as ExternC>::CType,
                    <F as ExternC>::CType,
                    <G as ExternC>::CType,
                    <H as ExternC>::CType,
                >,
            >,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
        F: ExternC,
        G: ExternC,
        H: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple8(
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            <(E, F, G, H) as Niche>::NICHE_VALUE.0,
            <(E, F, G, H) as Niche>::NICHE_VALUE.1,
            <(E, F, G, H) as Niche>::NICHE_VALUE.2,
            <(E, F, G, H) as Niche>::NICHE_VALUE.3,
        );
    }

    impl<A, B, C, D, E, F, G, H> Niche for (A, B, C, D, E, F, G, H)
    where
        (A, B, C, D): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple4<
                    <A as ExternC>::CType,
                    <B as ExternC>::CType,
                    <C as ExternC>::CType,
                    <D as ExternC>::CType,
                >,
            >,
        (E, F, G, H): Ir<Type: crate::niche::WithNiche>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
        F: ExternC,
        G: ExternC,
        H: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple8(
            <(A, B, C, D) as Niche>::NICHE_VALUE.0,
            <(A, B, C, D) as Niche>::NICHE_VALUE.1,
            <(A, B, C, D) as Niche>::NICHE_VALUE.2,
            <(A, B, C, D) as Niche>::NICHE_VALUE.3,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<A, B, C, D, E, F, G, H, I> Niche for (A, B, C, D, E, F, G, H, I)
    where
        (A, B, C, D): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple4<
                    <A as ExternC>::CType,
                    <B as ExternC>::CType,
                    <C as ExternC>::CType,
                    <D as ExternC>::CType,
                >,
            >,
        (E, F, G, H, I): Ir<Type = crate::niche::WithoutNiche>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
        F: ExternC,
        G: ExternC,
        H: ExternC,
        I: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple9(
            <(A, B, C, D) as Niche>::NICHE_VALUE.0,
            <(A, B, C, D) as Niche>::NICHE_VALUE.1,
            <(A, B, C, D) as Niche>::NICHE_VALUE.2,
            <(A, B, C, D) as Niche>::NICHE_VALUE.3,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }

    impl<A, B, C, D, E, F, G, H, I> Niche for (A, B, C, D, E, F, G, H, I)
    where
        (A, B, C, D): Ir<Type = crate::niche::WithoutNiche>,
        (E, F, G, H, I): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple5<
                    <E as ExternC>::CType,
                    <F as ExternC>::CType,
                    <G as ExternC>::CType,
                    <H as ExternC>::CType,
                    <I as ExternC>::CType,
                >,
            >,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
        F: ExternC,
        G: ExternC,
        H: ExternC,
        I: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple9(
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            <(E, F, G, H, I) as Niche>::NICHE_VALUE.0,
            <(E, F, G, H, I) as Niche>::NICHE_VALUE.1,
            <(E, F, G, H, I) as Niche>::NICHE_VALUE.2,
            <(E, F, G, H, I) as Niche>::NICHE_VALUE.3,
            <(E, F, G, H, I) as Niche>::NICHE_VALUE.4,
        );
    }

    impl<A, B, C, D, E, F, G, H, I> Niche for (A, B, C, D, E, F, G, H, I)
    where
        (A, B, C, D): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple4<
                    <A as ExternC>::CType,
                    <B as ExternC>::CType,
                    <C as ExternC>::CType,
                    <D as ExternC>::CType,
                >,
            >,
        (E, F, G, H, I): Ir<Type: crate::niche::WithNiche>,
        A: ExternC,
        B: ExternC,
        C: ExternC,
        D: ExternC,
        E: ExternC,
        F: ExternC,
        G: ExternC,
        H: ExternC,
        I: ExternC,
    {
        const NICHE_VALUE: Self::CType = CTuple9(
            <(A, B, C, D) as Niche>::NICHE_VALUE.0,
            <(A, B, C, D) as Niche>::NICHE_VALUE.1,
            <(A, B, C, D) as Niche>::NICHE_VALUE.2,
            <(A, B, C, D) as Niche>::NICHE_VALUE.3,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<A, B, C, D, E, F, G, H, I, J> Niche for (A, B, C, D, E, F, G, H, I, J)
    where
        (A, B, C, D, E): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple5<
                    <A as ExternC>::CType,
                    <B as ExternC>::CType,
                    <C as ExternC>::CType,
                    <D as ExternC>::CType,
                    <E as ExternC>::CType,
                >,
            >,
        (F, G, H, I, J): Ir<Type = crate::niche::WithoutNiche>,
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
    {
        const NICHE_VALUE: Self::CType = CTuple10(
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.0,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.1,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.2,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.3,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.4,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }

    impl<A, B, C, D, E, F, G, H, I, J> Niche for (A, B, C, D, E, F, G, H, I, J)
    where
        (A, B, C, D, E): Ir<Type = crate::niche::WithoutNiche>,
        (F, G, H, I, J): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple5<
                    <F as ExternC>::CType,
                    <G as ExternC>::CType,
                    <H as ExternC>::CType,
                    <I as ExternC>::CType,
                    <J as ExternC>::CType,
                >,
            >,
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
    {
        const NICHE_VALUE: Self::CType = CTuple10(
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            <(F, G, H, I, J) as Niche>::NICHE_VALUE.0,
            <(F, G, H, I, J) as Niche>::NICHE_VALUE.1,
            <(F, G, H, I, J) as Niche>::NICHE_VALUE.2,
            <(F, G, H, I, J) as Niche>::NICHE_VALUE.3,
            <(F, G, H, I, J) as Niche>::NICHE_VALUE.4,
        );
    }

    impl<A, B, C, D, E, F, G, H, I, J> Niche for (A, B, C, D, E, F, G, H, I, J)
    where
        (A, B, C, D, E): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple5<
                    <A as ExternC>::CType,
                    <B as ExternC>::CType,
                    <C as ExternC>::CType,
                    <D as ExternC>::CType,
                    <E as ExternC>::CType,
                >,
            >,
        (F, G, H, I, J): Ir<Type: crate::niche::WithNiche>,
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
    {
        const NICHE_VALUE: Self::CType = CTuple10(
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.0,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.1,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.2,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.3,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.4,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<A, B, C, D, E, F, G, H, I, J, K> Niche for (A, B, C, D, E, F, G, H, I, J, K)
    where
        (A, B, C, D, E): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple5<
                    <A as ExternC>::CType,
                    <B as ExternC>::CType,
                    <C as ExternC>::CType,
                    <D as ExternC>::CType,
                    <E as ExternC>::CType,
                >,
            >,
        (F, G, H, I, J, K): Ir<Type = crate::niche::WithoutNiche>,
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
    {
        const NICHE_VALUE: Self::CType = CTuple11(
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.0,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.1,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.2,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.3,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.4,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }

    impl<A, B, C, D, E, F, G, H, I, J, K> Niche for (A, B, C, D, E, F, G, H, I, J, K)
    where
        (A, B, C, D, E): Ir<Type = crate::niche::WithoutNiche>,
        (F, G, H, I, J, K): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple6<
                    <F as ExternC>::CType,
                    <G as ExternC>::CType,
                    <H as ExternC>::CType,
                    <I as ExternC>::CType,
                    <J as ExternC>::CType,
                    <K as ExternC>::CType,
                >,
            >,
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
    {
        const NICHE_VALUE: Self::CType = CTuple11(
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            <(F, G, H, I, J, K) as Niche>::NICHE_VALUE.0,
            <(F, G, H, I, J, K) as Niche>::NICHE_VALUE.1,
            <(F, G, H, I, J, K) as Niche>::NICHE_VALUE.2,
            <(F, G, H, I, J, K) as Niche>::NICHE_VALUE.3,
            <(F, G, H, I, J, K) as Niche>::NICHE_VALUE.4,
            <(F, G, H, I, J, K) as Niche>::NICHE_VALUE.5,
        );
    }

    impl<A, B, C, D, E, F, G, H, I, J, K> Niche for (A, B, C, D, E, F, G, H, I, J, K)
    where
        (A, B, C, D, E): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple5<
                    <A as ExternC>::CType,
                    <B as ExternC>::CType,
                    <C as ExternC>::CType,
                    <D as ExternC>::CType,
                    <E as ExternC>::CType,
                >,
            >,
        (F, G, H, I, J, K): Ir<Type: crate::niche::WithNiche>,
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
    {
        const NICHE_VALUE: Self::CType = CTuple11(
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.0,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.1,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.2,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.3,
            <(A, B, C, D, E) as Niche>::NICHE_VALUE.4,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<A, B, C, D, E, F, G, H, I, J, K, L> Niche for (A, B, C, D, E, F, G, H, I, J, K, L)
    where
        (A, B, C, D, E, F): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple6<
                    <A as ExternC>::CType,
                    <B as ExternC>::CType,
                    <C as ExternC>::CType,
                    <D as ExternC>::CType,
                    <E as ExternC>::CType,
                    <F as ExternC>::CType,
                >,
            >,
        (G, H, I, J, K, L): Ir<Type = crate::niche::WithoutNiche>,
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
    {
        const NICHE_VALUE: Self::CType = CTuple12(
            <(A, B, C, D, E, F) as Niche>::NICHE_VALUE.0,
            <(A, B, C, D, E, F) as Niche>::NICHE_VALUE.1,
            <(A, B, C, D, E, F) as Niche>::NICHE_VALUE.2,
            <(A, B, C, D, E, F) as Niche>::NICHE_VALUE.3,
            <(A, B, C, D, E, F) as Niche>::NICHE_VALUE.4,
            <(A, B, C, D, E, F) as Niche>::NICHE_VALUE.5,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }

    impl<A, B, C, D, E, F, G, H, I, J, K, L> Niche for (A, B, C, D, E, F, G, H, I, J, K, L)
    where
        (A, B, C, D, E, F): Ir<Type = crate::niche::WithoutNiche>,
        (G, H, I, J, K, L): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple6<
                    <G as ExternC>::CType,
                    <H as ExternC>::CType,
                    <I as ExternC>::CType,
                    <J as ExternC>::CType,
                    <K as ExternC>::CType,
                    <L as ExternC>::CType,
                >,
            >,
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
    {
        const NICHE_VALUE: Self::CType = CTuple12(
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            <(G, H, I, J, K, L) as Niche>::NICHE_VALUE.0,
            <(G, H, I, J, K, L) as Niche>::NICHE_VALUE.1,
            <(G, H, I, J, K, L) as Niche>::NICHE_VALUE.2,
            <(G, H, I, J, K, L) as Niche>::NICHE_VALUE.3,
            <(G, H, I, J, K, L) as Niche>::NICHE_VALUE.4,
            <(G, H, I, J, K, L) as Niche>::NICHE_VALUE.5,
        );
    }

    impl<A, B, C, D, E, F, G, H, I, J, K, L> Niche for (A, B, C, D, E, F, G, H, I, J, K, L)
    where
        (A, B, C, D, E, F): Ir<Type: crate::niche::WithNiche>
            + Niche<
                CType = CTuple6<
                    <A as ExternC>::CType,
                    <B as ExternC>::CType,
                    <C as ExternC>::CType,
                    <D as ExternC>::CType,
                    <E as ExternC>::CType,
                    <F as ExternC>::CType,
                >,
            >,
        (G, H, I, J, K, L): Ir<Type: crate::niche::WithNiche>,
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
    {
        const NICHE_VALUE: Self::CType = CTuple12(
            <(A, B, C, D, E, F) as Niche>::NICHE_VALUE.0,
            <(A, B, C, D, E, F) as Niche>::NICHE_VALUE.1,
            <(A, B, C, D, E, F) as Niche>::NICHE_VALUE.2,
            <(A, B, C, D, E, F) as Niche>::NICHE_VALUE.3,
            <(A, B, C, D, E, F) as Niche>::NICHE_VALUE.4,
            <(A, B, C, D, E, F) as Niche>::NICHE_VALUE.5,
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
            unsafe { core::mem::zeroed() },
        );
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Ir {
        type Type;
    }

    impl<A> Ir for (A,)
    where
        A: Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithoutNiche;
    }
    impl<A> Ir for (A,)
    where
        A: Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Ir {
        type Type;
    }

    impl<A, B> Ir for (A, B)
    where
        A: Ir<Type = crate::niche::WithoutNiche>,
        B: Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithoutNiche;
    }
    impl<A, B> Ir for (A, B)
    where
        A: Ir<Type: crate::niche::WithNiche>,
        B: Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B> Ir for (A, B)
    where
        A: Ir<Type = crate::niche::WithoutNiche>,
        B: Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B> Ir for (A, B)
    where
        A: Ir<Type: crate::niche::WithNiche>,
        B: Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Ir {
        type Type;
    }

    impl<A, B, C> Ir for (A, B, C)
    where
        A: Ir<Type = crate::niche::WithoutNiche>,
        (B, C): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithoutNiche;
    }
    impl<A, B, C> Ir for (A, B, C)
    where
        A: Ir<Type: crate::niche::WithNiche>,
        (B, C): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C> Ir for (A, B, C)
    where
        A: Ir<Type = crate::niche::WithoutNiche>,
        (B, C): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C> Ir for (A, B, C)
    where
        A: Ir<Type: crate::niche::WithNiche>,
        (B, C): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Ir {
        type Type;
    }

    impl<A, B, C, D> Ir for (A, B, C, D)
    where
        (A, B): Ir<Type = crate::niche::WithoutNiche>,
        (C, D): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithoutNiche;
    }
    impl<A, B, C, D> Ir for (A, B, C, D)
    where
        (A, B): Ir<Type: crate::niche::WithNiche>,
        (C, D): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D> Ir for (A, B, C, D)
    where
        (A, B): Ir<Type = crate::niche::WithoutNiche>,
        (C, D): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D> Ir for (A, B, C, D)
    where
        (A, B): Ir<Type: crate::niche::WithNiche>,
        (C, D): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Ir {
        type Type;
    }

    impl<A, B, C, D, E> Ir for (A, B, C, D, E)
    where
        (A, B): Ir<Type = crate::niche::WithoutNiche>,
        (C, D, E): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithoutNiche;
    }
    impl<A, B, C, D, E> Ir for (A, B, C, D, E)
    where
        (A, B): Ir<Type: crate::niche::WithNiche>,
        (C, D, E): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E> Ir for (A, B, C, D, E)
    where
        (A, B): Ir<Type = crate::niche::WithoutNiche>,
        (C, D, E): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E> Ir for (A, B, C, D, E)
    where
        (A, B): Ir<Type: crate::niche::WithNiche>,
        (C, D, E): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Ir {
        type Type;
    }

    impl<A, B, C, D, E, F> Ir for (A, B, C, D, E, F)
    where
        (A, B, C): Ir<Type = crate::niche::WithoutNiche>,
        (D, E, F): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithoutNiche;
    }
    impl<A, B, C, D, E, F> Ir for (A, B, C, D, E, F)
    where
        (A, B, C): Ir<Type: crate::niche::WithNiche>,
        (D, E, F): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E, F> Ir for (A, B, C, D, E, F)
    where
        (A, B, C): Ir<Type = crate::niche::WithoutNiche>,
        (D, E, F): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E, F> Ir for (A, B, C, D, E, F)
    where
        (A, B, C): Ir<Type: crate::niche::WithNiche>,
        (D, E, F): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Ir {
        type Type;
    }

    impl<A, B, C, D, E, F, G> Ir for (A, B, C, D, E, F, G)
    where
        (A, B, C): Ir<Type = crate::niche::WithoutNiche>,
        (D, E, F, G): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithoutNiche;
    }
    impl<A, B, C, D, E, F, G> Ir for (A, B, C, D, E, F, G)
    where
        (A, B, C): Ir<Type: crate::niche::WithNiche>,
        (D, E, F, G): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E, F, G> Ir for (A, B, C, D, E, F, G)
    where
        (A, B, C): Ir<Type = crate::niche::WithoutNiche>,
        (D, E, F, G): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E, F, G> Ir for (A, B, C, D, E, F, G)
    where
        (A, B, C): Ir<Type: crate::niche::WithNiche>,
        (D, E, F, G): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Ir {
        type Type;
    }

    impl<A, B, C, D, E, F, G, H> Ir for (A, B, C, D, E, F, G, H)
    where
        (A, B, C, D): Ir<Type = crate::niche::WithoutNiche>,
        (E, F, G, H): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithoutNiche;
    }
    impl<A, B, C, D, E, F, G, H> Ir for (A, B, C, D, E, F, G, H)
    where
        (A, B, C, D): Ir<Type: crate::niche::WithNiche>,
        (E, F, G, H): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E, F, G, H> Ir for (A, B, C, D, E, F, G, H)
    where
        (A, B, C, D): Ir<Type = crate::niche::WithoutNiche>,
        (E, F, G, H): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E, F, G, H> Ir for (A, B, C, D, E, F, G, H)
    where
        (A, B, C, D): Ir<Type: crate::niche::WithNiche>,
        (E, F, G, H): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Ir {
        type Type;
    }

    impl<A, B, C, D, E, F, G, H, I> Ir for (A, B, C, D, E, F, G, H, I)
    where
        (A, B, C, D): Ir<Type = crate::niche::WithoutNiche>,
        (E, F, G, H, I): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithoutNiche;
    }
    impl<A, B, C, D, E, F, G, H, I> Ir for (A, B, C, D, E, F, G, H, I)
    where
        (A, B, C, D): Ir<Type: crate::niche::WithNiche>,
        (E, F, G, H, I): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E, F, G, H, I> Ir for (A, B, C, D, E, F, G, H, I)
    where
        (A, B, C, D): Ir<Type = crate::niche::WithoutNiche>,
        (E, F, G, H, I): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E, F, G, H, I> Ir for (A, B, C, D, E, F, G, H, I)
    where
        (A, B, C, D): Ir<Type: crate::niche::WithNiche>,
        (E, F, G, H, I): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Ir {
        type Type;
    }

    impl<A, B, C, D, E, F, G, H, I, J> Ir for (A, B, C, D, E, F, G, H, I, J)
    where
        (A, B, C, D, E): Ir<Type = crate::niche::WithoutNiche>,
        (F, G, H, I, J): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithoutNiche;
    }
    impl<A, B, C, D, E, F, G, H, I, J> Ir for (A, B, C, D, E, F, G, H, I, J)
    where
        (A, B, C, D, E): Ir<Type: crate::niche::WithNiche>,
        (F, G, H, I, J): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E, F, G, H, I, J> Ir for (A, B, C, D, E, F, G, H, I, J)
    where
        (A, B, C, D, E): Ir<Type = crate::niche::WithoutNiche>,
        (F, G, H, I, J): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E, F, G, H, I, J> Ir for (A, B, C, D, E, F, G, H, I, J)
    where
        (A, B, C, D, E): Ir<Type: crate::niche::WithNiche>,
        (F, G, H, I, J): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Ir {
        type Type;
    }

    impl<A, B, C, D, E, F, G, H, I, J, K> Ir for (A, B, C, D, E, F, G, H, I, J, K)
    where
        (A, B, C, D, E): Ir<Type = crate::niche::WithoutNiche>,
        (F, G, H, I, J, K): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithoutNiche;
    }
    impl<A, B, C, D, E, F, G, H, I, J, K> Ir for (A, B, C, D, E, F, G, H, I, J, K)
    where
        (A, B, C, D, E): Ir<Type: crate::niche::WithNiche>,
        (F, G, H, I, J, K): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E, F, G, H, I, J, K> Ir for (A, B, C, D, E, F, G, H, I, J, K)
    where
        (A, B, C, D, E): Ir<Type = crate::niche::WithoutNiche>,
        (F, G, H, I, J, K): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E, F, G, H, I, J, K> Ir for (A, B, C, D, E, F, G, H, I, J, K)
    where
        (A, B, C, D, E): Ir<Type: crate::niche::WithNiche>,
        (F, G, H, I, J, K): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
}

disjoint_impls::disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Ir {
        type Type;
    }

    impl<A, B, C, D, E, F, G, H, I, J, K, L> Ir for (A, B, C, D, E, F, G, H, I, J, K, L)
    where
        (A, B, C, D, E, F): Ir<Type = crate::niche::WithoutNiche>,
        (G, H, I, J, K, L): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithoutNiche;
    }
    impl<A, B, C, D, E, F, G, H, I, J, K, L> Ir for (A, B, C, D, E, F, G, H, I, J, K, L)
    where
        (A, B, C, D, E, F): Ir<Type: crate::niche::WithNiche>,
        (G, H, I, J, K, L): Ir<Type = crate::niche::WithoutNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E, F, G, H, I, J, K, L> Ir for (A, B, C, D, E, F, G, H, I, J, K, L)
    where
        (A, B, C, D, E, F): Ir<Type = crate::niche::WithoutNiche>,
        (G, H, I, J, K, L): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
    impl<A, B, C, D, E, F, G, H, I, J, K, L> Ir for (A, B, C, D, E, F, G, H, I, J, K, L)
    where
        (A, B, C, D, E, F): Ir<Type: crate::niche::WithNiche>,
        (G, H, I, J, K, L): Ir<Type: crate::niche::WithNiche>,
    {
        type Type = crate::niche::WithCustomNiche;
    }
}

#[cfg(test)]
mod tests {}
