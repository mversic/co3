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
//! When no tuple element has a niche value, `Option<(A, B, ...)>` is represented as tagged.
//!
//! # Example
//!
//! ```rust
//! use core::mem::size_of;
//!
//! use co3::{
//!     encode, niche::Niche, option::abi, ReprC,
//! };
//!
//! type TupleWithNiche1<'a> = (u8, bool, &'a bool);
//! type TupleWithNiche2<'a> = (u8, &'a bool, bool);
//! type TupleWithoutNiche = (u64, u32, u8);
//!
//! assert_eq!(
//!     size_of::<<Option<TupleWithNiche1> as ReprC>::CType>(),
//!     size_of::<<TupleWithNiche1 as ReprC>::CType>(),
//! );
//! assert_eq!(
//!     size_of::<<Option<TupleWithNiche2> as ReprC>::CType>(),
//!     size_of::<<TupleWithNiche2 as ReprC>::CType>(),
//! );
//!
//! assert_eq!(
//!     size_of::<<Option<TupleWithoutNiche> as ReprC>::CType>(),
//!     8 + size_of::<<TupleWithoutNiche as ReprC>::CType>(),
//! );
//!
//! let none_value_1: Option<TupleWithNiche1> = None;
//! let encoded_none_value_1 = encode(none_value_1);
//! let payload_1 = &encoded_none_value_1.0.0.0;
//! assert_eq!(payload_1.0, 0);
//! assert!(bool::is_niche(&payload_1.1));
//! assert!(payload_1.2.as_ptr().is_null());
//!
//! let none_value_2: Option<TupleWithNiche2> = None;
//! let encoded_none_value_2 = encode(none_value_2);
//! let payload_2 = &encoded_none_value_2.0.0.0;
//! assert_eq!(payload_2.0, 0);
//! assert!(payload_2.1.as_ptr().is_null());
//! assert_eq!(payload_2.2, co3::primitive::abi::bool::FALSE);
//!
//! let none_value_3: Option<TupleWithoutNiche> = None;
//! assert_eq!(encode(none_value_3).0, abi::Option::None());
//! ```

use rust_spec::{
    RustSpec,
    mutability::{Exclusive, ExclusiveKind},
};

use crate::{
    CFnArg, CFnReturn, CType, ReprC, Store,
    convert::{
        AbiCompatibleValidate, Decode, DecodeMove, Encode, EncodeMove, EncodeMoveUnchecked,
        SoftEncodeRef, SoftEncodeRefUnchecked,
    },
    marker::{AbiCompatible, Native},
    niche::Niche,
    reference::abi::RefCarrier,
};

macro_rules! impl_tuple {
    (@codecs [$($ty:ident:$idx:tt),+] -> $ffi_ty:ident, $carrier:ident, $encode:ident, $unchecked:ident, $decode:ident) => {
        unsafe impl<$($ty: $encode),*> $encode for ($($ty,)*) {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
            where
                Self: 'itm
            {
                let ($($ty,)*) = self;
                Native($ffi_ty::<$($ty),*>($($ty),*).soft_encode(store))
            }

        }
        unsafe impl<$($ty: $encode),*> $encode for $ffi_ty<$($ty),*> {
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
                abi::$carrier($( $encode::soft_encode($ty, store.$ty) ),*)
            }

        }
        unsafe impl<$($ty: $unchecked),*> $unchecked for ($($ty,)*) {
            type SoftStore = ($($ty::SoftStore,)*);

            #[expect(non_snake_case)]
            unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
                let ($($ty,)*) = self;
                Native(unsafe { $ffi_ty::<$($ty),*>($($ty),*).soft_encode_unchecked(store) })
            }
        }
        unsafe impl<$($ty: $unchecked),*> $unchecked for $ffi_ty<$($ty),*> {
            type SoftStore = ($($ty::SoftStore,)*);

            #[expect(non_snake_case)]
            unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
                impl_tuple! {@decl_priv_store $($ty),*}

                let $ffi_ty($($ty),*) = self;
                let store: private_store::Store<$($ty::SoftStore,)*> = store.into();
                abi::$carrier($(unsafe { $ty.soft_encode_unchecked(store.$ty) }),*)
            }
        }
        unsafe impl<'d, $($ty: $decode<'d>),*> $decode<'d> for ($($ty,)*) {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                let $ffi_ty($($ty),*) = unsafe {
                    <$ffi_ty<$($ty),*> as $decode<'d>>::soft_decode(source.0, store)
                }?;
                Some(($($ty,)*))
            }

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
                let $ffi_ty($($ty),*) = unsafe {
                    <$ffi_ty<$($ty),*> as $decode<'d>>::soft_decode_unchecked(source.0, store)
                };
                ($($ty,)*)
            }
        }
        unsafe impl<'d, $($ty: $decode<'d>),*> $decode<'d> for $ffi_ty<$($ty),*> {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                impl_tuple! {@decl_priv_store $($ty),*}

                let abi::$carrier($($ty),*) = source;
                let store: private_store::Store<$($ty::Store,)*> = store.into();
                Some(unsafe { $ffi_ty($( $ty::soft_decode($ty, store.$ty)? ),*) })
            }

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
                impl_tuple! {@decl_priv_store $($ty),*}

                let abi::$carrier($($ty),*) = source;
                let store: private_store::Store<$($ty::Store),*> = store.into();
                unsafe { $ffi_ty($( $ty::soft_decode_unchecked($ty, store.$ty) ),*) }
            }
        }
    };

    ( ($( $ty:ident : $idx:tt ),+) -> $ffi_ty:ident, $carrier:ident ) => {
        impl_tuple!(@main [$($ty:$idx),+] [] $($ty:$idx),+ -> $ffi_ty, $carrier);
    };

    (@main [$($ty:ident:$idx:tt),+] [$($head:ident:$head_idx:tt,)*] $last:ident:$last_idx:tt -> $ffi_ty:ident, $carrier:ident) => {
        impl_tuple!(@codecs [$($ty:$idx),+] -> $ffi_ty, $carrier, EncodeMove, EncodeMoveUnchecked, DecodeMove);
        const _: () = {
            use crate::borrow::{BorrowedEncode, BorrowedEncodeUnchecked, BorrowedDecode};
            unsafe impl<$($ty: BorrowedEncode),*> BorrowedEncode for ($($ty,)*) {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            fn soft_encode<'itm>(self, store: &mut Self::Store, owner: &'itm mut Self::Owner) -> Self::CType
            where
                Self: 'itm
            {
                let ($($ty,)*) = self;
                Native($ffi_ty::<$($ty),*>($($ty),*).soft_encode(store, owner))
            }

        }
        unsafe impl<$($ty: BorrowedEncode),*> BorrowedEncode for $ffi_ty<$($ty),*> {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            fn soft_encode<'itm>(self, store: &'itm mut Self::Store, owner: &'itm mut Self::Owner) -> Self::CType
            where
                Self: 'itm,
            {
                impl_tuple! {@decl_priv_store $($ty),*}

                let $ffi_ty($($ty),*) = self;
                let store: private_store::Store<$($ty::Store,)*> = store.into();
                        let owner: private_store::Store<$($ty::Owner,)*> = owner.into();
                abi::$carrier($( BorrowedEncode::soft_encode($ty, store.$ty, owner.$ty) ),*)
            }

        }
        unsafe impl<$($ty: BorrowedEncodeUnchecked),*> BorrowedEncodeUnchecked for ($($ty,)*) {
            type Owner = ($($ty::Owner,)*);
            type SoftStore = ($($ty::SoftStore,)*);

            #[expect(non_snake_case)]
            unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore, owner: &'itm mut Self::Owner) -> Self::CType where Self: 'itm {
                let ($($ty,)*) = self;
                Native(unsafe { $ffi_ty::<$($ty),*>($($ty),*).soft_encode_unchecked(store, owner) })
            }
        }
        unsafe impl<$($ty: BorrowedEncodeUnchecked),*> BorrowedEncodeUnchecked for $ffi_ty<$($ty),*> {
            type Owner = ($($ty::Owner,)*);
            type SoftStore = ($($ty::SoftStore,)*);

            #[expect(non_snake_case)]
            unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore, owner: &'itm mut Self::Owner) -> Self::CType where Self: 'itm {
                impl_tuple! {@decl_priv_store $($ty),*}

                let $ffi_ty($($ty),*) = self;
                let store: private_store::Store<$($ty::SoftStore,)*> = store.into();
                        let owner: private_store::Store<$($ty::Owner,)*> = owner.into();
                abi::$carrier($(unsafe { $ty.soft_encode_unchecked(store.$ty, owner.$ty) }),*)
            }
        }
        unsafe impl<'d, $($ty: BorrowedDecode<'d>),*> BorrowedDecode<'d> for ($($ty,)*) {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                let $ffi_ty($($ty),*) = unsafe {
                    <$ffi_ty<$($ty),*> as BorrowedDecode<'d>>::soft_decode(source.0, store)
                }?;
                Some(($($ty,)*))
            }

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
                let $ffi_ty($($ty),*) = unsafe {
                    <$ffi_ty<$($ty),*> as BorrowedDecode<'d>>::soft_decode_unchecked(source.0, store)
                };
                ($($ty,)*)
            }
        }
        unsafe impl<'d, $($ty: BorrowedDecode<'d>),*> BorrowedDecode<'d> for $ffi_ty<$($ty),*> {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                impl_tuple! {@decl_priv_store $($ty),*}

                let abi::$carrier($($ty),*) = source;
                let store: private_store::Store<$($ty::Store,)*> = store.into();
                Some(unsafe { $ffi_ty($( $ty::soft_decode($ty, store.$ty)? ),*) })
            }

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
                impl_tuple! {@decl_priv_store $($ty),*}

                let abi::$carrier($($ty),*) = source;
                let store: private_store::Store<$($ty::Store),*> = store.into();
                unsafe { $ffi_ty($( $ty::soft_decode_unchecked($ty, store.$ty) ),*) }
            }
        }
        };
        unsafe impl<'d, $($ty: ReprC<CType: crate::CType + Sized> + RustSpec + 'd),+> crate::convert::SoftDecodeRef<'d> for ($($ty,)*)
        where
            Self: RustSpec + ReprC<CType = Native<abi::$carrier<$(<$ty as ReprC>::CType),*>>>,
            &'d Self: ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
            $(&'d $ty: DecodeMove<'d> + ReprC<CType: RefCarrier<Pointee = <$ty as ReprC>::CType>>,)+
            $(crate::sync::DecodedField<
                'd, $ty, <&'d $ty as DecodeMove<'d>>::Store, <$ty as RustSpec>::Layout,
            >: crate::sync::TakeDecoded<Value = $ty>,)+
        {
            type Store = crate::sync::RefSizedDecodeStore<Self, ($(<&'d $ty as DecodeMove<'d>>::Store,)*)>;
            unsafe fn soft_decode_ref<'itm: 'd>(source: <&'d Self as ReprC>::CType, store: &'itm mut Self::Store) -> Option<&'d Self> {
                let source = unsafe { source.into_ref()? };
                store.decode(|fields| Some(($(
                    unsafe {
                        crate::decoded::decode_ref_field::<$ty>(
                            RefCarrier::from_raw(&source.0.$idx),
                            &mut fields.$idx,
                        )
                    }?,
                )*)))
            }
            unsafe fn soft_decode_ref_unchecked<'itm: 'd>(source: <&'d Self as ReprC>::CType, store: &'itm mut Self::Store) -> &'d Self {
                let source = unsafe { source.into_ref().unwrap_unchecked() };
                store.decode(|fields| Some(($(
                    unsafe {
                        crate::decoded::decode_ref_field_unchecked::<$ty>(
                            RefCarrier::from_raw(&source.0.$idx),
                            &mut fields.$idx,
                        )
                    },
                )*))).expect("unchecked decoding produces a value")
            }
        }

        unsafe impl<'d, $($ty: ReprC<CType: crate::CType + Sized> + RustSpec + 'd),+> crate::convert::SoftDecodeRef<'d> for $ffi_ty<$($ty),*>
        where
            Self: RustSpec + ReprC<CType = abi::$carrier<$(<$ty as ReprC>::CType),*>>,
            &'d Self: ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
            $(&'d $ty: DecodeMove<'d> + ReprC<CType: RefCarrier<Pointee = <$ty as ReprC>::CType>>,)+
            $(crate::sync::DecodedField<
                'd, $ty, <&'d $ty as DecodeMove<'d>>::Store, <$ty as RustSpec>::Layout,
            >: crate::sync::TakeDecoded<Value = $ty>,)+
        {
            type Store = crate::sync::RefSizedDecodeStore<Self, ($(<&'d $ty as DecodeMove<'d>>::Store,)*)>;
            unsafe fn soft_decode_ref<'itm: 'd>(source: <&'d Self as ReprC>::CType, store: &'itm mut Self::Store) -> Option<&'d Self> {
                let source = unsafe { source.into_ref()? };
                store.decode(|fields| Some($ffi_ty($(
                    unsafe {
                        crate::decoded::decode_ref_field::<$ty>(
                            RefCarrier::from_raw(&source.$idx),
                            &mut fields.$idx,
                        )
                    }?,
                )*)))
            }
            unsafe fn soft_decode_ref_unchecked<'itm: 'd>(source: <&'d Self as ReprC>::CType, store: &'itm mut Self::Store) -> &'d Self {
                let source = unsafe { source.into_ref().unwrap_unchecked() };
                store.decode(|fields| Some($ffi_ty($(
                    unsafe {
                        crate::decoded::decode_ref_field_unchecked::<$ty>(
                            RefCarrier::from_raw(&source.$idx),
                            &mut fields.$idx,
                        )
                    },
                )*))).expect("unchecked decoding produces a value")
            }
        }
        unsafe impl<'d, $($ty: ReprC<CType: crate::CType + Copy> + 'd),+> crate::convert::SoftDecodeMut<'d> for ($($ty,)*)
        where
            $($ty: crate::borrow::BorrowedDecode<'d>,)*
            Self: EncodeMove<Store: crate::convert::EmptyStore> + RustSpec + ReprC<CType = Native<abi::$carrier<$(<$ty as ReprC>::CType),*>>>,
            &'d mut Self: ReprC<CType = crate::reference::abi::SoftRefMut<<Self as ReprC>::CType>>,
        {
            type Store = crate::sync::RefMutSizedDecodeStore<Self, ($(<$ty as crate::borrow::BorrowedDecode<'d>>::Store,)*)>;
            unsafe fn soft_decode_mut<'itm: 'd>(source: <&'d mut Self as ReprC>::CType, store: &'itm mut Self::Store) -> Option<&'d mut Self> {
                let pointer = source.as_ptr();
                let source = unsafe { pointer.as_ref()? };
                unsafe { store.decode(pointer, |fields| Some(($(
                    crate::decoded::decode_mut_field::<$ty>(source.0.$idx, &mut fields.$idx)?,
                )*))) }
            }
            unsafe fn soft_decode_mut_unchecked<'itm: 'd>(source: <&'d mut Self as ReprC>::CType, store: &'itm mut Self::Store) -> &'d mut Self {
                let pointer = source.as_ptr();
                let source = unsafe { &*pointer };
                unsafe { store.decode(pointer, |fields| Some(($(
                    crate::decoded::decode_mut_field_unchecked::<$ty>(source.0.$idx, &mut fields.$idx),
                )*))) }.expect("unchecked decoding produces a value")
            }
        }

        unsafe impl<'d, $($ty: ReprC<CType: crate::CType + Copy> + 'd),+> crate::convert::SoftDecodeMut<'d> for $ffi_ty<$($ty),*>
        where
            $($ty: crate::borrow::BorrowedDecode<'d>,)*
            Self: EncodeMove<Store: crate::convert::EmptyStore> + RustSpec + ReprC<CType = abi::$carrier<$(<$ty as ReprC>::CType),*>>,
            &'d mut Self: ReprC<CType = crate::reference::abi::SoftRefMut<<Self as ReprC>::CType>>,
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
        &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
            Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
            $(&'a $ty: EncodeMove + ReprC<CType: RefCarrier<Pointee = <$ty as ReprC>::CType>>),*
        {
            type Store = crate::sync::RefSizedEncodeStore<Self, ($(<&'a $ty as EncodeMove>::Store,)*)>;
            #[expect(non_snake_case)]
            fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType  where 'a: 'itm {
                store.encode(|fields| {
                    impl_tuple! {@decl_priv_store $($ty),*}
                    let ($($ty,)*) = self;
                    let fields: private_store::Store<$(<&'a $ty as EncodeMove>::Store,)*> = fields.into();
                    Native(abi::$carrier($($crate::stored::encode_ref_field($ty, fields.$ty)),*))
                })
            }
        }

        unsafe impl<'a, $($ty: ReprC<CType: Copy> + 'a),*, __Interior: ExclusiveKind> SoftEncodeRef<'a> for $ffi_ty<$($ty),*>
        where
        &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
            Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
            $(&'a $ty: EncodeMove + ReprC<CType: RefCarrier<Pointee = <$ty as ReprC>::CType>>),*
        {
            type Store = crate::sync::RefSizedEncodeStore<Self, ($(<&'a $ty as EncodeMove>::Store,)*)>;
            #[expect(non_snake_case)]
            fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType  where 'a: 'itm {
                store.encode(|fields| {
                    impl_tuple! {@decl_priv_store $($ty),*}
                    let $ffi_ty($($ty),*) = self;
                    let fields: private_store::Store<$(<&'a $ty as EncodeMove>::Store,)*> = fields.into();
                    abi::$carrier($($crate::stored::encode_ref_field($ty, fields.$ty)),*)
                })
            }
        }

        unsafe impl<'a, $($ty: ReprC<CType: Copy> + 'a),*, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a> for ($($ty,)*)
        where
        &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
            Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
            $(&'a $ty: EncodeMoveUnchecked + ReprC<CType: RefCarrier<Pointee = <$ty as ReprC>::CType>>),*
        {
            type SoftStore = crate::sync::RefSizedEncodeStore<Self, ($(<&'a $ty as EncodeMoveUnchecked>::SoftStore,)*)>;
            #[expect(non_snake_case)]
            unsafe fn soft_encode_ref_unchecked<'itm>(&'a self, store: &'itm mut Self::SoftStore) -> <&'a Self as ReprC>::CType  where 'a: 'itm {
                store.encode(|fields| {
                    impl_tuple! {@decl_priv_store $($ty),*}
                    let ($($ty,)*) = self;
                    let fields: private_store::Store<$(<&'a $ty as EncodeMoveUnchecked>::SoftStore,)*> = fields.into();
                    Native(abi::$carrier($(unsafe { $crate::stored::encode_ref_field_unchecked($ty, fields.$ty) }),*))
                })
            }
        }

        unsafe impl<'a, $($ty: ReprC<CType: Copy> + 'a),*, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a> for $ffi_ty<$($ty),*>
        where
        &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
            Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
            $(&'a $ty: EncodeMoveUnchecked + ReprC<CType: RefCarrier<Pointee = <$ty as ReprC>::CType>>),*
        {
            type SoftStore = crate::sync::RefSizedEncodeStore<Self, ($(<&'a $ty as EncodeMoveUnchecked>::SoftStore,)*)>;
            #[expect(non_snake_case)]
            unsafe fn soft_encode_ref_unchecked<'itm>(&'a self, store: &'itm mut Self::SoftStore) -> <&'a Self as ReprC>::CType  where 'a: 'itm {
                store.encode(|fields| {
                    impl_tuple! {@decl_priv_store $($ty),*}
                    let $ffi_ty($($ty),*) = self;
                    let fields: private_store::Store<$(<&'a $ty as EncodeMoveUnchecked>::SoftStore,)*> = fields.into();
                    abi::$carrier($(unsafe { $crate::stored::encode_ref_field_unchecked($ty, fields.$ty) }),*)
                })
            }
        }

        impl<$($ty: Encode),*> Encode for ($($ty,)*) {}
        impl<$($ty: Encode),*> Encode for $ffi_ty<$($ty),*> {}
        impl<$($ty: crate::convert::EncodeUnchecked),*> crate::convert::EncodeUnchecked for ($($ty,)*) {}
        impl<$($ty: crate::convert::EncodeUnchecked),*> crate::convert::EncodeUnchecked for $ffi_ty<$($ty),*> {}

        impl<'d, $($ty: Decode<'d>),*> Decode<'d> for ($($ty,)*) {}
        impl<'d, $($ty: Decode<'d>),*> Decode<'d> for $ffi_ty<$($ty),*> {}

        unsafe impl<$($ty: CType),*> CFnArg for $ffi_ty<$($ty),*>
        where
            Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        {}
        unsafe impl<$($ty: CType),*> CFnReturn for $ffi_ty<$($ty),*>
        where
            Self: crate::abi::CFnReturnLayout,
        {}

        impl_tuple!(@split [] $($ty:$idx),+ -> $ffi_ty, $carrier);
    };

    (@split [$($head:ident:$head_idx:tt,)*] $last:ident:$last_idx:tt -> $ffi_ty:ident, $carrier:ident) => {
        /// FFI-safe tuple with a stable `repr(C)` memory layout.
        ///
        /// See [the module level documentation](self) for more.
        #[repr(C)]
        #[derive(RustSpec, Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $ffi_ty<$($head,)* $last: ?Sized>($(pub $head,)* pub $last);

        impl<$($head: CType,)* __Co3WideHeader: CType> crate::wide::WideHeader for $ffi_ty<$($head,)* __Co3WideHeader> {
            type Data = Self;
        }

        impl_tuple!(@wide_impl [$($head,)*] $last -> $ffi_ty, $carrier);

        impl<$($head,)* $last> $ffi_ty<$($head,)* $last> {
            /// Return the stored fields as a Rust tuple without decoding them.
            #[expect(non_snake_case)]
            pub fn into_tuple(self) -> ($($head,)* $last,) {
                let $ffi_ty($($head,)* $last) = self;
                ($($head,)* $last,)
            }
        }

        impl<$($head: ReprC<CType: Sized>,)* $last: ReprC + ?Sized> ReprC for ($($head,)* $last,) {
            type CType = Native<abi::$carrier<$($head::CType,)* $last::CType>>;
        }
        impl<$($head: ReprC<CType: Sized>,)* $last: ReprC + ?Sized> ReprC for $ffi_ty<$($head,)* $last> {
            type CType = abi::$carrier<$($head::CType,)* $last::CType>;
        }

        unsafe impl<$($head: AbiCompatibleValidate<CType: Copy>,)* $last: AbiCompatibleValidate + ?Sized> AbiCompatibleValidate for $ffi_ty<$($head,)* $last> {
            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                let abi::$carrier($($head,)* $last) = target;
                true $(&& unsafe { $head::is_valid($head) })* && unsafe { $last::is_valid($last) }
            }
        }
        unsafe impl<$($head: AbiCompatible<CType: Copy>,)* $last: AbiCompatible + ?Sized> AbiCompatible for $ffi_ty<$($head,)* $last> {}

        unsafe impl<$($head: CType,)* $last: CType + ?Sized> CType for $ffi_ty<$($head,)* $last> {}

        unsafe impl<$($head: crate::convert::EmptyStore,)* $last: crate::convert::EmptyStore> crate::convert::EmptyStore for ($($head,)* $last,) {}
        unsafe impl<$($head: crate::convert::EmptyStore,)* $last: crate::convert::EmptyStore> crate::convert::EmptyStore for $ffi_ty<$($head,)* $last> {}
    };

    (@split [$($head:ident:$head_idx:tt,)*] $next:ident:$next_idx:tt, $($tail:ident:$tail_idx:tt),+ -> $ffi_ty:ident, $carrier:ident) => {
        impl_tuple!(@split [$($head:$head_idx,)* $next:$next_idx,] $($tail:$tail_idx),+ -> $ffi_ty, $carrier);
    };

    (@main [$($ty:ident:$idx:tt),+] [$($head:ident:$head_idx:tt,)*] $next:ident:$next_idx:tt, $($tail:ident:$tail_idx:tt),+ -> $ffi_ty:ident, $carrier:ident) => {
        impl_tuple!(@main [$($ty:$idx),+] [$($head:$head_idx,)* $next:$next_idx,] $($tail:$tail_idx),+ -> $ffi_ty, $carrier);
    };

    (@wide_impl [] $last:ident -> $ffi_ty:ident, $carrier:ident) => {};
    (@wide_impl [$($head:ident,)+] $last:ident -> $ffi_ty:ident, $carrier:ident) => {
        unsafe impl<$($head: CType,)+ $last: CType + crate::wide::Wide<Metadata = usize> + RustSpec<Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>> + ?Sized>
            crate::wide::Wide for $ffi_ty<$($head,)+ $last>
        where
            <$last as crate::wide::Wide>::Header: CType + Sized,
        {
            type Header = $ffi_ty<$($head,)+ <$last as crate::wide::Wide>::Header>;
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

impl_tuple! {(A:0) -> Tuple1, Tuple1}
impl_tuple! {(A:0, B:1) -> Tuple2, Tuple2}
impl_tuple! {(A:0, B:1, C:2) -> Tuple3, Tuple3}
impl_tuple! {(A:0, B:1, C:2, D:3) -> Tuple4, Tuple4}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4) -> Tuple5, Tuple5}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5) -> Tuple6, Tuple6}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6) -> Tuple7, Tuple7}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7) -> Tuple8, Tuple8}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8) -> Tuple9, Tuple9}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9) -> Tuple10, Tuple10}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10) -> Tuple11, Tuple11}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11) -> Tuple12, Tuple12}

impl<A: Niche> Niche for (A,) {
    const NICHE: Self::CType = Native(abi::Tuple1(A::NICHE));

    fn is_niche(value: &Self::CType) -> bool {
        A::is_niche(&value.0.0)
    }
}

impl<A: ReprC<CType: Copy>, B: ReprC<CType: Copy>, C: ReprC<CType: Copy>> Niche for (A, B, C)
where
    (A, (B, C)): Niche<CType = Native<abi::Tuple2<<A as ReprC>::CType, <(B, C) as ReprC>::CType>>>,
{
    const NICHE: Self::CType = {
        let pair = <(A, (B, C))>::NICHE.0;
        Native(abi::Tuple3(pair.0, pair.1.0.0, pair.1.0.1))
    };

    fn is_niche(value: &Self::CType) -> bool {
        let value = &value.0;
        <(A, (B, C)) as Niche>::is_niche(&Native(abi::Tuple2(
            value.0,
            Native(abi::Tuple2(value.1, value.2)),
        )))
    }
}

macro_rules! impl_tuple_niche_recursive {
    ($(($($all:ident),+) => ($left:ty, $right:ty) : $ffi_ty:ident($(( $outer:tt, $inner:tt )),+): ($left_val:expr, $right_val:expr)),+ $(,)?) => {
        $(
            impl<$($all: ReprC<CType: Copy>),+> Niche for ($($all,)+)
            where
                ($left, $right): Niche<CType = Native<abi::Tuple2<<$left as ReprC>::CType, <$right as ReprC>::CType>>>,
            {
                const NICHE: Self::CType = Native(abi::$ffi_ty(
                    $(<($left, $right)>::NICHE.0.$outer .0.$inner),+
                ));

                fn is_niche(value: &Self::CType) -> bool {
                    <($left, $right) as Niche>::is_niche(&Native(abi::Tuple2(Native(($left_val)(value)), Native(($right_val)(value)))))
                }
            }
        )+
    };
}

impl_tuple_niche_recursive! {
    (A, B, C, D) => ((A, B), (C, D)) : Tuple4((0, 0), (0, 1), (1, 0), (1, 1)): (|value: &Self::CType| { let value = &value.0; abi::Tuple2(value.0, value.1) }, |value: &Self::CType| { let value = &value.0; abi::Tuple2(value.2, value.3) }),
    (A, B, C, D, E) => ((A, B), (C, D, E)) : Tuple5((0, 0), (0, 1), (1, 0), (1, 1), (1, 2)): (|value: &Self::CType| { let value = &value.0; abi::Tuple2(value.0, value.1) }, |value: &Self::CType| { let value = &value.0; abi::Tuple3(value.2, value.3, value.4) }),
    (A, B, C, D, E, F) => ((A, B, C), (D, E, F)) : Tuple6((0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2)): (|value: &Self::CType| { let value = &value.0; abi::Tuple3(value.0, value.1, value.2) }, |value: &Self::CType| { let value = &value.0; abi::Tuple3(value.3, value.4, value.5) }),
    (A, B, C, D, E, F, G) => ((A, B, C), (D, E, F, G)) : Tuple7((0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2), (1, 3)): (|value: &Self::CType| { let value = &value.0; abi::Tuple3(value.0, value.1, value.2) }, |value: &Self::CType| { let value = &value.0; abi::Tuple4(value.3, value.4, value.5, value.6) }),
    (A, B, C, D, E, F, G, H) => ((A, B, C, D), (E, F, G, H)) : Tuple8((0, 0), (0, 1), (0, 2), (0, 3), (1, 0), (1, 1), (1, 2), (1, 3)): (|value: &Self::CType| { let value = &value.0; abi::Tuple4(value.0, value.1, value.2, value.3) }, |value: &Self::CType| { let value = &value.0; abi::Tuple4(value.4, value.5, value.6, value.7) }),
    (A, B, C, D, E, F, G, H, I) => ((A, B, C, D), (E, F, G, H, I)) : Tuple9((0, 0), (0, 1), (0, 2), (0, 3), (1, 0), (1, 1), (1, 2), (1, 3), (1, 4)): (|value: &Self::CType| { let value = &value.0; abi::Tuple4(value.0, value.1, value.2, value.3) }, |value: &Self::CType| { let value = &value.0; abi::Tuple5(value.4, value.5, value.6, value.7, value.8) }),
    (A, B, C, D, E, F, G, H, I, J) => ((A, B, C, D, E), (F, G, H, I, J)) : Tuple10((0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (1, 0), (1, 1), (1, 2), (1, 3), (1, 4)): (|value: &Self::CType| { let value = &value.0; abi::Tuple5(value.0, value.1, value.2, value.3, value.4) }, |value: &Self::CType| { let value = &value.0; abi::Tuple5(value.5, value.6, value.7, value.8, value.9) }),
    (A, B, C, D, E, F, G, H, I, J, K) => ((A, B, C, D, E), (F, G, H, I, J, K)) : Tuple11((0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (1, 0), (1, 1), (1, 2), (1, 3), (1, 4), (1, 5)): (|value: &Self::CType| { let value = &value.0; abi::Tuple5(value.0, value.1, value.2, value.3, value.4) }, |value: &Self::CType| { let value = &value.0; abi::Tuple6(value.5, value.6, value.7, value.8, value.9, value.10) }),
    (A, B, C, D, E, F, G, H, I, J, K, L) => ((A, B, C, D, E, F), (G, H, I, J, K, L)) : Tuple12((0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (0, 5), (1, 0), (1, 1), (1, 2), (1, 3), (1, 4), (1, 5)): (|value: &Self::CType| { let value = &value.0; abi::Tuple6(value.0, value.1, value.2, value.3, value.4, value.5) }, |value: &Self::CType| { let value = &value.0; abi::Tuple6(value.6, value.7, value.8, value.9, value.10, value.11) })
}

#[doc(hidden)]
pub mod abi {
    use super::*;
    macro_rules! impl_abi_tuple {
        ([$($ty:ident),+] [$($head:ident:$head_idx:tt,)*] $last:ident:$last_idx:tt -> $ffi_ty:ident) => {
        /// Tuple representation with lowered fields.
        #[doc(hidden)]
        #[repr(C)]
        #[derive(RustSpec)]
        pub struct $ffi_ty<$($head: CType,)* $last: CType + ?Sized>($(pub $head,)* pub $last);

        impl<$($head: CType,)* $last: CType + ?Sized> ReprC for $ffi_ty<$($head,)* $last> {
            type CType = Self;
        }
        unsafe impl<$($head: CType,)* $last: CType + ?Sized> CType for $ffi_ty<$($head,)* $last> {}
        unsafe impl<$($head: CType,)* $last: CType + ?Sized> AbiCompatibleValidate for $ffi_ty<$($head,)* $last> {
            unsafe fn is_valid(_: &Self) -> bool { true }
        }
        unsafe impl<$($head: CType,)* $last: CType + ?Sized> AbiCompatible for $ffi_ty<$($head,)* $last> {}

        impl<$($head: CType,)* $last: CType> $ffi_ty<$($head,)* $last> {
            /// Return the lowered fields without decoding them.
            pub fn into_tuple(self) -> ($($head,)* $last,) {
                ($(self.$head_idx,)* self.$last_idx,)
            }
        }
        impl<$($head: CType + Copy,)* $last: CType + Copy> Copy for $ffi_ty<$($head,)* $last> {}
        impl<$($head: CType + Clone,)* $last: CType + Clone> Clone for $ffi_ty<$($head,)* $last> {
            fn clone(&self) -> Self { Self($(self.$head_idx.clone(),)* self.$last_idx.clone()) }
        }
        impl<$($head: CType + core::fmt::Debug + Sized,)* $last: CType + core::fmt::Debug + Sized> core::fmt::Debug for $ffi_ty<$($head,)* $last> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_tuple(stringify!($ffi_ty))$(.field(&self.$head_idx))*.field(&self.$last_idx).finish()
            }
        }
        impl<$($head: CType + PartialEq + Sized,)* $last: CType + PartialEq + Sized> PartialEq for $ffi_ty<$($head,)* $last> {
            fn eq(&self, other: &Self) -> bool { true $(&& self.$head_idx == other.$head_idx)* && self.$last_idx == other.$last_idx }
        }
        impl<$($head: CType + Eq + Sized,)* $last: CType + Eq + Sized> Eq for $ffi_ty<$($head,)* $last> {}
        impl<$($head: CType + Default,)* $last: CType + Default> Default for $ffi_ty<$($head,)* $last> {
            fn default() -> Self { Self($(<$head>::default(),)* <$last>::default()) }
        }
        impl<$($head: CType + PartialOrd + Sized,)* $last: CType + PartialOrd + Sized> PartialOrd for $ffi_ty<$($head,)* $last> {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                ($( &self.$head_idx,)* &self.$last_idx,).partial_cmp(&($( &other.$head_idx,)* &other.$last_idx,))
            }
        }
        impl<$($head: CType + Ord + Sized,)* $last: CType + Ord + Sized> Ord for $ffi_ty<$($head,)* $last> {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                ($( &self.$head_idx,)* &self.$last_idx,).cmp(&($( &other.$head_idx,)* &other.$last_idx,))
            }
        }
        impl<$($head: CType + core::hash::Hash + Sized,)* $last: CType + core::hash::Hash + Sized> core::hash::Hash for $ffi_ty<$($head,)* $last> {
            fn hash<__Hasher: core::hash::Hasher>(&self, state: &mut __Hasher) { $(core::hash::Hash::hash(&self.$head_idx, state);)* core::hash::Hash::hash(&self.$last_idx, state); }
        }


        unsafe impl<$($ty: CType),*> EncodeMove for $ffi_ty<$($ty),*> {
            type Store = ();
            fn soft_encode<'a>(self, _: &'a mut ()) -> Self where Self: 'a { self }
        }
        unsafe impl<$($ty: CType),*> EncodeMoveUnchecked for $ffi_ty<$($ty),*> {
            type SoftStore = ();
            unsafe fn soft_encode_unchecked<'a>(self, _: &'a mut ()) -> Self where Self: 'a { self }
        }
        unsafe impl<'d, $($ty: CType),*> DecodeMove<'d> for $ffi_ty<$($ty),*> {
            type Store = ();
            unsafe fn soft_decode<'a: 'd>(source: Self, _: &'a mut ()) -> Option<Self> { Some(source) }
        }
        impl<$($ty: CType),*> Encode for $ffi_ty<$($ty),*> {}
        impl<$($ty: CType),*> crate::convert::EncodeUnchecked for $ffi_ty<$($ty),*> {}
        impl<'d, $($ty: CType),*> Decode<'d> for $ffi_ty<$($ty),*> {}
        crate::borrow::__impl_borrowed_identity!([$($ty: CType,)* ] $ffi_ty<$($ty),*>);
        unsafe impl<$($ty: CType),*> CFnArg for $ffi_ty<$($ty),*>
        where Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>> {}
        unsafe impl<$($ty: CType),*> CFnReturn for $ffi_ty<$($ty),*>
        where Self: crate::abi::CFnReturnLayout {}
impl<$($head: CType,)* __Co3WideHeader: CType> crate::wide::WideHeader for $ffi_ty<$($head,)* __Co3WideHeader> {
    type Data = Self;
}
impl_abi_tuple!(@wide [$($head,)*] $last -> $ffi_ty);
};
(@wide [] $last:ident -> $ffi_ty:ident) => {};
(@wide [$($head:ident,)+] $last:ident -> $ffi_ty:ident) => {
        unsafe impl<$($head: CType,)+ $last: CType + crate::wide::Wide<Metadata = usize> + RustSpec<Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>> + ?Sized>
            crate::wide::Wide for $ffi_ty<$($head,)+ $last>
        where
            <$last as crate::wide::Wide>::Header: CType + Sized,
        {
            type Header = $ffi_ty<$($head,)+ <$last as crate::wide::Wide>::Header>;
            type Metadata = usize;

            fn metadata(ptr: *const Self) -> usize {
                <super::$ffi_ty<$($head,)+ $last> as crate::wide::Wide>::metadata(ptr as *const _)
            }
            fn as_ptr(ptr: *const Self) -> *const Self::Header { ptr as *const Self::Header }
            fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Header { ptr as *mut Self::Header }
            unsafe fn from_raw_parts<'a>(data: *const Self::Header, len: usize) -> &'a Self {
                let fields = unsafe { <super::$ffi_ty<$($head,)+ $last> as crate::wide::Wide>::from_raw_parts(data.cast(), len) };
                unsafe { &*(core::ptr::from_ref(fields) as *const Self) }
            }
            unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Header, len: usize) -> &'a mut Self {
                let fields = unsafe { <super::$ffi_ty<$($head,)+ $last> as crate::wide::Wide>::from_raw_parts_mut(data.cast(), len) };
                unsafe { &mut *(core::ptr::from_mut(fields) as *mut Self) }
            }
            #[cfg(feature = "alloc")]
            fn into_non_null(self: alloc::boxed::Box<Self>) -> core::ptr::NonNull<Self::Header> {
                alloc::boxed::Box::into_non_null(self).cast()
            }
            #[cfg(feature = "alloc")]
            unsafe fn from_non_null(data: core::ptr::NonNull<Self::Header>, len: usize) -> alloc::boxed::Box<Self> {
                let fields = unsafe { <super::$ffi_ty<$($head,)+ $last> as crate::wide::Wide>::from_non_null(data.cast(), len) };
                unsafe { alloc::boxed::Box::from_raw(alloc::boxed::Box::into_raw(fields) as *mut Self) }
            }
        }

};
}
    impl_abi_tuple!([A] [] A:0 -> Tuple1);
    impl_abi_tuple!([A, B] [A:0,] B:1 -> Tuple2);
    impl_abi_tuple!([A, B, C] [A:0, B:1,] C:2 -> Tuple3);
    impl_abi_tuple!([A, B, C, D] [A:0, B:1, C:2,] D:3 -> Tuple4);
    impl_abi_tuple!([A, B, C, D, E] [A:0, B:1, C:2, D:3,] E:4 -> Tuple5);
    impl_abi_tuple!([A, B, C, D, E, F] [A:0, B:1, C:2, D:3, E:4,] F:5 -> Tuple6);
    impl_abi_tuple!([A, B, C, D, E, F, G] [A:0, B:1, C:2, D:3, E:4, F:5,] G:6 -> Tuple7);
    impl_abi_tuple!([A, B, C, D, E, F, G, H] [A:0, B:1, C:2, D:3, E:4, F:5, G:6,] H:7 -> Tuple8);
    impl_abi_tuple!([A, B, C, D, E, F, G, H, I] [A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7,] I:8 -> Tuple9);
    impl_abi_tuple!([A, B, C, D, E, F, G, H, I, J] [A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8,] J:9 -> Tuple10);
    impl_abi_tuple!([A, B, C, D, E, F, G, H, I, J, K] [A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9,] K:10 -> Tuple11);
    impl_abi_tuple!([A, B, C, D, E, F, G, H, I, J, K, L] [A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10,] L:11 -> Tuple12);
}

#[cfg(test)]
mod tests {
    use crate::marker::Native;

    #[test]
    fn tuple_families_lower_and_decode() {
        use crate::{decode, encode, tuple::Tuple2};
        let stable: Tuple2<_, _> = Tuple2(23_u32, true);
        let encoded: super::abi::Tuple2<u32, crate::primitive::abi::bool> = encode(stable);
        assert_eq!(
            unsafe { decode::<Tuple2<u32, bool>>(encoded) },
            Some(stable)
        );

        let encoded: Native<super::abi::Tuple2<u32, <bool as crate::ReprC>::CType>> =
            encode((23_u32, true));
        assert_eq!(unsafe { decode::<(u32, bool)>(encoded) }, Some((23, true)));
        assert_eq!(encode(encoded), encoded);
        static_assertions::assert_type_ne_all!(super::abi::Tuple2<u32, <bool as crate::ReprC>::CType>, Tuple2<u32, bool>);
    }

    #[test]
    fn native_carrier_traits_follow_lowered_fields() {
        use static_assertions::{assert_impl_all, assert_not_impl_any, assert_type_eq_all};
        assert_impl_all!(abi::Tuple2<<bool as crate::ReprC>::CType, <core::num::NonZeroU8 as crate::ReprC>::CType>: Copy, CType, Encode, Decode<'static>);
        assert_not_impl_any!(abi::Tuple1<<core::cell::Cell<u8> as crate::ReprC>::CType>: Copy);
        assert_type_eq_all!(<abi::Tuple2<<bool as crate::ReprC>::CType, u8> as ReprC>::CType, abi::Tuple2<<bool as crate::ReprC>::CType, u8>);
        assert_type_eq_all!(
            <(bool, u8) as ReprC>::CType,
            Native<abi::Tuple2<<bool as crate::ReprC>::CType, u8>>
        );
        assert_type_eq_all!(
            <Tuple2<bool, u8> as ReprC>::CType,
            abi::Tuple2<crate::primitive::abi::bool, u8>
        );
        static_assertions::assert_type_ne_all!(Native<abi::Tuple2<u8, u32>>, abi::Tuple2<u8, u32>);
        assert_eq!(
            core::mem::size_of::<abi::Tuple2<u8, u32>>(),
            core::mem::size_of::<abi::Tuple2<u8, u32>>()
        );
        assert_eq!(
            core::mem::align_of::<abi::Tuple2<u8, u32>>(),
            core::mem::align_of::<abi::Tuple2<u8, u32>>()
        );

        struct Payload<'a>(&'a core::cell::Cell<usize>);
        unsafe impl CType for Payload<'_> {}
        impl Drop for Payload<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let drops = core::cell::Cell::new(0);
        let native = abi::Tuple1::<_>(Payload(&drops));
        let stable: abi::Tuple1<Payload<'_>> = native;
        assert_eq!(drops.get(), 0);
        drop(stable);
        assert_eq!(drops.get(), 1);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn native_wide_carrier_preserves_allocation_and_metadata() {
        use crate::wide::Wide;
        type TailHeader = <[u64] as Wide>::Header;
        static_assertions::assert_type_eq_all!(<abi::Tuple2<u8, [u64]> as Wide>::Header, abi::Tuple2<u8, TailHeader>);
        static_assertions::assert_type_eq_all!(
            <abi::Tuple2<u8, [u64]> as Wide>::Header,
            abi::Tuple2<u8, TailHeader>
        );
        let fields: alloc::boxed::Box<Tuple2<u8, [u64]>> =
            alloc::boxed::Box::new(Tuple2(7, [11, 22, 33]));
        let pointer = core::ptr::from_ref(&*fields).cast::<()>();
        let data = Wide::into_non_null(fields);
        // Both repr(C) families contain the same lowered fields in the same order.
        let carrier = unsafe { <abi::Tuple2<u8, [u64]> as Wide>::from_non_null(data.cast(), 3) };
        assert_eq!(core::ptr::from_ref(&*carrier).cast::<()>(), pointer);
        assert_eq!(<abi::Tuple2<u8, [u64]> as Wide>::metadata(&*carrier), 3);
        assert_eq!(carrier.0, 7);
        assert_eq!(&carrier.1, &[11, 22, 33]);
        let data = Wide::into_non_null(carrier);
        let fields = unsafe { <Tuple2<u8, [u64]> as Wide>::from_non_null(data.cast(), 3) };
        assert_eq!(core::ptr::from_ref(&*fields).cast::<()>(), pointer);
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
    #[cfg(feature = "alloc")]
    use crate::{boxed::abi::Box as AbiBox, wide::abi::WideBox as AbiWideBox};
    use crate::{
        convert::{Decode, Encode},
        option::abi::Option as AbiOption,
        reference::abi::{SoftRef, SoftRefMut},
    };
    #[cfg(feature = "alloc")]
    use crate::{
        convert::{DecodeMove, EncodeMove},
        wide::abi::{SoftWideRef as AbiSoftWideRef, SoftWideRefMut as AbiSoftWideRefMut},
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
            ReprC<CType = Native<abi::Tuple3<u8, u8, u8>>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(&(u8, u8, u8):
            Niche<CType = SoftRef<<(u8, u8, u8) as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut (u8, u8, u8):
        Niche<CType = SoftRefMut<<(u8, u8, u8) as crate::ReprC>::CType>>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<(u8, u8, u8)>:
            Niche<CType = AbiBox<<(u8, u8, u8) as crate::ReprC>::CType>>,
            EncodeMove,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&[(u8, u8, u8)]:
            Niche<CType = AbiSoftWideRef<<[(u8, u8, u8)] as crate::ReprC>::CType>>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [(u8, u8, u8)]:
            Niche<CType = AbiSoftWideRefMut<<[(u8, u8, u8)] as crate::ReprC>::CType>>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[(u8, u8, u8)]>:
            Niche<CType = AbiWideBox<<[(u8, u8, u8)] as crate::ReprC>::CType>>,
            EncodeMove,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, u8, u8)>:
            Niche<CType = Native<crate::vec::abi::Vec<<(u8, u8, u8) as crate::ReprC>::CType>>>,
            DecodeMove<'static>,
            EncodeMove,
        );
        assert_impl_all!([(u8, u8, u8); 2]:
            ReprC<CType = [Native<abi::Tuple3<u8, u8, u8>>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<(u8, u8, u8)>:
            Niche<CType = Native<AbiOption<<(u8, u8, u8) as crate::ReprC>::CType>>>,
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
        let source = crate::convert::encode_move::<Value>(Box::new((1, 2, 3)));
        let decoded = unsafe { crate::convert::decode_move::<Value>(source) };
        assert_eq!(decoded, Some(Box::new((1, 2, 3))));
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn unstable_slice_box_owned_round_trip() {
        type Value = Box<[(u8, u8, u8)]>;
        let input: Value = Vec::from([(1, 2, 3), (4, 5, 6)]).into_boxed_slice();
        let source = crate::convert::encode_move(input);
        let decoded = unsafe { crate::convert::decode_move::<Value>(source) };
        assert_eq!(
            decoded,
            Some(Vec::from([(1, 2, 3), (4, 5, 6)]).into_boxed_slice())
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn unstable_boxed_slice_uses_box_allocation_layout() {
        let value = Vec::from([(1u32,), (2,)]).into_boxed_slice();
        let carrier = crate::convert::encode_move(value);
        let decoded = unsafe { crate::convert::decode_move::<Box<[(u32,)]>>(carrier) }.unwrap();
        assert_eq!(&*decoded, &[(1,), (2,)]);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn unstable_interior_box_checked_round_trip() {
        use core::cell::Cell;
        let encoded = crate::convert::encode_move(Box::new(Cell::new((7_u8,))));
        let decoded = unsafe { crate::convert::decode_move::<Box<Cell<(u8,)>>>(encoded) }.unwrap();
        assert_eq!(decoded.get(), (7,));
    }

    #[test]
    fn stored_tuple_3_with_niche() {
        // NOTE: Confirms niche is taken from the first available element
        let niche = <(u8, StdNonZero<u8>, bool)>::NICHE;
        assert_eq!(
            (niche.0.0, niche.0.1.0, niche.0.2),
            (0, 0, crate::primitive::abi::bool::FALSE)
        );

        assert_impl_all!((u8, StdNonZero<u8>, bool):
            Niche<CType = Native<abi::Tuple3<u8, <core::num::NonZeroU8 as crate::ReprC>::CType, <bool as crate::ReprC>::CType>>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(&(u8, StdNonZero<u8>, bool):
            Niche<CType = SoftRef<<(u8, StdNonZero<u8>, bool) as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut (u8, StdNonZero<u8>, bool):
            Niche<CType = SoftRefMut<<(u8, StdNonZero<u8>, bool) as crate::ReprC>::CType>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<(u8, StdNonZero<u8>, bool)>:
            Niche<CType = AbiBox<<(u8, StdNonZero<u8>, bool) as crate::ReprC>::CType>>,
            DecodeMove<'static>,
            EncodeMove,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&[(u8, StdNonZero<u8>, bool)]:
            Niche<CType = AbiSoftWideRef<<[(u8, StdNonZero<u8>, bool)] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [(u8, StdNonZero<u8>, bool)]:
            Niche<CType = AbiSoftWideRefMut<<[(u8, StdNonZero<u8>, bool)] as crate::ReprC>::CType>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[(u8, StdNonZero<u8>, bool)]>:
            Niche<CType = AbiWideBox<<[(u8, StdNonZero<u8>, bool)] as crate::ReprC>::CType>>,
            DecodeMove<'static>,
            EncodeMove,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, StdNonZero<u8>, bool)>:
            Niche<CType = Native<crate::vec::abi::Vec<<(u8, core::num::NonZeroU8, bool) as crate::ReprC>::CType>>>,
            DecodeMove<'static>,
            EncodeMove,
        );
        assert_impl_all!([(u8, StdNonZero<u8>, bool); 2]:
            Niche<CType = [Native<abi::Tuple3<u8, <core::num::NonZeroU8 as crate::ReprC>::CType, <bool as crate::ReprC>::CType>>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<(u8, StdNonZero<u8>, bool)>:
            // TODO: Depends on: https://github.com/mversic/co3/issues/33
            //Niche<CType = Native<abi::Tuple3<u8, u8, u8>>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<[(u8, StdNonZero<u8>, u8)]>: Encode, Decode<'static>);
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, StdNonZero<u8>, u8)>: Encode, Decode<'static>);
    }
}
