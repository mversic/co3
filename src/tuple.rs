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
//! use co3::{option::COption, tuple::CTuple3, ExternC, EncodeWithStore};
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
    ExternC, ReprC, Store,
    borrow::{Borrow, ToOwned},
    cloned::DecodeCloned,
    heapify::Heapify,
    niche::{Niche, NicheFamily, WithNiche, WithoutNiche},
    size::SizeFamily,
};

macro_rules! impl_tuple {
    ( ($( $ty:ident ),+) -> $ffi_ty:ident ) => {
        unsafe impl<$($ty: ReprC),+> ReprC for $ffi_ty<$($ty),+> {}

        /// FFI-safe tuple with a stable `repr(C)` memory layout.
        ///
        /// See [the module level documentation](self) for more.
        #[repr(C)]
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $ffi_ty<$($ty),+>($(pub $ty),+);

        crate::reprC! {
            impl($($ty),+) Cloned for ($($ty,)+) {}
        }

        impl_tuple!(@dst ($($ty),+));

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
                let field_out_ptrs: private_out_ptr::OutPtr<$(<$ty as crate::out_ptr::OutPtr>::OutPtr),+> = (&mut field_out_ptrs).into();

                unsafe {
                    $( crate::out_ptr::OutPtrWrite::write_out($ty, field_out_ptrs.$ty.as_mut_ptr()); )+
                    out_ptr.write($ffi_ty($( field_out_ptrs.$ty.assume_init() ),+));
                }
            }
        }

        impl<$($ty: Heapify),+> Heapify for ($($ty,)+) {
            type Kind = ($( <$ty>::Kind, )+);

            #[expect(non_snake_case)]
            fn heapify(self) -> Self::Kind {
                let ($($ty,)+) = self;
                ($( <$ty>::heapify($ty), )+)
            }

            #[expect(non_snake_case)]
            fn unheapify(($($ty,)+): Self::Kind) -> Self {
                ($( <$ty>::unheapify($ty), )+)
            }
        }

        impl<$($ty: Borrow<true>),+, const IN_STRUCT: bool> Borrow<IN_STRUCT> for ($($ty,)+) {
            type Borrowed<'itm> = ($( <$ty as Borrow<true>>::Borrowed<'itm>, )+)
            where
                Self: 'itm;

            type Store = ($( <$ty as Borrow<true>>::Store, )+);

            #[inline(always)]
            #[expect(non_snake_case)]
            fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                impl_tuple! {@decl_priv_store $($ty),+}

                let ($($ty,)+) = self;
                let store: private_store::Store<$(<$ty as Borrow<true>>::Store),+> = store.into();
                ($( <$ty as Borrow<true>>::borrow($ty, store.$ty), )+)
            }
        }

        impl<'r, $($ty: ToOwned<'r, true>),+, const IN_STRUCT: bool> ToOwned<'r, IN_STRUCT> for ($($ty,)+) {
            #[expect(non_snake_case)]
            fn to_owned(($($ty,)+): Self::Borrowed<'r>) -> Self {
                ($( <$ty as ToOwned<'r, true>>::to_owned($ty), )+)
            }
        }

        impl<$($ty: crate::EncodeWithStore),+> crate::EncodeWithStore for ($($ty,)+) {
            type Store = ($( $ty::Store, )+);

            #[expect(non_snake_case)]
            fn encode<'itm>(self, store: &mut Self::Store) -> Self::CType where Self: 'itm {
                impl_tuple! {@decl_priv_store $($ty),+}

                let ($($ty,)+) = self;
                let store: private_store::Store<$(<$ty as crate::EncodeWithStore>::Store),+> = store.into();
                $ffi_ty($( $ty::encode($ty, store.$ty), )+)
            }
        }
        impl<'d, $($ty: crate::DecodeWithStore<'d, false>),+> crate::DecodeWithStore<'d, false> for ($($ty,)+) {
            type Store = ($( $ty::Store, )+);

            #[expect(non_snake_case)]
            unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                impl_tuple! {@decl_priv_store $($ty),+}

                let $ffi_ty($($ty,)+) = source;
                let store: private_store::Store<$(<$ty as crate::DecodeWithStore<'d>>::Store),+> = store.into();
                Some(unsafe {($( $ty::decode($ty, store.$ty)?, )+)})
            }
        }

        impl<'d, $($ty: DecodeCloned<'d, false>),+> DecodeCloned<'d, false> for ($($ty,)+) {
            #[expect(non_snake_case)]
            unsafe fn decode_cloned<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                impl_tuple! {@decl_priv_store $($ty),+}

                let $ffi_ty($($ty,)+) = source;
                let store: private_store::Store<$(<$ty as crate::DecodeWithStore<'d>>::Store),+> = store.into();
                Some(unsafe {($( $ty::decode_cloned($ty, store.$ty)?, )+)})
            }
        }

        unsafe impl<$($ty: crate::out_ptr::Zst),+> crate::out_ptr::Zst for ($($ty,)+) {}

        impl<$($ty),+> From<($( $ty, )+)> for $ffi_ty<$($ty),+> {
            #[expect(non_snake_case)]
            fn from(source: ($( $ty, )+)) -> Self {
                let ($($ty,)+) = source;
                Self($( $ty ),+)
            }
        }
    };

    (@dst_split ($($head:ident,)*) $last:ident) => {
        impl<$($head,)* $last: SizeFamily> SizeFamily for ($($head,)* $last,) {
            type Kind = <$last as SizeFamily>::Kind;
        }
    };

    (@dst_split ($($head:ident,)*) $next:ident, $($tail:ident),+) => {
        impl_tuple!(@dst_split ($($head,)* $next,) $($tail),+);
    };

    (@dst ($($ty:ident),+)) => {
        impl_tuple!(@dst_split () $($ty),+);
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
    {
        const NICHE_VALUE: Self::CType = CTuple2(A::NICHE_VALUE, unsafe { core::mem::zeroed() });
    }

    impl<A: ExternC, B: Niche> Niche for (A, B)
    where
        A: NicheFamily<Kind = WithoutNiche>,
        B: NicheFamily<Kind: WithNiche>,
    {
        const NICHE_VALUE: Self::CType = CTuple2(unsafe { core::mem::zeroed() }, B::NICHE_VALUE);
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

macro_rules! impl_tuple_niche_recursive {
    ($(($($all:ident),+) => ($left:ty, $right:ty) : $ffi_ty:ident($($field:tt),+)),+ $(,)?) => {
        $(
            impl<$($all: ExternC),+> Niche for ($($all,)+)
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
        CSlice, CSliceMut, Decode, DecodeWithStore, Encode, EncodeWithStore,
        ir::ReprFamily,
        niche::{StableNiche, WithCustomNiche, WithStableNiche},
        option::COption,
    };

    #[test]
    fn cloned_tuple_3_without_niche() {
        assert_impl_all!((u8, u8, u8):
            ReprFamily<Kind = (u8, u8, u8)>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = CTuple3<u8, u8, u8>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );

        assert_impl_all!(&(u8, u8, u8):
            ReprFamily<Kind = &'static (u8, u8, u8)>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const CTuple3<u8, u8, u8>>,
        );
        assert_impl_all!(&mut (u8, u8, u8):
            ReprFamily<Kind = &'static mut (u8, u8, u8)>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut CTuple3<u8, u8, u8>>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<(u8, u8, u8)>:
            ReprFamily<Kind = Box<(u8, u8, u8)>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<CTuple3<u8, u8, u8>>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&[(u8, u8, u8)]:
            ReprFamily<Kind = &'static [(u8, u8, u8)]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<CTuple3<u8, u8, u8>>>,
        );
        assert_impl_all!(&mut [(u8, u8, u8)]:
            ReprFamily<Kind = &'static mut [(u8, u8, u8)]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<CTuple3<u8, u8, u8>>>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[(u8, u8, u8)]>:
            ReprFamily<Kind = Box<[(u8, u8, u8)]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<CTuple3<u8, u8, u8>>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, u8, u8)>:
            ReprFamily<Kind = Vec<(u8, u8, u8)>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<CTuple3<u8, u8, u8>>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!([(u8, u8, u8); 2]:
            ReprFamily<Kind = [(u8, u8, u8); 2]>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = [CTuple3<u8, u8, u8>; 2]>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(Option<(u8, u8, u8)>:
            ReprFamily<Kind = Option<WithoutNiche>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = COption<CTuple3<u8, u8, u8>>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );

        assert_impl_all!(&(u8, u8, u8): EncodeWithStore, DecodeWithStore<'static>);
        assert_impl_all!(&[(u8, u8, u8)]: EncodeWithStore, DecodeWithStore<'static>);
        assert_impl_all!(&mut (u8, u8, u8): EncodeWithStore, DecodeWithStore<'static>);
        assert_impl_all!(&mut [(u8, u8, u8)]: EncodeWithStore, DecodeWithStore<'static>);

        assert_not_impl_any!(&(u8, u8, u8): Encode, Decode<'static>);
        assert_not_impl_any!(&[(u8, u8, u8)]: Encode, Decode<'static>);
        assert_not_impl_any!(&mut (u8, u8, u8): Encode, Decode<'static>);
        assert_not_impl_any!(&mut [(u8, u8, u8)]: Encode, Decode<'static>);
    }

    #[test]
    fn cloned_tuple_3_with_niche() {
        // NOTE: Confirms niche is taken from the first available element
        assert_eq!(<(u8, NonZeroU8, bool)>::NICHE_VALUE, CTuple3(0, 0, 0));

        assert_impl_all!((u8, NonZeroU8, bool):
            ReprFamily<Kind = (u8, NonZeroU8, bool)>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CTuple3<u8, u8, u8>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );

        assert_impl_all!(&(u8, NonZeroU8, bool):
            ReprFamily<Kind = &'static (u8, NonZeroU8, bool)>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const CTuple3<u8, u8, u8>>,
        );
        assert_impl_all!(&mut (u8, NonZeroU8, bool):
            ReprFamily<Kind = &'static mut (u8, NonZeroU8, bool)>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut CTuple3<u8, u8, u8>>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<(u8, NonZeroU8, bool)>:
            ReprFamily<Kind = Box<(u8, NonZeroU8, bool)>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<CTuple3<u8, u8, u8>>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&[(u8, NonZeroU8, bool)]:
            ReprFamily<Kind = &'static [(u8, NonZeroU8, bool)]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<CTuple3<u8, u8, u8>>>,
        );
        assert_impl_all!(&mut [(u8, NonZeroU8, bool)]:
            ReprFamily<Kind = &'static mut [(u8, NonZeroU8, bool)]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<CTuple3<u8, u8, u8>>>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[(u8, NonZeroU8, bool)]>:
            ReprFamily<Kind = Box<[(u8, NonZeroU8, bool)]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<CTuple3<u8, u8, u8>>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, NonZeroU8, bool)>:
            ReprFamily<Kind = Vec<(u8, NonZeroU8, bool)>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<CTuple3<u8, u8, u8>>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!([(u8, NonZeroU8, bool); 2]:
            ReprFamily<Kind = [(u8, NonZeroU8, bool); 2]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [CTuple3<u8, u8, u8>; 2]>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(Option<(u8, NonZeroU8, bool)>:
            ReprFamily<Kind = Option<WithCustomNiche>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
            // TODO: Depends on: https://github.com/mversic/co3/issues/33
            //NicheFamily<Kind = WithCustomNiche>,
            //Niche<CType = CTuple3<u8, u8, u8>>,
        );

        assert_impl_all!(&(u8, NonZeroU8, bool): EncodeWithStore, DecodeWithStore<'static>);
        assert_impl_all!(&[(u8, NonZeroU8, bool)]: EncodeWithStore, DecodeWithStore<'static>);
        assert_impl_all!(&mut (u8, NonZeroU8, bool): EncodeWithStore, DecodeWithStore<'static>);
        assert_impl_all!(&mut [(u8, NonZeroU8, bool)]: EncodeWithStore, DecodeWithStore<'static>);

        assert_not_impl_any!(&(u8, NonZeroU8, bool): Encode, Decode<'static>);
        assert_not_impl_any!(&[(u8, NonZeroU8, bool)]: Encode, Decode<'static>);
        assert_not_impl_any!(&mut (u8, NonZeroU8, bool): Encode, Decode<'static>);
        assert_not_impl_any!(&mut [(u8, NonZeroU8, bool)]: Encode, Decode<'static>);
    }
}
