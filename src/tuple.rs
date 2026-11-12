//! Provides `repr(C)` tuple types that can be safely passed across FFI boundaries.
//!
//! # Memory Layout
//!
//! Unlike Rust's native tuples, these have a guaranteed C-compatible
//! memory layout with fields in the order of declaration.
//!
//! The `STABLE` parameter records the source family without changing the layout:
//! native Rust tuples lower to `TupleN<..., false>`, while co3 tuples default to
//! `true`. Lowering a co3 tuple preserves its marker and lowers each field.
//!
//! A constructor without surrounding type information needs an annotation:
//!
//! ```
//! use co3::tuple::Tuple2;
//! let pair: Tuple2<_, _> = Tuple2(23, 23);
//! assert_eq!(pair.into_tuple(), (23, 23));
//! ```
//!
//! # Niche Optimization
//!
//! When one of the tuple elements has a niche value (trap representations), `Option<(A, B, ...)>` is
//! optimized to use niche value of the **first element with a niche** to represent [`None`] value.
//! Values of all the other tuple elements of the niche are zeroed (NOT a stable guarantee yet)
//!
//! When none of the tuple elements have a niche value, `Option<(A, B, ...)>` is represented as a
//! `COption<TupleN<A, B, ...>>`.
//!
//! # Example
//!
//! ```rust
//! use core::mem::size_of;
//!
//! use co3::{
//!     encode, niche::Niche, option::{COption, COptionSome}, primitives::CBool, ReprC,
//! };
//!
//! type TupleWithNiche1<'a> = (u8, bool, &'a bool);
//! type TupleWithNiche2<'a> = (u8, &'a bool, bool);
//! type TupleWithoutNiche = (u64, u32, u8);
//!
//! assert_eq!(
//!     size_of::<<TupleWithNiche1 as ReprC>::CType>(),
//!     size_of::<<Option::<TupleWithNiche1> as ReprC>::CType>()
//! );
//! assert_eq!(
//!     size_of::<<TupleWithNiche2 as ReprC>::CType>(),
//!     size_of::<<Option::<TupleWithNiche2> as ReprC>::CType>());
//!
//! assert_eq!(
//!     8 + size_of::<<TupleWithoutNiche as ReprC>::CType>(),
//!     size_of::<<Option::<TupleWithoutNiche> as ReprC>::CType>()
//! );
//!
//! let none_value_1: Option<TupleWithNiche1> = None;
//! let encoded_none_value_1: COptionSome<TupleWithNiche1> = encode(none_value_1);
//! let payload_1 = &encoded_none_value_1.0;
//! assert_eq!(payload_1.0, 0);
//! assert!(bool::is_niche(&payload_1.1));
//! assert!(payload_1.2.as_ptr().is_null());
//!
//! let none_value_2: Option<TupleWithNiche2> = None;
//! let encoded_none_value_2: COptionSome<TupleWithNiche2> = encode(none_value_2);
//! let payload_2 = &encoded_none_value_2.0;
//! assert_eq!(payload_2.0, 0);
//! assert!(payload_2.1.as_ptr().is_null());
//! assert_eq!(payload_2.2, CBool::FALSE);
//!
//! let none_value_3: Option<TupleWithoutNiche> = None;
//! assert_eq!(encode(none_value_3), COption::None());
//! ```

use rust_spec::{
    RustSpec,
    mutability::{Exclusive, ExclusiveKind},
};

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC, Store,
    niche::Niche,
    stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked, SoftEncodeRef, SoftEncodeRefUnchecked},
    transmute::{AbiCompatible, AbiCompatibleValidate},
};

macro_rules! impl_tuple {
    (@codecs [$($ty:ident:$idx:tt),+] -> $ffi_ty:ident, $encode:ident, $unchecked:ident, $decode:ident) => {
        unsafe impl<$($ty: $encode),*> $encode for ($($ty,)*) {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
            where
                Self: 'itm
            {
                let ($($ty,)*) = self;
                $ffi_ty::<$($ty),*, true>($($ty),*).soft_encode(store).with_source()
            }

        }
        unsafe impl<$($ty: $encode),*, const STABLE: bool> $encode for $ffi_ty<$($ty),*, STABLE> {
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
                $ffi_ty($( $encode::soft_encode($ty, store.$ty) ),*)
            }

        }
        unsafe impl<$($ty: $unchecked),*> $unchecked for ($($ty,)*) {
            type SoftStore = ($($ty::SoftStore,)*);

            #[expect(non_snake_case)]
            unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
                let ($($ty,)*) = self;
                unsafe { $ffi_ty::<$($ty),*, true>($($ty),*).soft_encode_unchecked(store).with_source() }
            }
        }
        unsafe impl<$($ty: $unchecked),*, const STABLE: bool> $unchecked for $ffi_ty<$($ty),*, STABLE> {
            type SoftStore = ($($ty::SoftStore,)*);

            #[expect(non_snake_case)]
            unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
                impl_tuple! {@decl_priv_store $($ty),*}

                let $ffi_ty($($ty),*) = self;
                let store: private_store::Store<$($ty::SoftStore,)*> = store.into();
                $ffi_ty($(unsafe { $ty.soft_encode_unchecked(store.$ty) }),*)
            }
        }
        unsafe impl<'d, $($ty: $decode<'d>),*> $decode<'d> for ($($ty,)*) {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                let $ffi_ty($($ty),*) = unsafe {
                    <$ffi_ty<$($ty),*> as $decode<'d>>::soft_decode(source.with_source(), store)
                }?;
                Some(($($ty,)*))
            }

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
                let $ffi_ty($($ty),*) = unsafe {
                    <$ffi_ty<$($ty),*> as $decode<'d>>::soft_decode_unchecked(source.with_source(), store)
                };
                ($($ty,)*)
            }
        }
        unsafe impl<'d, $($ty: $decode<'d>),*, const STABLE: bool> $decode<'d> for $ffi_ty<$($ty),*, STABLE> {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                impl_tuple! {@decl_priv_store $($ty),*}

                let $ffi_ty($($ty),*) = source;
                let store: private_store::Store<$($ty::Store,)*> = store.into();
                Some(unsafe { $ffi_ty($( $ty::soft_decode($ty, store.$ty)? ),*) })
            }

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
                impl_tuple! {@decl_priv_store $($ty),*}

                let $ffi_ty($($ty),*) = source;
                let store: private_store::Store<$($ty::Store),*> = store.into();
                unsafe { $ffi_ty($( $ty::soft_decode_unchecked($ty, store.$ty) ),*) }
            }
        }
    };

    ( ($( $ty:ident : $idx:tt ),+) -> $ffi_ty:ident ) => {
        impl_tuple!(@main [$($ty:$idx),+] [] $($ty:$idx),+ -> $ffi_ty);
    };

    (@main [$($ty:ident:$idx:tt),+] [$($head:ident:$head_idx:tt,)*] $last:ident:$last_idx:tt -> $ffi_ty:ident) => {
        impl_tuple!(@codecs [$($ty:$idx),+] -> $ffi_ty, EncodeMove, EncodeMoveUnchecked, DecodeMove);
        const _: () = {
            use crate::borrow::{BorrowedEncode, BorrowedEncodeUnchecked, BorrowedDecode};
            impl_tuple!(@codecs [$($ty:$idx),+] -> $ffi_ty, BorrowedEncode, BorrowedEncodeUnchecked, BorrowedDecode);
        };
        unsafe impl<'d, $($ty: ReprC<CType: crate::CType + Sized> + Clone + 'd),+> crate::stored::SoftDecodeRef<'d> for ($($ty,)*)
        where
            Self: RustSpec<Layout = rust_spec::Unstable> + ReprC<CType = $ffi_ty<$(<$ty as ReprC>::CType),*, false>>,
            &'d Self: ReprC<CType = crate::reference::CSoftRef<Self>>,
            $(&'d $ty: DecodeMove<'d> + ReprC<CType: crate::reference::SharedRefCarrier<$ty>>,)+
        {
            type Store = crate::sync::RefSizedDecodeStore<Self, ($(<&'d $ty as DecodeMove<'d>>::Store,)*)>;
            unsafe fn soft_decode_ref<'itm: 'd>(source: <&'d Self as ReprC>::CType, store: &'itm mut Self::Store) -> Option<&'d Self> {
                let source = unsafe { source.into_ref()? };
                store.decode(|fields| Some(($(
                    $ty::clone(unsafe { <&'d $ty>::soft_decode(crate::reference::SharedRefCarrier::from_shared_ptr(&source.$idx), &mut fields.$idx)? }),
                )*)))
            }
            unsafe fn soft_decode_ref_unchecked<'itm: 'd>(source: <&'d Self as ReprC>::CType, store: &'itm mut Self::Store) -> &'d Self {
                let source = unsafe { source.into_ref().unwrap_unchecked() };
                store.decode(|fields| Some(($(
                    $ty::clone(unsafe { <&'d $ty>::soft_decode_unchecked(crate::reference::SharedRefCarrier::from_shared_ptr(&source.$idx), &mut fields.$idx) }),
                )*))).expect("unchecked decoding produces a value")
            }
        }

        unsafe impl<'d, $($ty: ReprC<CType: crate::CType + Sized> + Clone + 'd),+, const STABLE: bool> crate::stored::SoftDecodeRef<'d> for $ffi_ty<$($ty),*, STABLE>
        where
            Self: RustSpec<Layout = rust_spec::Unstable> + ReprC<CType = $ffi_ty<$(<$ty as ReprC>::CType),*, STABLE>>,
            &'d Self: ReprC<CType = crate::reference::CSoftRef<Self>>,
            $(&'d $ty: DecodeMove<'d> + ReprC<CType: crate::reference::SharedRefCarrier<$ty>>,)+
        {
            type Store = crate::sync::RefSizedDecodeStore<Self, ($(<&'d $ty as DecodeMove<'d>>::Store,)*)>;
            unsafe fn soft_decode_ref<'itm: 'd>(source: <&'d Self as ReprC>::CType, store: &'itm mut Self::Store) -> Option<&'d Self> {
                let source = unsafe { source.into_ref()? };
                store.decode(|fields| Some($ffi_ty($(
                    $ty::clone(unsafe { <&'d $ty>::soft_decode(crate::reference::SharedRefCarrier::from_shared_ptr(&source.$idx), &mut fields.$idx)? }),
                )*)))
            }
            unsafe fn soft_decode_ref_unchecked<'itm: 'd>(source: <&'d Self as ReprC>::CType, store: &'itm mut Self::Store) -> &'d Self {
                let source = unsafe { source.into_ref().unwrap_unchecked() };
                store.decode(|fields| Some($ffi_ty($(
                    $ty::clone(unsafe { <&'d $ty>::soft_decode_unchecked(crate::reference::SharedRefCarrier::from_shared_ptr(&source.$idx), &mut fields.$idx) }),
                )*))).expect("unchecked decoding produces a value")
            }
        }
        unsafe impl<'d, $($ty: ReprC<CType: crate::CType + Copy> + 'd),+> crate::stored::SoftDecodeMut<'d> for ($($ty,)*)
        where
            $($ty: crate::borrow::BorrowedDecode<'d>,)*
            Self: EncodeMove<Store: crate::stored::EmptyStore> + RustSpec<Layout = rust_spec::Unstable> + ReprC<CType = $ffi_ty<$(<$ty as ReprC>::CType),*, false>>,
            &'d mut Self: ReprC<CType = crate::reference::CSoftRefMut<Self, true>>,
        {
            type Store = crate::sync::RefMutSizedDecodeStore<Self, ($(<$ty as crate::borrow::BorrowedDecode<'d>>::Store,)*)>;
            unsafe fn soft_decode_mut<'itm: 'd>(source: <&'d mut Self as ReprC>::CType, store: &'itm mut Self::Store) -> Option<&'d mut Self> {
                let pointer = source.as_ptr();
                let source = unsafe { pointer.as_ref()? };
                unsafe { store.decode(pointer, |fields| Some(($(
                    crate::decoded::decode_mut_field::<$ty>(source.$idx, &mut fields.$idx)?,
                )*))) }
            }
            unsafe fn soft_decode_mut_unchecked<'itm: 'd>(source: <&'d mut Self as ReprC>::CType, store: &'itm mut Self::Store) -> &'d mut Self {
                let pointer = source.as_ptr();
                let source = unsafe { &*pointer };
                unsafe { store.decode(pointer, |fields| Some(($(
                    crate::decoded::decode_mut_field_unchecked::<$ty>(source.$idx, &mut fields.$idx),
                )*))) }.expect("unchecked decoding produces a value")
            }
        }

        unsafe impl<'d, $($ty: ReprC<CType: crate::CType + Copy> + 'd),+, const STABLE: bool> crate::stored::SoftDecodeMut<'d> for $ffi_ty<$($ty),*, STABLE>
        where
            $($ty: crate::borrow::BorrowedDecode<'d>,)*
            Self: EncodeMove<Store: crate::stored::EmptyStore> + RustSpec<Layout = rust_spec::Unstable> + ReprC<CType = $ffi_ty<$(<$ty as ReprC>::CType),*, STABLE>>,
            &'d mut Self: ReprC<CType = crate::reference::CSoftRefMut<Self, true>>,
        {
            type Store = crate::sync::RefMutSizedDecodeStore<Self, ($(<$ty as crate::borrow::BorrowedDecode<'d>>::Store,)*)>;
            unsafe fn soft_decode_mut<'itm: 'd>(source: <&'d mut Self as ReprC>::CType, store: &'itm mut Self::Store) -> Option<&'d mut Self> {
                let pointer = source.as_ptr();
                let source = unsafe { pointer.as_ref()? };
                unsafe { store.decode(pointer, |fields| Some($ffi_ty($(
                    crate::decoded::decode_mut_field::<$ty>(source.$idx, &mut fields.$idx)?,
                )*))) }
            }
            unsafe fn soft_decode_mut_unchecked<'itm: 'd>(source: <&'d mut Self as ReprC>::CType, store: &'itm mut Self::Store) -> &'d mut Self {
                let pointer = source.as_ptr();
                let source = unsafe { &*pointer };
                unsafe { store.decode(pointer, |fields| Some($ffi_ty($(
                    crate::decoded::decode_mut_field_unchecked::<$ty>(source.$idx, &mut fields.$idx),
                )*))) }.expect("unchecked decoding produces a value")
            }
        }
        impl<$($ty: Store),+> Store for ($($ty,)+) {
            #[expect(non_snake_case)]
            fn sync(self) -> Option<()> {
                let ($($ty,)+) = self;
                crate::sync::sync_all([$($ty.sync(),)+])
            }
        }

        unsafe impl<'a, $($ty: ReprC<CType: Copy> + 'a),*, __Interior: ExclusiveKind> SoftEncodeRef<'a> for ($($ty,)*)
        where
        &'a Self: crate::ReprC<CType = crate::reference::CSoftRef<Self>>,
            Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
            $(&'a $ty: EncodeMove + ReprC<CType: crate::reference::SharedRefCarrier<$ty>>),*
        {
            type Store = crate::sync::RefSizedEncodeStore<Self, ($(<&'a $ty as EncodeMove>::Store,)*)>;
            #[expect(non_snake_case)]
            fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType  where 'a: 'itm {
                store.encode(|fields| {
                    impl_tuple! {@decl_priv_store $($ty),*}
                    let ($($ty,)*) = self;
                    let fields: private_store::Store<$(<&'a $ty as EncodeMove>::Store,)*> = fields.into();
                    $ffi_ty($($crate::stored::encode_ref_field($ty, fields.$ty)),*)
                })
            }
        }

        unsafe impl<'a, $($ty: ReprC<CType: Copy> + 'a),*, __Interior: ExclusiveKind, const STABLE: bool> SoftEncodeRef<'a> for $ffi_ty<$($ty),*, STABLE>
        where
        &'a Self: crate::ReprC<CType = crate::reference::CSoftRef<Self>>,
            Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
            $(&'a $ty: EncodeMove + ReprC<CType: crate::reference::SharedRefCarrier<$ty>>),*
        {
            type Store = crate::sync::RefSizedEncodeStore<Self, ($(<&'a $ty as EncodeMove>::Store,)*)>;
            #[expect(non_snake_case)]
            fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType  where 'a: 'itm {
                store.encode(|fields| {
                    impl_tuple! {@decl_priv_store $($ty),*}
                    let $ffi_ty($($ty),*) = self;
                    let fields: private_store::Store<$(<&'a $ty as EncodeMove>::Store,)*> = fields.into();
                    $ffi_ty($($crate::stored::encode_ref_field($ty, fields.$ty)),*)
                })
            }
        }

        unsafe impl<'a, $($ty: ReprC<CType: Copy> + 'a),*, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a> for ($($ty,)*)
        where
        &'a Self: crate::ReprC<CType = crate::reference::CSoftRef<Self>>,
            Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
            $(&'a $ty: EncodeMoveUnchecked + ReprC<CType: crate::reference::SharedRefCarrier<$ty>>),*
        {
            type SoftStore = crate::sync::RefSizedEncodeStore<Self, ($(<&'a $ty as EncodeMoveUnchecked>::SoftStore,)*)>;
            #[expect(non_snake_case)]
            unsafe fn soft_encode_ref_unchecked<'itm>(&'a self, store: &'itm mut Self::SoftStore) -> <&'a Self as ReprC>::CType  where 'a: 'itm {
                store.encode(|fields| {
                    impl_tuple! {@decl_priv_store $($ty),*}
                    let ($($ty,)*) = self;
                    let fields: private_store::Store<$(<&'a $ty as EncodeMoveUnchecked>::SoftStore,)*> = fields.into();
                    $ffi_ty($(unsafe { $crate::stored::encode_ref_field_unchecked($ty, fields.$ty) }),*)
                })
            }
        }

        unsafe impl<'a, $($ty: ReprC<CType: Copy> + 'a),*, __Interior: ExclusiveKind, const STABLE: bool> SoftEncodeRefUnchecked<'a> for $ffi_ty<$($ty),*, STABLE>
        where
        &'a Self: crate::ReprC<CType = crate::reference::CSoftRef<Self>>,
            Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
            $(&'a $ty: EncodeMoveUnchecked + ReprC<CType: crate::reference::SharedRefCarrier<$ty>>),*
        {
            type SoftStore = crate::sync::RefSizedEncodeStore<Self, ($(<&'a $ty as EncodeMoveUnchecked>::SoftStore,)*)>;
            #[expect(non_snake_case)]
            unsafe fn soft_encode_ref_unchecked<'itm>(&'a self, store: &'itm mut Self::SoftStore) -> <&'a Self as ReprC>::CType  where 'a: 'itm {
                store.encode(|fields| {
                    impl_tuple! {@decl_priv_store $($ty),*}
                    let $ffi_ty($($ty),*) = self;
                    let fields: private_store::Store<$(<&'a $ty as EncodeMoveUnchecked>::SoftStore,)*> = fields.into();
                    $ffi_ty($(unsafe { $crate::stored::encode_ref_field_unchecked($ty, fields.$ty) }),*)
                })
            }
        }

        impl<$($ty: Encode),*> Encode for ($($ty,)*) {}
        impl<$($ty: Encode),*, const STABLE: bool> Encode for $ffi_ty<$($ty),*, STABLE> {}
        impl<$($ty: crate::EncodeUnchecked),*> crate::EncodeUnchecked for ($($ty,)*) {}
        impl<$($ty: crate::EncodeUnchecked),*, const STABLE: bool> crate::EncodeUnchecked for $ffi_ty<$($ty),*, STABLE> {}

        impl<'d, $($ty: Decode<'d>),*> Decode<'d> for ($($ty,)*) {}
        impl<'d, $($ty: Decode<'d>),*, const STABLE: bool> Decode<'d> for $ffi_ty<$($ty),*, STABLE> {}

        unsafe impl<$($ty: CType),*, const STABLE: bool> CFnArg for $ffi_ty<$($ty),*, STABLE>
        where
            Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        {}
        unsafe impl<$($ty: CType),*, const STABLE: bool> CFnReturn for $ffi_ty<$($ty),*, STABLE>
        where
            Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        {}

        impl_tuple!(@split [] $($ty:$idx),+ -> $ffi_ty);
    };

    (@split [$($head:ident:$head_idx:tt,)*] $last:ident:$last_idx:tt -> $ffi_ty:ident) => {
        /// FFI-safe tuple with a stable `repr(C)` memory layout.
        ///
        /// See [the module level documentation](self) for more.
        #[repr(C)]
        #[derive(RustSpec, Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $ffi_ty<$($head,)* $last: ?Sized, const STABLE: bool = true>($(pub $head,)* pub $last);

        impl<$($head: CType,)* __Co3WideHeader: CType, const STABLE: bool> crate::wide::WideHeader for $ffi_ty<$($head,)* __Co3WideHeader, STABLE> {
            type Data = Self;
        }

        impl_tuple!(@wide_impl [$($head,)*] $last -> $ffi_ty);

        impl<$($head,)* $last, const STABLE: bool> $ffi_ty<$($head,)* $last, STABLE> {
            pub(crate) fn with_source<const OTHER: bool>(self) -> $ffi_ty<$($head,)* $last, OTHER> {
                $ffi_ty($(self.$head_idx,)* self.$last_idx)
            }

            /// Return the stored fields as a Rust tuple without decoding them.
            #[expect(non_snake_case)]
            pub fn into_tuple(self) -> ($($head,)* $last,) {
                let $ffi_ty($($head,)* $last) = self;
                ($($head,)* $last,)
            }
        }

        impl<$($head: ReprC<CType: Sized>,)* $last: ReprC + ?Sized> ReprC for ($($head,)* $last,) {
            type CType = $ffi_ty<$($head::CType,)* $last::CType, false>;
        }
        impl<$($head: ReprC<CType: Sized>,)* $last: ReprC + ?Sized, const STABLE: bool> ReprC for $ffi_ty<$($head,)* $last, STABLE> {
            type CType = $ffi_ty<$($head::CType,)* $last::CType, STABLE>;
        }

        unsafe impl<$($head: AbiCompatibleValidate<CType: Copy>,)* $last: AbiCompatibleValidate + ?Sized, const STABLE: bool> AbiCompatibleValidate for $ffi_ty<$($head,)* $last, STABLE> {
            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                let $ffi_ty($($head,)* $last) = target;
                true $(&& unsafe { $head::is_valid($head) })* && unsafe { $last::is_valid($last) }
            }
        }
        unsafe impl<$($head: AbiCompatible<CType: Copy>,)* $last: AbiCompatible + ?Sized, const STABLE: bool> AbiCompatible for $ffi_ty<$($head,)* $last, STABLE> {}

        unsafe impl<$($head: CType,)* $last: CType + ?Sized, const STABLE: bool> CType for $ffi_ty<$($head,)* $last, STABLE> {}

        unsafe impl<$($head: crate::stored::EmptyStore,)* $last: crate::stored::EmptyStore> crate::stored::EmptyStore for ($($head,)* $last,) {}
        unsafe impl<$($head: crate::stored::EmptyStore,)* $last: crate::stored::EmptyStore, const STABLE: bool> crate::stored::EmptyStore for $ffi_ty<$($head,)* $last, STABLE> {}
    };

    (@split [$($head:ident:$head_idx:tt,)*] $next:ident:$next_idx:tt, $($tail:ident:$tail_idx:tt),+ -> $ffi_ty:ident) => {
        impl_tuple!(@split [$($head:$head_idx,)* $next:$next_idx,] $($tail:$tail_idx),+ -> $ffi_ty);
    };

    (@main [$($ty:ident:$idx:tt),+] [$($head:ident:$head_idx:tt,)*] $next:ident:$next_idx:tt, $($tail:ident:$tail_idx:tt),+ -> $ffi_ty:ident) => {
        impl_tuple!(@main [$($ty:$idx),+] [$($head:$head_idx,)* $next:$next_idx,] $($tail:$tail_idx),+ -> $ffi_ty);
    };

    (@wide_impl [] $last:ident -> $ffi_ty:ident) => {};
    (@wide_impl [$($head:ident,)+] $last:ident -> $ffi_ty:ident) => {
        unsafe impl<$($head: CType,)+ $last: CType + crate::wide::Wide<Metadata = usize> + RustSpec<Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>> + ?Sized, const STABLE: bool>
            crate::wide::Wide for $ffi_ty<$($head,)+ $last, STABLE>
        where
            <$last as crate::wide::Wide>::Header: CType + Sized,
        {
            type Header = $ffi_ty<$($head,)+ <$last as crate::wide::Wide>::Header, STABLE>;
            type Metadata = usize;

            fn metadata(ptr: *const Self) -> usize {
                <$last as crate::wide::Wide>::metadata(ptr as *const $last)
            }

            fn as_ptr(ptr: *const Self) -> *const Self::Header {
                ptr as *const Self::Header
            }

            fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Header {
                ptr as *mut Self::Header
            }

            unsafe fn from_raw_parts<'a>(data: *const Self::Header, len: usize) -> &'a Self {
                let slice = core::ptr::slice_from_raw_parts(data.cast::<crate::wide::PointeeData<$last>>(), len);
                assert_eq!(core::mem::size_of::<*const Self>(), core::mem::size_of_val(&slice));
                let ptr = unsafe { core::mem::transmute_copy::<_, *const Self>(&slice) };
                unsafe { &*ptr }
            }

            unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Header, len: usize) -> &'a mut Self {
                let slice = core::ptr::slice_from_raw_parts_mut(data.cast::<crate::wide::PointeeData<$last>>(), len);
                assert_eq!(core::mem::size_of::<*mut Self>(), core::mem::size_of_val(&slice));
                let ptr = unsafe { core::mem::transmute_copy::<_, *mut Self>(&slice) };
                unsafe { &mut *ptr }
            }

            #[cfg(feature = "alloc")]
            fn into_non_null(self: alloc::boxed::Box<Self>) -> core::ptr::NonNull<Self::Header> {
                alloc::boxed::Box::into_non_null(self).cast()
            }

            #[cfg(feature = "alloc")]
            unsafe fn from_non_null(data: core::ptr::NonNull<Self::Header>, len: usize) -> alloc::boxed::Box<Self> {
                let slice = core::ptr::slice_from_raw_parts_mut(data.as_ptr().cast::<crate::wide::PointeeData<$last>>(), len);
                assert_eq!(core::mem::size_of::<*mut Self>(), core::mem::size_of_val(&slice));
                let ptr = unsafe { core::mem::transmute_copy::<_, *mut Self>(&slice) };
                unsafe { alloc::boxed::Box::from_raw(ptr) }
            }
        }
    };

    // NOTE: This is a trick to index tuples
    ( @decl_priv_store $( $ty:ident ),+) => {
        // Field names mirror type parameters so the macro can address tuple stores.
        #[allow(non_snake_case)]
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

impl_tuple! {(A:0) -> Tuple1}
impl_tuple! {(A:0, B:1) -> Tuple2}
impl_tuple! {(A:0, B:1, C:2) -> Tuple3}
impl_tuple! {(A:0, B:1, C:2, D:3) -> Tuple4}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4) -> Tuple5}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5) -> Tuple6}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6) -> Tuple7}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7) -> Tuple8}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8) -> Tuple9}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9) -> Tuple10}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10) -> Tuple11}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11) -> Tuple12}

impl<A: Niche> Niche for (A,) {
    const NICHE: Self::CType = Tuple1(A::NICHE);

    fn is_niche(value: &Self::CType) -> bool {
        A::is_niche(&value.0)
    }
}

impl<A: ReprC<CType: Copy>, B: ReprC<CType: Copy>, C: ReprC<CType: Copy>> Niche for (A, B, C)
where
    (A, (B, C)):
        Niche<CType = Tuple2<<A as crate::ReprC>::CType, <(B, C) as crate::ReprC>::CType, false>>,
{
    const NICHE: Self::CType = Tuple3(
        <(A, (B, C))>::NICHE.0,
        <(A, (B, C))>::NICHE.1.0,
        <(A, (B, C))>::NICHE.1.1,
    );

    fn is_niche(value: &Self::CType) -> bool {
        <(A, (B, C)) as Niche>::is_niche(&Tuple2(value.0, Tuple2(value.1, value.2)))
    }
}

macro_rules! impl_tuple_niche_recursive {
    ($(($($all:ident),+) => ($left:ty, $right:ty) : $ffi_ty:ident($($field:tt),+): ($left_val:expr, $right_val:expr)),+ $(,)?) => {
        $(
            impl<$($all: ReprC<CType: Copy>),+> Niche for ($($all,)+)
            where
                ($left, $right): Niche<CType = Tuple2<<$left as crate::ReprC>::CType, <$right as crate::ReprC>::CType, false>>,
            {
                const NICHE: Self::CType = $ffi_ty(
                    $(<($left, $right)>::NICHE.$field),+
                );

                fn is_niche(value: &Self::CType) -> bool {
                    <($left, $right) as Niche>::is_niche(&Tuple2(($left_val)(value), ($right_val)(value)))
                }
            }
        )+
    };
}

impl_tuple_niche_recursive! {
    (A, B, C, D) => ((A, B), (C, D)) : Tuple4(0.0, 0.1, 1.0, 1.1): (|value: &Self::CType| Tuple2(value.0, value.1), |value: &Self::CType| Tuple2(value.2, value.3)),
    (A, B, C, D, E) => ((A, B), (C, D, E)) : Tuple5(0.0, 0.1, 1.0, 1.1, 1.2): (|value: &Self::CType| Tuple2(value.0, value.1), |value: &Self::CType| Tuple3(value.2, value.3, value.4)),
    (A, B, C, D, E, F) => ((A, B, C), (D, E, F)) : Tuple6(0.0, 0.1, 0.2, 1.0, 1.1, 1.2): (|value: &Self::CType| Tuple3(value.0, value.1, value.2), |value: &Self::CType| Tuple3(value.3, value.4, value.5)),
    (A, B, C, D, E, F, G) => ((A, B, C), (D, E, F, G)) : Tuple7(0.0, 0.1, 0.2, 1.0, 1.1, 1.2, 1.3): (|value: &Self::CType| Tuple3(value.0, value.1, value.2), |value: &Self::CType| Tuple4(value.3, value.4, value.5, value.6)),
    (A, B, C, D, E, F, G, H) => ((A, B, C, D), (E, F, G, H)) : Tuple8(0.0, 0.1, 0.2, 0.3, 1.0, 1.1, 1.2, 1.3): (|value: &Self::CType| Tuple4(value.0, value.1, value.2, value.3), |value: &Self::CType| Tuple4(value.4, value.5, value.6, value.7)),
    (A, B, C, D, E, F, G, H, I) => ((A, B, C, D), (E, F, G, H, I)) : Tuple9(0.0, 0.1, 0.2, 0.3, 1.0, 1.1, 1.2, 1.3, 1.4): (|value: &Self::CType| Tuple4(value.0, value.1, value.2, value.3), |value: &Self::CType| Tuple5(value.4, value.5, value.6, value.7, value.8)),
    (A, B, C, D, E, F, G, H, I, J) => ((A, B, C, D, E), (F, G, H, I, J)) : Tuple10(0.0, 0.1, 0.2, 0.3, 0.4, 1.0, 1.1, 1.2, 1.3, 1.4): (|value: &Self::CType| Tuple5(value.0, value.1, value.2, value.3, value.4), |value: &Self::CType| Tuple5(value.5, value.6, value.7, value.8, value.9)),
    (A, B, C, D, E, F, G, H, I, J, K) => ((A, B, C, D, E), (F, G, H, I, J, K)) : Tuple11(0.0, 0.1, 0.2, 0.3, 0.4, 1.0, 1.1, 1.2, 1.3, 1.4, 1.5): (|value: &Self::CType| Tuple5(value.0, value.1, value.2, value.3, value.4), |value: &Self::CType| Tuple6(value.5, value.6, value.7, value.8, value.9, value.10)),
    (A, B, C, D, E, F, G, H, I, J, K, L) => ((A, B, C, D, E, F), (G, H, I, J, K, L)) : Tuple12(0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 1.0, 1.1, 1.2, 1.3, 1.4, 1.5): (|value: &Self::CType| Tuple6(value.0, value.1, value.2, value.3, value.4, value.5), |value: &Self::CType| Tuple6(value.6, value.7, value.8, value.9, value.10, value.11))
}

#[cfg(test)]
mod tests {
    #[test]
    fn tuple_marker_survives_lowering_and_decode() {
        use crate::{decode, encode, primitives::CBool, tuple::Tuple2};
        let stable: Tuple2<_, _> = Tuple2(23_u32, true);
        let encoded: Tuple2<u32, CBool> = encode(stable);
        assert_eq!(
            unsafe { decode::<Tuple2<u32, bool>>(encoded) },
            Some(stable)
        );

        let native_marker: Tuple2<_, _, false> = Tuple2(23_u32, true);
        let encoded: Tuple2<u32, CBool, false> = encode(native_marker);
        assert_eq!(
            unsafe { decode::<Tuple2<u32, bool, false>>(encoded) },
            Some(native_marker)
        );
        assert_eq!(encode((23_u32, true)), encoded);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn wide_tuple_box_preserves_allocation_and_metadata() {
        use crate::wide::Wide;

        fn round_trip<const N: usize>(tail: [u64; N]) {
            let value: alloc::boxed::Box<super::Tuple2<u8, [u64]>> =
                alloc::boxed::Box::new(super::Tuple2(7, tail));
            let original = core::ptr::from_ref(&*value);
            let len = <super::Tuple2<u8, [u64]> as Wide>::metadata(original);
            let data = Wide::into_non_null(value);
            let value = unsafe { <super::Tuple2<u8, [u64]> as Wide>::from_non_null(data, len) };
            assert!(core::ptr::eq(original, &*value));
            assert_eq!(value.0, 7);
            assert_eq!(&value.1, &tail);
        }

        round_trip([]);
        round_trip([11, 22, 33]);
    }

    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, vec::Vec};
    use core::num::NonZero as StdNonZero;

    use rust_spec::size::{Sized as Co3Sized, Zero};
    use static_assertions::assert_impl_all;
    #[cfg(feature = "alloc")]
    use static_assertions::assert_not_impl_any;

    use super::*;
    use crate::{
        Decode, Encode,
        option::COption,
        reference::{CSoftRef, CSoftRefMut},
    };
    #[cfg(feature = "alloc")]
    use crate::{boxed::CBox, wide::CWideBox};
    #[cfg(feature = "alloc")]
    use crate::{
        stored::{DecodeMove, EncodeMove},
        wide::{CSoftWideRef, CSoftWideRefMut},
    };

    #[test]
    fn tuple_size_family_tracks_zst_fields() {
        assert_impl_all!(Tuple2<(), ()>: RustSpec<Size = Co3Sized<Zero>>);
        assert_impl_all!(Tuple3<(), (), ()>: RustSpec<Size = Co3Sized<Zero>>);
    }

    #[test]
    fn tuple_size_family_tracks_non_zst_fields() {
        assert_impl_all!(Tuple2<(), u8>: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>);
        assert_impl_all!(Tuple2<u8, ()>: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>);
        assert_impl_all!(Tuple3<(), u8, ()>: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>);
    }

    #[test]
    fn stored_tuple_3_without_niche() {
        assert_impl_all!((u8, u8, u8):
            ReprC<CType = Tuple3<<u8 as crate::ReprC>::CType, <u8 as crate::ReprC>::CType, <u8 as crate::ReprC>::CType, false>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(&(u8, u8, u8):
            Niche<CType = CSoftRef<(u8, u8, u8)>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut (u8, u8, u8):
        Niche<CType = CSoftRefMut<(u8, u8, u8), true>>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<(u8, u8, u8)>:
            Niche<CType = CBox<(u8, u8, u8)>>,
            EncodeMove,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&[(u8, u8, u8)]:
            Niche<CType = CSoftWideRef<[(u8, u8, u8)]>>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [(u8, u8, u8)]:
            Niche<CType = CSoftWideRefMut<[(u8, u8, u8)], true>>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[(u8, u8, u8)]>:
            Niche<CType = CWideBox<[(u8, u8, u8)]>>,
            EncodeMove,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, u8, u8)>:
            Niche<CType = crate::vec::CVec<(u8, u8, u8)>>,
            DecodeMove<'static>,
            EncodeMove,
        );
        assert_impl_all!([(u8, u8, u8); 2]:
            ReprC<CType = [Tuple3<<u8 as crate::ReprC>::CType, <u8 as crate::ReprC>::CType, <u8 as crate::ReprC>::CType, false>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<(u8, u8, u8)>:
            Niche<CType = COption<(u8, u8, u8)>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<[(u8, u8, u8)]>: Encode, Decode<'static>);
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, u8, u8)>: Encode, Decode<'static>);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn unstable_boxes_do_not_expose_public_conversion() {
        assert_not_impl_any!(Box<(u8, u8, u8)>: Encode, Decode<'static>);
        assert_not_impl_any!(Box<[(u8, u8, u8)]>: Encode, Decode<'static>);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn unstable_sized_box_decode_releases_source_allocation() {
        type Value = Box<(u8, u8, u8)>;
        let source = crate::stored::encode_move::<Value>(Box::new((1, 2, 3)));
        let decoded = unsafe { crate::stored::decode_move::<Value>(source) };
        assert_eq!(decoded, Some(Box::new((1, 2, 3))));
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn unstable_slice_box_owned_round_trip() {
        type Value = Box<[(u8, u8, u8)]>;
        let input: Value = Vec::from([(1, 2, 3), (4, 5, 6)]).into_boxed_slice();
        let source = crate::stored::encode_move(input);
        let decoded = unsafe { crate::stored::decode_move::<Value>(source) };
        assert_eq!(
            decoded,
            Some(Vec::from([(1, 2, 3), (4, 5, 6)]).into_boxed_slice())
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn unstable_boxed_slice_uses_box_allocation_layout() {
        let value = Vec::from([(1u32,), (2,)]).into_boxed_slice();
        let carrier = crate::stored::encode_move(value);
        let decoded = unsafe { crate::stored::decode_move::<Box<[(u32,)]>>(carrier) }.unwrap();
        assert_eq!(&*decoded, &[(1,), (2,)]);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn unstable_interior_box_checked_round_trip() {
        use core::cell::Cell;
        let encoded = crate::stored::encode_move(Box::new(Cell::new((7_u8,))));
        let decoded = unsafe { crate::stored::decode_move::<Box<Cell<(u8,)>>>(encoded) }.unwrap();
        assert_eq!(decoded.get(), (7,));
    }

    #[test]
    fn stored_tuple_3_with_niche() {
        // NOTE: Confirms niche is taken from the first available element
        let niche = <(u8, StdNonZero<u8>, bool)>::NICHE;
        assert_eq!(
            (niche.0, niche.1.0, niche.2),
            (0, 0, crate::primitives::CBool::FALSE)
        );

        assert_impl_all!((u8, StdNonZero<u8>, bool):
            Niche<CType = Tuple3<<u8 as crate::ReprC>::CType, <core::num::NonZeroU8 as crate::ReprC>::CType, <bool as crate::ReprC>::CType, false>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(&(u8, StdNonZero<u8>, bool):
            Niche<CType = CSoftRef<(u8, StdNonZero<u8>, bool)>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut (u8, StdNonZero<u8>, bool):
            Niche<CType = CSoftRefMut<(u8, StdNonZero<u8>, bool), true>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<(u8, StdNonZero<u8>, bool)>:
            Niche<CType = CBox<(u8, StdNonZero<u8>, bool)>>,
            DecodeMove<'static>,
            EncodeMove,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&[(u8, StdNonZero<u8>, bool)]:
            Niche<CType = CSoftWideRef<[(u8, StdNonZero<u8>, bool)]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [(u8, StdNonZero<u8>, bool)]:
            Niche<CType = CSoftWideRefMut<[(u8, StdNonZero<u8>, bool)], true>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[(u8, StdNonZero<u8>, bool)]>:
            Niche<CType = CWideBox<[(u8, StdNonZero<u8>, bool)]>>,
            DecodeMove<'static>,
            EncodeMove,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, StdNonZero<u8>, bool)>:
            Niche<CType = crate::vec::CVec<(u8, core::num::NonZeroU8, bool)>>,
            DecodeMove<'static>,
            EncodeMove,
        );
        assert_impl_all!([(u8, StdNonZero<u8>, bool); 2]:
            Niche<CType = [Tuple3<<u8 as crate::ReprC>::CType, <core::num::NonZeroU8 as crate::ReprC>::CType, <bool as crate::ReprC>::CType, false>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<(u8, StdNonZero<u8>, bool)>:
            // TODO: Depends on: https://github.com/mversic/co3/issues/33
            //Niche<CType = Tuple3<<u8 as crate::ReprC>::CType, <u8 as crate::ReprC>::CType, <u8 as crate::ReprC>::CType, false>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<[(u8, StdNonZero<u8>, u8)]>: Encode, Decode<'static>);
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, StdNonZero<u8>, u8)>: Encode, Decode<'static>);
    }
}
