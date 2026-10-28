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
//! `COption<ReprCTupleN<A, B, ...>>`.
//!
//! # Example
//!
//! ```rust
//! use core::mem::size_of;
//!
//! use co3::{
//!     encode, niche::Niche, option::COption, primitives::CBool,
//!     reference::CRef, tuple::ReprCTuple3, ReprC,
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
//! assert_eq!(encode(none_value_1), ReprCTuple3(0, bool::NICHE, CRef::null()));
//!
//! let none_value_2: Option<TupleWithNiche2> = None;
//! assert_eq!(encode(none_value_2), ReprCTuple3(0, CRef::null(), CBool::FALSE));
//!
//! let none_value_3: Option<TupleWithoutNiche> = None;
//! assert_eq!(encode(none_value_3), COption::None());
//! ```

use disjoint_impls::disjoint_impls;
use rust_spec::{
    RustSpec,
    niche::{NicheStabilityKind, WithNiche, WithoutNiche},
};

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC, Store,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    niche::Niche,
    stored::{DecodeOwned, EncodeMove, EncodeMoveUnchecked, EncodeRef, EncodeRefUnchecked},
    transmute::AbiCompatible,
};

macro_rules! impl_tuple {
    ( ($( $ty:ident : $idx:tt ),+) -> $ffi_ty:ident ) => {
        impl_tuple!(@main [$($ty:$idx),+] [] $($ty:$idx),+ -> $ffi_ty);
    };

    (@main [$($ty:ident:$idx:tt),+] [$($head:ident:$head_idx:tt,)*] $last:ident:$last_idx:tt -> $ffi_ty:ident) => {
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

        unsafe impl<$($ty: EncodeMove),*> EncodeMove for ($($ty,)*) {
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
        unsafe impl<$($ty: EncodeMove),*> EncodeMove for $ffi_ty<$($ty),*> {
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

        unsafe impl<$($ty: EncodeMoveUnchecked),*> EncodeMoveUnchecked for ($($ty,)*) {
            type SoftStore = ($($ty::SoftStore,)*);

            #[expect(non_snake_case)]
            unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
                impl_tuple! {@decl_priv_store $($ty),*}

                let ($($ty,)*) = self;
                let store: private_store::Store<$($ty::SoftStore,)*> = store.into();
                $ffi_ty($(unsafe { $ty.soft_encode_unchecked(store.$ty) }),*)
            }
        }

        unsafe impl<$($ty: EncodeMoveUnchecked),*> EncodeMoveUnchecked for $ffi_ty<$($ty),*> {
            type SoftStore = ($($ty::SoftStore,)*);

            #[expect(non_snake_case)]
            unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
                impl_tuple! {@decl_priv_store $($ty),*}

                let $ffi_ty($($ty),*) = self;
                let store: private_store::Store<$($ty::SoftStore,)*> = store.into();
                $ffi_ty($(unsafe { $ty.soft_encode_unchecked(store.$ty) }),*)
            }
        }

        #[allow(non_snake_case)]
        const _: () = {
        disjoint_impls! {
        /// # Safety
        /// Implementations must uphold the contract of [`crate::stored::EncodeRef`].
        #[disjoint_impls(remote)]
        pub unsafe trait EncodeRef<'a>: EncodeRefUnchecked<'a> {
            type Store: Store + Default;
            fn encode_ref(&'a self, store: &mut Self::Store) -> <&'a Self as ReprC>::CType
            where &'a Self: ReprC<CType: Sized>;
        }

        unsafe impl<'a, $($ty: ReprC<CType: Copy> + 'a),*> EncodeRef<'a> for ($($ty,)*)
        where
            Self: RustSpec<Mutability = rust_spec::mutability::Exclusive, Size: crate::Thin>,
            $(&'a $ty: EncodeMove + ReprC<CType = crate::reference::CRef<<$ty as ReprC>::CType>>),*
        {
            type Store = crate::sync::RefSizedEncodeStore<Self, ($(<&'a $ty as EncodeMove>::Store,)*)>;
            fn encode_ref(&'a self, store: &mut Self::Store) -> <&'a Self as ReprC>::CType {
                store.encode(|fields| {
                    impl_tuple! {@decl_priv_store $($ty),*}
                    let ($($ty,)*) = self;
                    let fields: private_store::Store<$(<&'a $ty as EncodeMove>::Store,)*> = fields.into();
                    $ffi_ty($($crate::stored::encode_ref_field($ty, fields.$ty)),*)
                })
            }
        }

        unsafe impl<'a, $($ty: ReprC<CType: Copy> + 'a),*> EncodeRef<'a> for $ffi_ty<$($ty),*>
        where
            Self: RustSpec<Mutability = rust_spec::mutability::Exclusive, Size: crate::Thin>,
            $(&'a $ty: EncodeMove + ReprC<CType = crate::reference::CRef<<$ty as ReprC>::CType>>),*
        {
            type Store = crate::sync::RefSizedEncodeStore<Self, ($(<&'a $ty as EncodeMove>::Store,)*)>;
            fn encode_ref(&'a self, store: &mut Self::Store) -> <&'a Self as ReprC>::CType {
                store.encode(|fields| {
                    impl_tuple! {@decl_priv_store $($ty),*}
                    let $ffi_ty($($ty),*) = self;
                    let fields: private_store::Store<$(<&'a $ty as EncodeMove>::Store,)*> = fields.into();
                    $ffi_ty($($crate::stored::encode_ref_field($ty, fields.$ty)),*)
                })
            }
        }
        }};

        #[allow(non_snake_case)]
        const _: () = {
        disjoint_impls! {
        /// # Safety
        /// Implementations must uphold the contract of [`crate::stored::EncodeRefUnchecked`].
        #[disjoint_impls(remote)]
        pub unsafe trait EncodeRefUnchecked<'a>: ReprC + 'a {
            type SoftStore: Store + Default;
            unsafe fn encode_ref_unchecked(&'a self, store: &mut Self::SoftStore) -> <&'a Self as ReprC>::CType
            where &'a Self: ReprC<CType: Sized>;
        }

        unsafe impl<'a, $($ty: ReprC<CType: Copy> + 'a),*> EncodeRefUnchecked<'a> for ($($ty,)*)
        where
            Self: RustSpec<Mutability = rust_spec::mutability::Exclusive, Size: crate::Thin>,
            $(&'a $ty: EncodeMoveUnchecked + ReprC<CType = crate::reference::CRef<<$ty as ReprC>::CType>>),*
        {
            type SoftStore = crate::sync::RefSizedEncodeStore<Self, ($(<&'a $ty as EncodeMoveUnchecked>::SoftStore,)*)>;
            unsafe fn encode_ref_unchecked(&'a self, store: &mut Self::SoftStore) -> <&'a Self as ReprC>::CType {
                store.encode(|fields| {
                    impl_tuple! {@decl_priv_store $($ty),*}
                    let ($($ty,)*) = self;
                    let fields: private_store::Store<$(<&'a $ty as EncodeMoveUnchecked>::SoftStore,)*> = fields.into();
                    $ffi_ty($(unsafe { $crate::stored::encode_ref_field_unchecked($ty, fields.$ty) }),*)
                })
            }
        }

        unsafe impl<'a, $($ty: ReprC<CType: Copy> + 'a),*> EncodeRefUnchecked<'a> for $ffi_ty<$($ty),*>
        where
            Self: RustSpec<Mutability = rust_spec::mutability::Exclusive, Size: crate::Thin>,
            $(&'a $ty: EncodeMoveUnchecked + ReprC<CType = crate::reference::CRef<<$ty as ReprC>::CType>>),*
        {
            type SoftStore = crate::sync::RefSizedEncodeStore<Self, ($(<&'a $ty as EncodeMoveUnchecked>::SoftStore,)*)>;
            unsafe fn encode_ref_unchecked(&'a self, store: &mut Self::SoftStore) -> <&'a Self as ReprC>::CType {
                store.encode(|fields| {
                    impl_tuple! {@decl_priv_store $($ty),*}
                    let $ffi_ty($($ty),*) = self;
                    let fields: private_store::Store<$(<&'a $ty as EncodeMoveUnchecked>::SoftStore,)*> = fields.into();
                    $ffi_ty($(unsafe { $crate::stored::encode_ref_field_unchecked($ty, fields.$ty) }),*)
                })
            }
        }
        }};

        unsafe impl<'d, $($ty: DecodeOwned<'d>),*> DecodeOwned<'d> for ($($ty,)*) {
            type Store = ($( $ty::Store, )*);

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                impl_tuple! {@decl_priv_store $($ty),*}

                let $ffi_ty($($ty),*) = source;
                let store: private_store::Store<$($ty::Store),*> = store.into();
                Some(unsafe { ($( $ty::soft_decode($ty, store.$ty)?, )*) })
            }

            #[inline(always)]
            #[expect(non_snake_case)]
            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
                impl_tuple! {@decl_priv_store $($ty),*}

                let $ffi_ty($($ty),*) = source;
                let store: private_store::Store<$($ty::Store),*> = store.into();
                unsafe { ($( $ty::soft_decode_unchecked($ty, store.$ty), )*) }
            }
        }
        unsafe impl<'d, $($ty: DecodeOwned<'d>),*> DecodeOwned<'d> for $ffi_ty<$($ty),*> {
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

        impl<$($ty: Encode),*> Encode for ($($ty,)*) {}
        impl<$($ty: Encode),*> Encode for $ffi_ty<$($ty),*> {}
        impl<$($ty: crate::EncodeUnchecked),*> crate::EncodeUnchecked for ($($ty,)*) {}
        impl<$($ty: crate::EncodeUnchecked),*> crate::EncodeUnchecked for $ffi_ty<$($ty),*> {}

        impl<'d, $($ty: Decode<'d>),*> Decode<'d> for ($($ty,)*) {}
        impl<'d, $($ty: Decode<'d>),*> Decode<'d> for $ffi_ty<$($ty),*> {}

        unsafe impl<$($ty: CType),*> CFnArg for $ffi_ty<$($ty),*>
        where
            Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        {}
        unsafe impl<$($ty: CType),*> CFnReturn for $ffi_ty<$($ty),*>
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
        pub struct $ffi_ty<$($head,)* $last: ?Sized>($(pub $head,)* pub $last);

        impl<$($head: CType,)* __Co3WideHeader: CType> crate::wide::WideHeader for $ffi_ty<$($head,)* __Co3WideHeader> {
            type Data = Self;
        }

        impl_tuple!(@wide_impl [$($head,)*] $last -> $ffi_ty);

        impl<$($head,)* $last> $ffi_ty<$($head,)* $last> {
            /// Return the stored fields as a Rust tuple without decoding them.
            #[expect(non_snake_case)]
            pub fn into_tuple(self) -> ($($head,)* $last,) {
                let $ffi_ty($($head,)* $last) = self;
                ($($head,)* $last,)
            }
        }

        impl<$($head: ReprC<CType: Sized>,)* $last: ReprC + ?Sized> ReprC for ($($head,)* $last,) {
            type CType = $ffi_ty<$($head::CType,)* $last::CType>;
        }
        impl<$($head: ReprC<CType: Sized>,)* $last: ReprC + ?Sized> ReprC for $ffi_ty<$($head,)* $last> {
            type CType = $ffi_ty<$($head::CType,)* $last::CType>;
        }

        unsafe impl<$($head: AbiCompatible<CType: Copy>,)* $last: AbiCompatible + ?Sized> AbiCompatible for $ffi_ty<$($head,)* $last> {
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

        unsafe impl<$($head: CType,)* $last: CType + ?Sized> CType for $ffi_ty<$($head,)* $last> {}

        unsafe impl<$($head: crate::stored::EmptyStore,)* $last: crate::stored::EmptyStore> crate::stored::EmptyStore for ($($head,)* $last,) {}
        unsafe impl<$($head: crate::stored::EmptyStore,)* $last: crate::stored::EmptyStore> crate::stored::EmptyStore for $ffi_ty<$($head,)* $last> {}
    };

    (@split [$($head:ident:$head_idx:tt,)*] $next:ident:$next_idx:tt, $($tail:ident:$tail_idx:tt),+ -> $ffi_ty:ident) => {
        impl_tuple!(@split [$($head:$head_idx,)* $next:$next_idx,] $($tail:$tail_idx),+ -> $ffi_ty);
    };

    (@main [$($ty:ident:$idx:tt),+] [$($head:ident:$head_idx:tt,)*] $next:ident:$next_idx:tt, $($tail:ident:$tail_idx:tt),+ -> $ffi_ty:ident) => {
        impl_tuple!(@main [$($ty:$idx),+] [$($head:$head_idx,)* $next:$next_idx,] $($tail:$tail_idx),+ -> $ffi_ty);
    };

    (@wide_impl [] $last:ident -> $ffi_ty:ident) => {};
    (@wide_impl [$($head:ident,)+] $last:ident -> $ffi_ty:ident) => {
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
                // A slice-like DST has a data pointer and usize metadata. The
                // repr(C) tuple has the same metadata as its unsized last field.
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
                let ptr = unsafe { Self::from_raw_parts_mut(data.as_ptr(), len) } as *mut Self;
                unsafe { alloc::boxed::Box::from_raw(ptr) }
            }
        }
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

impl_tuple! {(A:0) -> ReprCTuple1}
impl_tuple! {(A:0, B:1) -> ReprCTuple2}
impl_tuple! {(A:0, B:1, C:2) -> ReprCTuple3}
impl_tuple! {(A:0, B:1, C:2, D:3) -> ReprCTuple4}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4) -> ReprCTuple5}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5) -> ReprCTuple6}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6) -> ReprCTuple7}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7) -> ReprCTuple8}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8) -> ReprCTuple9}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9) -> ReprCTuple10}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10) -> ReprCTuple11}
impl_tuple! {(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11) -> ReprCTuple12}

impl<A: Niche> Niche for (A,) {
    const NICHE: Self::CType = ReprCTuple1(A::NICHE);
}

disjoint_impls! {
    #[disjoint_impls(remote)]
    trait Niche: ReprC<CType: Copy> + Sized {
        const NICHE: Self::CType;
    }

    impl<A: Niche<CType: Copy>, B: ReprC<CType: Copy>, N: NicheStabilityKind> Niche for (A, B)
    where
        A: RustSpec<Niche = WithNiche<N>>,
    {
        const NICHE: Self::CType = ReprCTuple2(A::NICHE, unsafe { core::mem::zeroed() });
    }

    impl<A: ReprC<CType: Copy>, B: Niche<CType: Copy>, N: NicheStabilityKind> Niche for (A, B)
    where
        A: RustSpec<Niche = WithoutNiche>,
        B: RustSpec<Niche = WithNiche<N>>,
    {
        const NICHE: Self::CType = ReprCTuple2(unsafe { core::mem::zeroed() }, B::NICHE);
    }
}

impl<A: ReprC<CType: Copy>, B: ReprC<CType: Copy>, C: ReprC<CType: Copy>> Niche for (A, B, C)
where
    (A, (B, C)): Niche<CType = ReprCTuple2<A::CType, <(B, C) as ReprC>::CType>>,
{
    const NICHE: Self::CType = ReprCTuple3(
        <(A, (B, C))>::NICHE.0,
        <(A, (B, C))>::NICHE.1.0,
        <(A, (B, C))>::NICHE.1.1,
    );
}

macro_rules! impl_tuple_niche_recursive {
    ($(($($all:ident),+) => ($left:ty, $right:ty) : $ffi_ty:ident($($field:tt),+)),+ $(,)?) => {
        $(
            impl<$($all: ReprC<CType: Copy>),+> Niche for ($($all,)+)
            where
                ($left, $right): Niche<CType = ReprCTuple2<<$left as ReprC>::CType, <$right as ReprC>::CType>>,
            {
                const NICHE: Self::CType = $ffi_ty(
                    $(<($left, $right)>::NICHE.$field),+
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
    #[cfg(feature = "alloc")]
    use crate::boxed::{CBox, CWideBox};
    use crate::reference::{CRef, CRefMut};
    use crate::{Decode, Encode, option::COption};
    #[cfg(feature = "alloc")]
    use crate::{
        slice::{CWideRef, CWideRefMut},
        stored::{DecodeOwned, EncodeMove},
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
            ReprC<CType = ReprCTuple3<u8, u8, u8>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(&(u8, u8, u8):
            Niche<CType = CRef<ReprCTuple3<u8, u8, u8>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut (u8, u8, u8):
        Niche<CType = CRefMut<ReprCTuple3<u8, u8, u8>, true>>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<(u8, u8, u8)>:
            Niche<CType = CBox<ReprCTuple3<u8, u8, u8>>>,
            EncodeMove,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&[(u8, u8, u8)]:
            Niche<CType = CWideRef<[ReprCTuple3<u8, u8, u8>]>>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [(u8, u8, u8)]:
            Niche<CType = CWideRefMut<[ReprCTuple3<u8, u8, u8>], true>>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[(u8, u8, u8)]>:
            Niche<CType = CWideBox<[ReprCTuple3<u8, u8, u8>]>>,
            EncodeMove,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, u8, u8)>:
            Niche<CType = CWideBox<[ReprCTuple3<u8, u8, u8>]>>,
            DecodeOwned<'static>,
            EncodeMove,
        );
        assert_impl_all!([(u8, u8, u8); 2]:
            ReprC<CType = [ReprCTuple3<u8, u8, u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<(u8, u8, u8)>:
            Niche<CType = COption<ReprCTuple3<u8, u8, u8>>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<[(u8, u8, u8)]>: Encode, Decode<'static>);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Vec<(u8, u8, u8)>: Encode, Decode<'static>);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn unstable_sized_box_decode_releases_source_allocation() {
        type Value = Box<(u8, u8, u8)>;

        let mut encode_store = Default::default();
        let source = <Value as EncodeMove>::soft_encode(Box::new((1, 2, 3)), &mut encode_store);
        let mut decode_store = Default::default();
        let decoded = unsafe { <Value as DecodeOwned>::soft_decode(source, &mut decode_store) };

        assert_eq!(decoded, Some(Box::new((1, 2, 3))));
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn unstable_slice_box_owned_round_trip() {
        type Value = Box<[(u8, u8, u8)]>;

        let input: Value = Vec::from([(1, 2, 3), (4, 5, 6)]).into_boxed_slice();
        let mut encode_store = Default::default();
        let source = <Value as EncodeMove>::soft_encode(input, &mut encode_store);
        let mut decode_store = Default::default();
        let decoded = unsafe { <Value as DecodeOwned>::soft_decode(source, &mut decode_store) };

        assert_eq!(
            decoded,
            Some(Vec::from([(1, 2, 3), (4, 5, 6)]).into_boxed_slice())
        );
    }

    #[test]
    fn stored_tuple_3_with_niche() {
        // NOTE: Confirms niche is taken from the first available element
        assert_eq!(
            <(u8, StdNonZero<u8>, bool)>::NICHE,
            ReprCTuple3(0, 0, crate::primitives::CBool::FALSE)
        );

        assert_impl_all!((u8, StdNonZero<u8>, bool):
            Niche<CType = ReprCTuple3<u8, u8, crate::primitives::CBool>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(&(u8, StdNonZero<u8>, bool):
            Niche<CType = CRef<ReprCTuple3<u8, u8, crate::primitives::CBool>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut (u8, StdNonZero<u8>, bool):
            Niche<CType = CRefMut<ReprCTuple3<u8, u8, crate::primitives::CBool>, true>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<(u8, StdNonZero<u8>, bool)>:
            Niche<CType = CBox<ReprCTuple3<u8, u8, crate::primitives::CBool>>>,
            DecodeOwned<'static>,
            EncodeMove,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&[(u8, StdNonZero<u8>, bool)]:
            Niche<CType = CWideRef<[ReprCTuple3<u8, u8, crate::primitives::CBool>]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [(u8, StdNonZero<u8>, bool)]:
            Niche<CType = CWideRefMut<[ReprCTuple3<u8, u8, crate::primitives::CBool>], true>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[(u8, StdNonZero<u8>, bool)]>:
            Niche<CType = CWideBox<[ReprCTuple3<u8, u8, crate::primitives::CBool>]>>,
            DecodeOwned<'static>,
            EncodeMove,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<(u8, StdNonZero<u8>, bool)>:
            Niche<CType = CWideBox<[ReprCTuple3<u8, u8, crate::primitives::CBool>]>>,
            DecodeOwned<'static>,
            EncodeMove,
        );
        assert_impl_all!([(u8, StdNonZero<u8>, bool); 2]:
            Niche<CType = [ReprCTuple3<u8, u8, crate::primitives::CBool>; 2]>,
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
