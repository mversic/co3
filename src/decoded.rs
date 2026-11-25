//! Referent-specific decoding and mutable writeback for unstable layouts.

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};

use disjoint_impls::disjoint_impls;
use rust_spec::RustSpec;
#[cfg(all(feature = "alloc", feature = "repr-rust"))]
use rust_spec::size::{MetaSized, SliceLike};
#[cfg(feature = "alloc")]
use rust_spec::{
    Unspecified,
    mutability::{Exclusive, ExclusiveKind},
};

#[cfg(feature = "repr-rust")]
use crate::ffi::NulTerminatedBuf;
#[cfg(all(feature = "alloc", feature = "repr-rust"))]
use crate::sync::{RefMutSliceDecodeStore, RefMutSliceDecodeStoreUnchecked};
use crate::{
    CType, ReprC,
    convert::{
        DecodeMove, DecodeMoveUnchecked, EmptyStore, EncodeMove, EncodeMoveUnchecked, Store,
    },
    lang::{self, Language},
    marker::Native,
    niche::Niche,
    option::abi::{Option as AbiOption, OptionSome as AbiOptionSome},
    reference::abi::{RefCarrier, RefMutCarrier, SoftRef, SoftRefMut},
    result::abi::{Result as AbiResult, ResultErr as AbiResultErr, ResultOk as AbiResultOk},
    sync::{
        ArrayStore, DecodedField, DecodedFieldUnchecked, RefMutSizedDecodeStore,
        RefMutSizedDecodeStoreUnchecked, RefSizedDecodeStore, TakeDecoded,
    },
};
#[cfg(feature = "alloc")]
use crate::{
    boxed::abi::Box as AbiBox,
    vec::abi::Vec as AbiVec,
    wide::{abi::WideBox as AbiWideBox, abi::WideRefCarrier},
};

// Decode the field through the ordinary reference path, then obtain its value
// using the adapter's stable-clone or unstable-extraction implementation.
pub(crate) unsafe fn decode_ref_field<'d, 'itm: 'd, T: ReprC + RustSpec + 'd, L: Language>(
    source: <&'d T as ReprC>::CType,
    store: &'itm mut <&'d T as DecodeMove<'d, L>>::Store,
) -> Option<T>
where
    &'d T: DecodeMove<'d, L> + ReprC<CType: Sized>,
    crate::sync::DecodedField<'d, T, <&'d T as DecodeMove<'d, L>>::Store, T::Layout, L>:
        TakeDecoded<Value = T>,
{
    unsafe {
        DecodedField::<T, <&'d T as DecodeMove<'d, L>>::Store, T::Layout, L>::decode(source, store)
    }
}

pub(crate) unsafe fn decode_ref_field_unchecked<'d, 'itm: 'd, T: ReprC + RustSpec + 'd>(
    source: <&'d T as ReprC>::CType,
    store: &'itm mut <&'d T as DecodeMoveUnchecked<'d>>::SoftStore,
) -> T
where
    &'d T: DecodeMoveUnchecked<'d> + ReprC<CType: Sized>,
    crate::sync::DecodedFieldUnchecked<
        'd,
        T,
        <&'d T as DecodeMoveUnchecked<'d>>::SoftStore,
        T::Layout,
    >: TakeDecoded<Value = T>,
{
    unsafe {
        DecodedFieldUnchecked::<T, <&'d T as DecodeMoveUnchecked<'d>>:: SoftStore, T::Layout>::decode_unchecked(
            source, store,
        )
    }
}

disjoint_impls! {
    /// Decodes a carrier whose Rust validity is guaranteed by the caller.
    ///
    /// # Safety
    /// Implementations must preserve ownership, lifetimes and aliasing. Shared
    /// interior-mutable contents must never be inspected. An empty conversion
    /// store must not provide backing storage for the returned value.
    pub unsafe trait SoftDecodeRefUnchecked<'d>: ReprC + 'd {
        /// Storage retained for the unchecked conversion.
        type SoftStore: Store + Default;

        /// Decode without validating the source value.
        ///
        /// # Safety
        /// The source must satisfy all Rust validity, access and ownership
        /// requirements, including those of any references in the result.
        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d Self
        where
            &'d Self: ReprC<CType: Sized>;
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: 'd, S: rust_spec::size::SizedKind> SoftDecodeRefUnchecked<'d> for Box<T>
    where
        Self: ReprC<CType = AbiBox<<T as ReprC>::CType>>,
        T: ReprC + RustSpec<Size = rust_spec::size::Sized<S>>,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'d T: DecodeMoveUnchecked<'d, SoftStore: Store + Default + Sized + TakeDecoded<Value = T>>
            + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
        <T as ReprC>::CType: Sized,
    {
        type SoftStore = RefSizedDecodeStore<Self, <&'d T as DecodeMoveUnchecked<'d>>::SoftStore>;

        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d Self {
            let header = unsafe { *source.into_ref().unwrap_unchecked() };
            let carrier = unsafe { RefCarrier::from_raw(header.as_ptr()) };
            store
                .decode(|field| {
                    let value = unsafe {
                        DecodedFieldUnchecked::<T, _, Unspecified>::decode_unchecked(carrier, field)
                    };
                    Some(Box::new(value))
                })
                .expect("unchecked decoding produces a value")
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: ?Sized + alloc::borrow::ToOwned + 'd, __Interior: ExclusiveKind>
        SoftDecodeRefUnchecked<'d> for Box<T>
    where
        Self: ReprC<CType = AbiWideBox<<T as ReprC>::CType>>,
        T: ReprC<
                CType: crate::CType
                           + crate::wide::Wide<
                    Header: crate::wide::WideHeader<Data: CType>,
                    Metadata = usize,
                >,
            > + RustSpec<
                Layout = rust_spec::Specified,
                Mutability = Exclusive<__Interior>,
                Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>,
            >,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'d T: DecodeMoveUnchecked<'d, SoftStore: Store + Default + Sized>
            + ReprC<CType: WideRefCarrier<Pointee = <T as ReprC>::CType>>,
        Self: From<<T as alloc::borrow::ToOwned>::Owned>,
    {
        type SoftStore = RefSizedDecodeStore<Self, <&'d T as DecodeMoveUnchecked<'d>>::SoftStore>;

        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d Self {
            let header = unsafe { *source.into_ref().unwrap_unchecked() };
            let carrier = unsafe {
                WideRefCarrier::from_raw_parts(header.as_ptr().cast_const(), header.metadata())
            };
            store
                .decode(|field| {
                    let value = unsafe {
                        <&'d T as DecodeMoveUnchecked<'d>>::soft_decode_unchecked(carrier, field)
                    };
                    Some(alloc::borrow::ToOwned::to_owned(value).into())
                })
                .expect("unchecked decoding produces a value")
        }
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, T: ?Sized + 'd> SoftDecodeRefUnchecked<'d> for Box<T>
    where
        Self: ReprC<CType = AbiWideBox<<T as ReprC>::CType>>,
        T: ReprC<
                CType: crate::CType
                           + crate::wide::Wide<
                    Header: crate::wide::WideHeader<Data: CType>,
                    Metadata = usize,
                >,
            > + RustSpec<
                Layout = Unspecified,
                Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>,
            >,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'d T: DecodeMoveUnchecked<'d, SoftStore: Store + Default + Sized + TakeDecoded>
            + ReprC<CType: WideRefCarrier<Pointee = <T as ReprC>::CType>>,
        Self: From<<<&'d T as DecodeMoveUnchecked<'d>>::SoftStore as TakeDecoded>::Value>,
    {
        type SoftStore = RefSizedDecodeStore<Self, <&'d T as DecodeMoveUnchecked<'d>>::SoftStore>;

        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d Self {
            let header = unsafe { *source.into_ref().unwrap_unchecked() };
            let carrier = unsafe {
                WideRefCarrier::from_raw_parts(header.as_ptr().cast_const(), header.metadata())
            };
            store
                .decode(|field| {
                    let value = unsafe {
                        DecodedFieldUnchecked::<T, _, Unspecified>::decode_unchecked(carrier, field)
                    };
                    Some(value.into())
                })
                .expect("unchecked decoding produces a value")
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, T: ReprC + RustSpec<Layout: Sized> + 'd> SoftDecodeRefUnchecked<'d> for Option<T>
    where
        crate::sync::DecodedFieldUnchecked<
            'd,
            T,
            <&'d T as DecodeMoveUnchecked<'d>>::SoftStore,
            <T as RustSpec>::Layout,
        >: TakeDecoded<Value = T>,
        Self: ReprC<CType = Native<AbiOption<<T as ReprC>::CType>>>,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        T: RustSpec<Niche = rust_spec::niche::WithoutNiche>,
        &'d T: DecodeMoveUnchecked<'d, SoftStore: Store + Default + Sized>
            + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
        <T as ReprC>::CType: Copy,
    {
        type SoftStore =
            RefSizedDecodeStore<Self, Option<<&'d T as DecodeMoveUnchecked<'d>>::SoftStore>>;

        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d Self {
            let source = unsafe { source.into_ref().unwrap_unchecked() };
            store
                .decode(|fields| match source.as_option()? {
                    None => {
                        *fields = None;
                        Some(Self::None)
                    }
                    Some(value) => {
                        let field = fields.insert(Default::default());
                        let value = unsafe {
                            decode_ref_field_unchecked::<T>(RefCarrier::from_raw(value), field)
                        };
                        Some(Self::Some(value))
                    }
                })
                .expect("unchecked decoding produces a value")
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, T: ReprC + RustSpec<Layout: Sized> + 'd, E: ReprC + RustSpec<Layout: Sized> + 'd>
        SoftDecodeRefUnchecked<'d> for Result<T, E>
    where
        Self:
            ReprC<CType = Native<crate::result::abi::Result<<T as ReprC>::CType, <E as ReprC>::CType>>>,
        crate::sync::DecodedFieldUnchecked<
            'd,
            E,
            <&'d E as DecodeMoveUnchecked<'d>>::SoftStore,
            <E as RustSpec>::Layout,
        >: TakeDecoded<Value = E>,
        crate::sync::DecodedFieldUnchecked<
            'd,
            T,
            <&'d T as DecodeMoveUnchecked<'d>>::SoftStore,
            <T as RustSpec>::Layout,
        >: TakeDecoded<Value = T>,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'d T: DecodeMoveUnchecked<'d>,
        &'d E: DecodeMoveUnchecked<'d>,
        <T as ReprC>::CType: Copy,
        <E as ReprC>::CType: Copy,
        &'d T: DecodeMoveUnchecked<'d> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
        &'d E: DecodeMoveUnchecked<'d> + ReprC<CType: RefCarrier<Pointee = <E as ReprC>::CType>>,
    {
        type SoftStore = RefSizedDecodeStore<
            Self,
            Option<
                Result<
                    <&'d T as DecodeMoveUnchecked<'d>>::SoftStore,
                    <&'d E as DecodeMoveUnchecked<'d>>::SoftStore,
                >,
            >,
        >;

        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d Self {
            let source = unsafe { source.into_ref().unwrap_unchecked() };
            store
                .decode(|fields| match source.as_result()? {
                    Ok(value) => {
                        let Ok(field) = fields.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        let value = unsafe {
                            decode_ref_field_unchecked::<T>(RefCarrier::from_raw(value), field)
                        };
                        Some(Self::Ok(value))
                    }
                    Err(value) => {
                        let Err(field) = fields.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        let value = unsafe {
                            decode_ref_field_unchecked::<E>(RefCarrier::from_raw(value), field)
                        };
                        Some(Self::Err(value))
                    }
                })
                .expect("unchecked decoding produces a value")
        }
    }

    unsafe impl<'d, T: Niche + RustSpec<Layout: Sized> + 'd, N: rust_spec::layout::LayoutKind>
        SoftDecodeRefUnchecked<'d> for Option<T>
    where
        crate::sync::DecodedFieldUnchecked<
            'd,
            T,
            <&'d T as DecodeMoveUnchecked<'d>>::SoftStore,
            <T as RustSpec>::Layout,
        >: TakeDecoded<Value = T>,
        T: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<CType = Native<AbiOptionSome<<T as ReprC>::CType>>>,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'d T: DecodeMoveUnchecked<'d, SoftStore: Store + Default + Sized>
            + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    {
        type SoftStore =
            RefSizedDecodeStore<Self, Option<<&'d T as DecodeMoveUnchecked<'d>>::SoftStore>>;

        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d Self {
            let value = &unsafe { source.into_ref().unwrap_unchecked() }.0;
            store
                .decode(|field| {
                    if T::is_niche(&value.0) {
                        *field = None;
                        return Some(None);
                    }
                    let value = unsafe {
                        decode_ref_field_unchecked::<T>(
                            RefCarrier::from_raw(&value.0),
                            field.insert(Default::default()),
                        )
                    };
                    Some(Some(value))
                })
                .expect("unchecked decoding produces a value")
        }
    }
    unsafe impl<
        'd,
        T: Niche + RustSpec<Layout: Sized> + 'd,
        E: Default + ReprC + 'd,
        N: rust_spec::layout::LayoutKind,
    > SoftDecodeRefUnchecked<'d> for Result<T, E>
    where
        T: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultOk<<T as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        crate::sync::DecodedFieldUnchecked<
            'd,
            T,
            <&'d T as DecodeMoveUnchecked<'d>>::SoftStore,
            <T as RustSpec>::Layout,
        >: TakeDecoded<Value = T>,
        T: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'d T: DecodeMoveUnchecked<'d>,
        <E as ReprC>::CType: Sized,
        &'d T: DecodeMoveUnchecked<'d> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    {
        type SoftStore =
            RefSizedDecodeStore<Self, Option<<&'d T as DecodeMoveUnchecked<'d>>::SoftStore>>;

        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d Self {
            let value = &unsafe { source.into_ref().unwrap_unchecked() }.0;
            store
                .decode(|field| {
                    if T::is_niche(&value.0) {
                        *field = None;
                        return Some(Err(E::default()));
                    }
                    let value = unsafe {
                        decode_ref_field_unchecked::<T>(
                            RefCarrier::from_raw(&value.0),
                            field.insert(Default::default()),
                        )
                    };
                    Some(Ok(value))
                })
                .expect("unchecked decoding produces a value")
        }
    }
    unsafe impl<
        'd,
        E: Niche + RustSpec<Layout: Sized> + 'd,
        T: Default + ReprC + 'd,
        N: rust_spec::layout::LayoutKind,
    > SoftDecodeRefUnchecked<'d> for Result<T, E>
    where
        E: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultErr<<T as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        crate::sync::DecodedFieldUnchecked<
            'd,
            E,
            <&'d E as DecodeMoveUnchecked<'d>>::SoftStore,
            <E as RustSpec>::Layout,
        >: TakeDecoded<Value = E>,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        T: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'d E: DecodeMoveUnchecked<'d>,
        <T as ReprC>::CType: Sized,
        &'d E: DecodeMoveUnchecked<'d> + ReprC<CType: RefCarrier<Pointee = <E as ReprC>::CType>>,
    {
        type SoftStore =
            RefSizedDecodeStore<Self, Option<<&'d E as DecodeMoveUnchecked<'d>>::SoftStore>>;

        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d Self {
            let value = &unsafe { source.into_ref().unwrap_unchecked() }.0;
            store
                .decode(|field| {
                    if E::is_niche(&value.0) {
                        *field = None;
                        return Some(Ok(T::default()));
                    }
                    let value = unsafe {
                        decode_ref_field_unchecked::<E>(
                            RefCarrier::from_raw(&value.0),
                            field.insert(Default::default()),
                        )
                    };
                    Some(Err(value))
                })
                .expect("unchecked decoding produces a value")
        }
    }
}
disjoint_impls! {
    /// Decodes a shared reference under the source language's validity rules.
    ///
    /// # Safety
    /// Implementations must validate before exposing a Rust reference and
    /// keep its backing storage live for the borrow. Validation through a
    /// shared reference must never read interior-mutable contents.
    pub unsafe trait SoftDecodeRef<'d, L: Language = lang::C>: ReprC + 'd {
        type Store: Store + Default;

        /// Validate and decode the reference without taking ownership of its source.
        ///
        /// # Safety
        /// The source must satisfy the pointer, lifetime and aliasing requirements
        /// of `crate::soft_decode`.
        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self>
        where
            &'d Self: ReprC<CType: Sized>;
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, L: Language, T: 'd, S: rust_spec::size::SizedKind> SoftDecodeRef<'d, L> for Box<T>
    where
        Self: ReprC<CType = AbiBox<<T as ReprC>::CType>>,
        T: ReprC + RustSpec<Size = rust_spec::size::Sized<S>>,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'d T: DecodeMove<'d, L, Store: Store + Default + Sized + TakeDecoded<Value = T>>
            + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
        <T as ReprC>::CType: Sized,
    {
        type Store = RefSizedDecodeStore<Self, <&'d T as DecodeMove<'d, L>>::Store>;

        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let header = unsafe { *source.into_ref()? };
            let carrier = unsafe { RefCarrier::from_raw(header.as_ptr()) };
            store.decode(|field| {
                let value = unsafe { DecodedField::<T, _, Unspecified, L>::decode(carrier, field)? };
                Some(Box::new(value))
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, L: Language, T: ?Sized + alloc::borrow::ToOwned + 'd, __Interior: ExclusiveKind>
        SoftDecodeRef<'d, L> for Box<T>
    where
        Self: ReprC<CType = AbiWideBox<<T as ReprC>::CType>>,
        T: ReprC<
                CType: crate::CType
                           + crate::wide::Wide<
                    Header: crate::wide::WideHeader<Data: CType>,
                    Metadata = usize,
                >,
            > + RustSpec<
                Layout = rust_spec::Specified,
                Mutability = Exclusive<__Interior>,
                Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>,
            >,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'d T: DecodeMove<'d, L, Store: Store + Default + Sized>
            + ReprC<CType: WideRefCarrier<Pointee = <T as ReprC>::CType>>,
        Self: From<<T as alloc::borrow::ToOwned>::Owned>,
    {
        type Store = RefSizedDecodeStore<Self, <&'d T as DecodeMove<'d, L>>::Store>;
        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let header = unsafe { *source.into_ref()? };
            let carrier = unsafe {
                WideRefCarrier::from_raw_parts(header.as_ptr().cast_const(), header.metadata())
            };
            store.decode(|field| {
                let value = unsafe { <&'d T as DecodeMove<'d, L>>::soft_decode(carrier, field)? };
                Some(alloc::borrow::ToOwned::to_owned(value).into())
            })
        }
    }
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, L: Language, T: ?Sized + 'd> SoftDecodeRef<'d, L> for Box<T>
    where
        Self: ReprC<CType = AbiWideBox<<T as ReprC>::CType>>,
        T: ReprC<
                CType: crate::CType
                           + crate::wide::Wide<
                    Header: crate::wide::WideHeader<Data: CType>,
                    Metadata = usize,
                >,
            > + RustSpec<
                Layout = Unspecified,
                Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>,
            >,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'d T: DecodeMove<'d, L, Store: Store + Default + Sized + TakeDecoded>
            + ReprC<CType: WideRefCarrier<Pointee = <T as ReprC>::CType>>,
        Self: From<<<&'d T as DecodeMove<'d, L>>::Store as TakeDecoded>::Value>,
    {
        type Store = RefSizedDecodeStore<Self, <&'d T as DecodeMove<'d, L>>::Store>;
        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let header = unsafe { *source.into_ref()? };
            let carrier = unsafe {
                WideRefCarrier::from_raw_parts(header.as_ptr().cast_const(), header.metadata())
            };
            store.decode(|field| {
                let value = unsafe { DecodedField::<T, _, Unspecified, L>::decode(carrier, field)? };
                Some(value.into())
            })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, L: Language, T: ReprC + RustSpec<Layout: Sized> + 'd> SoftDecodeRef<'d, L>
        for Option<T>
    where
        crate::sync::DecodedField<
            'd,
            T,
            <&'d T as DecodeMove<'d, L>>::Store,
            <T as RustSpec>::Layout,
            L,
        >: TakeDecoded<Value = T>,
        Self: ReprC<CType = Native<AbiOption<<T as ReprC>::CType>>>,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        T: RustSpec<Niche = rust_spec::niche::WithoutNiche>,
        &'d T: DecodeMove<'d, L, Store: Store + Default + Sized>
            + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
        <T as ReprC>::CType: Copy,
    {
        type Store = RefSizedDecodeStore<Self, Option<<&'d T as DecodeMove<'d, L>>::Store>>;
        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let source = unsafe { source.into_ref()? };
            store.decode(|fields| match source.as_option()? {
                None => {
                    *fields = None;
                    Some(Self::None)
                }
                Some(value) => {
                    let field = fields.insert(Default::default());
                    let value =
                        unsafe { decode_ref_field::<T, L>(RefCarrier::from_raw(value), field) }?;
                    Some(Self::Some(value))
                }
            })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<
        'd,
        L: Language,
        T: ReprC + RustSpec<Layout: Sized> + 'd,
        E: ReprC + RustSpec<Layout: Sized> + 'd,
    > SoftDecodeRef<'d, L> for Result<T, E>
    where
        Self:
            ReprC<CType = Native<crate::result::abi::Result<<T as ReprC>::CType, <E as ReprC>::CType>>>,
        crate::sync::DecodedField<
            'd,
            E,
            <&'d E as DecodeMove<'d, L>>::Store,
            <E as RustSpec>::Layout,
            L,
        >: TakeDecoded<Value = E>,
        crate::sync::DecodedField<
            'd,
            T,
            <&'d T as DecodeMove<'d, L>>::Store,
            <T as RustSpec>::Layout,
            L,
        >: TakeDecoded<Value = T>,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'d T: DecodeMove<'d, L>,
        &'d E: DecodeMove<'d, L>,
        <T as ReprC>::CType: Copy,
        <E as ReprC>::CType: Copy,
        &'d T: ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
        &'d E: ReprC<CType: RefCarrier<Pointee = <E as ReprC>::CType>>,
    {
        type Store = RefSizedDecodeStore<
            Self,
            Option<Result<<&'d T as DecodeMove<'d, L>>::Store, <&'d E as DecodeMove<'d, L>>::Store>>,
        >;
        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let source = unsafe { source.into_ref()? };
            store.decode(|fields| match source.as_result()? {
                Ok(value) => {
                    let Ok(field) = fields.insert(Ok(Default::default())) else {
                        unreachable!()
                    };
                    let value =
                        unsafe { decode_ref_field::<T, L>(RefCarrier::from_raw(value), field) }?;
                    Some(Self::Ok(value))
                }
                Err(value) => {
                    let Err(field) = fields.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    let value =
                        unsafe { decode_ref_field::<E, L>(RefCarrier::from_raw(value), field) }?;
                    Some(Self::Err(value))
                }
            })
        }
    }
    unsafe impl<
        'd,
        L: Language,
        T: Niche + RustSpec<Layout: Sized> + 'd,
        N: rust_spec::layout::LayoutKind,
    > SoftDecodeRef<'d, L> for Option<T>
    where
        crate::sync::DecodedField<
            'd,
            T,
            <&'d T as DecodeMove<'d, L>>::Store,
            <T as RustSpec>::Layout,
            L,
        >: TakeDecoded<Value = T>,
        T: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<CType = Native<AbiOptionSome<<T as ReprC>::CType>>>,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'d T: DecodeMove<'d, L, Store: Store + Default + Sized>
            + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    {
        type Store = RefSizedDecodeStore<Self, Option<<&'d T as DecodeMove<'d, L>>::Store>>;
        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let value = &unsafe { source.into_ref()? }.0;
            store.decode(|field| {
                if T::is_niche(&value.0) {
                    *field = None;
                    return Some(None);
                }
                let value = unsafe {
                    decode_ref_field::<T, L>(
                        RefCarrier::from_raw(&value.0),
                        field.insert(Default::default()),
                    )
                }?;
                Some(Some(value))
            })
        }
    }
    unsafe impl<
        'd,
        L: Language,
        T: Niche + RustSpec<Layout: Sized> + 'd,
        E: Default + ReprC + 'd,
        N: rust_spec::layout::LayoutKind,
    > SoftDecodeRef<'d, L> for Result<T, E>
    where
        T: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultOk<<T as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        crate::sync::DecodedField<
            'd,
            T,
            <&'d T as DecodeMove<'d, L>>::Store,
            <T as RustSpec>::Layout,
            L,
        >: TakeDecoded<Value = T>,
        T: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'d T: DecodeMove<'d, L>,
        <E as ReprC>::CType: Sized,
        &'d T: ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    {
        type Store = RefSizedDecodeStore<Self, Option<<&'d T as DecodeMove<'d, L>>::Store>>;
        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let value = &unsafe { source.into_ref()? }.0;
            store.decode(|field| {
                if T::is_niche(&value.0) {
                    *field = None;
                    return Some(Err(E::default()));
                }
                let value = unsafe {
                    decode_ref_field::<T, L>(
                        RefCarrier::from_raw(&value.0),
                        field.insert(Default::default()),
                    )
                }?;
                Some(Ok(value))
            })
        }
    }
    unsafe impl<
        'd,
        L: Language,
        E: Niche + RustSpec<Layout: Sized> + 'd,
        T: Default + ReprC + 'd,
        N: rust_spec::layout::LayoutKind,
    > SoftDecodeRef<'d, L> for Result<T, E>
    where
        E: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultErr<<T as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        crate::sync::DecodedField<
            'd,
            E,
            <&'d E as DecodeMove<'d, L>>::Store,
            <E as RustSpec>::Layout,
            L,
        >: TakeDecoded<Value = E>,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        T: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'d E: DecodeMove<'d, L>,
        <T as ReprC>::CType: Sized,
        &'d E: ReprC<CType: RefCarrier<Pointee = <E as ReprC>::CType>>,
    {
        type Store = RefSizedDecodeStore<Self, Option<<&'d E as DecodeMove<'d, L>>::Store>>;
        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let value = &unsafe { source.into_ref()? }.0;
            store.decode(|field| {
                if E::is_niche(&value.0) {
                    *field = None;
                    return Some(Ok(T::default()));
                }
                let value = unsafe {
                    decode_ref_field::<E, L>(
                        RefCarrier::from_raw(&value.0),
                        field.insert(Default::default()),
                    )
                }?;
                Some(Err(value))
            })
        }
    }
}

// Null-terminated referents are implemented individually so downstream types
// can provide their own reference decoder.
#[cfg(feature = "repr-rust")]
macro_rules! impl_nul_decode_ref {
    ($ty:ty) => {
        unsafe impl<'d> SoftDecodeRefUnchecked<'d> for $ty {
            type SoftStore = ();

            unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
                source: <&'d Self as ReprC>::CType,
                _: &'itm mut (),
            ) -> &'d Self {
                unsafe { <Self as NulTerminatedBuf>::from_raw(source.as_ptr().cast()) }
            }
        }
        unsafe impl<'d, L: Language> SoftDecodeRef<'d, L> for $ty {
            type Store = ();

            unsafe fn soft_decode_ref<'itm: 'd>(
                source: <&'d Self as ReprC>::CType,
                store: &'itm mut (),
            ) -> Option<&'d Self> {
                if source.as_ptr().is_null() {
                    return None;
                }
                Some(unsafe {
                    <Self as crate::convert::SoftDecodeRefUnchecked<'d>>::soft_decode_ref_unchecked(
                        source, store,
                    )
                })
            }
        }
    };
}

#[cfg(feature = "repr-rust")]
impl_nul_decode_ref!(core::ffi::CStr);
#[cfg(feature = "repr-rust")]
impl_nul_decode_ref!(core::mem::ManuallyDrop<core::ffi::CStr>);

disjoint_impls! {
    /// Decodes a carrier whose Rust validity is guaranteed by the caller.
    ///
    /// # Safety
    /// Implementations must preserve ownership, lifetimes and aliasing. Shared
    /// interior-mutable contents must never be inspected. An empty conversion
    /// store must not provide backing storage for the returned value.
    pub unsafe trait SoftDecodeMutUnchecked<'d>: ReprC + 'd {
        /// Storage retained for the unchecked conversion.
        type SoftStore: Store + Default;

        /// Decode without validating the source value.
        ///
        /// # Safety
        /// The source must satisfy all Rust validity, access and ownership
        /// requirements, including those of any references in the result.
        unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d mut Self
        where
            &'d mut Self: ReprC<CType: Sized>;
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, T: ReprC + 'd> SoftDecodeMutUnchecked<'d> for Option<T>
    where
        T: crate::borrow::BorrowedDecodeUnchecked<'d>,
        Self: EncodeMoveUnchecked<SoftStore: EmptyStore>
            + RustSpec
            + ReprC<CType = Native<AbiOption<<T as ReprC>::CType>>>,
        &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
        T: RustSpec<Niche = rust_spec::niche::WithoutNiche>,
        <T as ReprC>::CType: Copy,
    {
        type SoftStore = RefMutSizedDecodeStoreUnchecked<
            Self,
            Option<<T as crate::borrow::BorrowedDecodeUnchecked<'d>>::SoftStore>,
        >;

        unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d mut Self {
            let pointer = source.as_ptr();
            let source = unsafe { &*pointer };
            unsafe {
                store.decode(pointer, |fields| match source.as_option()? {
                    None => {
                        *fields = None;
                        Some(Self::None)
                    }
                    Some(value) => {
                        let field = fields.insert(Default::default());
                        let value = decode_mut_field_unchecked::<T>(*value, field);
                        Some(Self::Some(value))
                    }
                })
            }
            .expect("unchecked decoding produces a value")
        }
    }

    unsafe impl<'d, T: ReprC + 'd, E: ReprC + 'd> SoftDecodeMutUnchecked<'d> for Result<T, E>
    where
        T: crate::borrow::BorrowedDecodeUnchecked<'d>,
        E: crate::borrow::BorrowedDecodeUnchecked<'d>,
        Self: EncodeMoveUnchecked<SoftStore: EmptyStore>
            + RustSpec
            + ReprC<CType = Native<AbiResult<<T as ReprC>::CType, <E as ReprC>::CType>>>,
        &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
        <T as ReprC>::CType: Copy,
        <E as ReprC>::CType: Copy,
    {
        type SoftStore = RefMutSizedDecodeStoreUnchecked<
            Self,
            Option<
                Result<
                    <T as crate::borrow::BorrowedDecodeUnchecked<'d>>::SoftStore,
                    <E as crate::borrow::BorrowedDecodeUnchecked<'d>>::SoftStore,
                >,
            >,
        >;

        unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d mut Self {
            let pointer = source.as_ptr();
            let source = unsafe { &*pointer };
            unsafe {
                store.decode(pointer, |fields| match source.as_result()? {
                    Ok(value) => {
                        let Ok(field) = fields.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        let value = decode_mut_field_unchecked::<T>(*value, field);
                        Some(Self::Ok(value))
                    }
                    Err(value) => {
                        let Err(field) = fields.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        let value = decode_mut_field_unchecked::<E>(*value, field);
                        Some(Self::Err(value))
                    }
                })
            }
            .expect("unchecked decoding produces a value")
        }
    }

    unsafe impl<'d, T: Niche + 'd, N: rust_spec::layout::LayoutKind> SoftDecodeMutUnchecked<'d>
        for Option<T>
    where
        T: crate::borrow::BorrowedDecodeUnchecked<'d>,
        T: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: EncodeMoveUnchecked<SoftStore: EmptyStore>
            + RustSpec
            + ReprC<CType = Native<AbiOptionSome<<T as ReprC>::CType>>>,
        &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    {
        type SoftStore = RefMutSizedDecodeStoreUnchecked<
            Self,
            Option<<T as crate::borrow::BorrowedDecodeUnchecked<'d>>::SoftStore>,
        >;

        unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d mut Self {
            let pointer = source.as_ptr();
            let value = &unsafe { &*pointer }.0;
            unsafe {
                store.decode(pointer, |field| {
                    if T::is_niche(&value.0) {
                        *field = None;
                        return Some(None);
                    }
                    let value =
                        decode_mut_field_unchecked::<T>(value.0, field.insert(Default::default()));
                    Some(Some(value))
                })
            }
            .expect("unchecked decoding produces a value")
        }
    }
    unsafe impl<'d, T: Niche + 'd, E: Default + ReprC + 'd, N: rust_spec::layout::LayoutKind>
        SoftDecodeMutUnchecked<'d> for Result<T, E>
    where
        T: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        T: crate::borrow::BorrowedDecodeUnchecked<'d>,
        T: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Self: EncodeMoveUnchecked<SoftStore: EmptyStore>
            + RustSpec
            + ReprC<CType = Native<AbiResultOk<<T as ReprC>::CType, <E as ReprC>::CType>>>,
        &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
        <T as ReprC>::CType: Copy,
        <E as ReprC>::CType: Sized,
    {
        type SoftStore = RefMutSizedDecodeStoreUnchecked<
            Self,
            Option<<T as crate::borrow::BorrowedDecodeUnchecked<'d>>::SoftStore>,
        >;

        unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d mut Self {
            let pointer = source.as_ptr();
            let value = &unsafe { &*pointer }.0;
            unsafe {
                store.decode(pointer, |field| {
                    if T::is_niche(&value.0) {
                        *field = None;
                        return Some(Err(E::default()));
                    }
                    let value =
                        decode_mut_field_unchecked::<T>(value.0, field.insert(Default::default()));
                    Some(Ok(value))
                })
            }
            .expect("unchecked decoding produces a value")
        }
    }
    unsafe impl<'d, E: Niche + 'd, T: Default + ReprC + 'd, N: rust_spec::layout::LayoutKind>
        SoftDecodeMutUnchecked<'d> for Result<T, E>
    where
        E: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        E: crate::borrow::BorrowedDecodeUnchecked<'d>,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        T: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Self: EncodeMoveUnchecked<SoftStore: EmptyStore>
            + RustSpec
            + ReprC<CType = Native<AbiResultErr<<T as ReprC>::CType, <E as ReprC>::CType>>>,
        &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
        <E as ReprC>::CType: Copy,
        <T as ReprC>::CType: Sized,
    {
        type SoftStore = RefMutSizedDecodeStoreUnchecked<
            Self,
            Option<<E as crate::borrow::BorrowedDecodeUnchecked<'d>>::SoftStore>,
        >;

        unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d mut Self {
            let pointer = source.as_ptr();
            let value = &unsafe { &*pointer }.0;
            unsafe {
                store.decode(pointer, |field| {
                    if E::is_niche(&value.0) {
                        *field = None;
                        return Some(Ok(T::default()));
                    }
                    let value =
                        decode_mut_field_unchecked::<E>(value.0, field.insert(Default::default()));
                    Some(Err(value))
                })
            }
            .expect("unchecked decoding produces a value")
        }
    }
}
disjoint_impls! {
    /// Decodes an exclusive reference under the source language's validity rules.
    ///
    /// # Safety
    /// Implementations must validate before exposing a Rust reference, preserve
    /// exclusive access for the borrow, and synchronize writes to any temporary
    /// back to the source without transferring its ownership.
    pub unsafe trait SoftDecodeMut<'d, L: Language = lang::C>: ReprC + 'd {
        type Store: Store + Default;

        /// Validate and decode the reference without taking ownership of its source.
        ///
        /// # Safety
        /// The source must satisfy the pointer, lifetime and aliasing requirements
        /// of `crate::soft_decode`. Mutable writes must be synchronized back without
        /// transferring ownership.
        unsafe fn soft_decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self>
        where
            &'d mut Self: ReprC<CType: Sized>;
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, L: Language, T: ReprC + 'd> SoftDecodeMut<'d, L> for Option<T>
    where
        T: crate::borrow::BorrowedDecode<'d, L>,
        Self: EncodeMove<Store: EmptyStore>
            + RustSpec
            + ReprC<CType = Native<AbiOption<<T as ReprC>::CType>>>,
        &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
        T: RustSpec<Niche = rust_spec::niche::WithoutNiche>,
        <T as ReprC>::CType: Copy,
    {
        type Store =
            RefMutSizedDecodeStore<Self, Option<<T as crate::borrow::BorrowedDecode<'d, L>>::Store>>;
        unsafe fn soft_decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let pointer = source.as_ptr();
            let source = unsafe { pointer.as_ref()? };
            unsafe {
                store.decode(pointer, |fields| match source.as_option()? {
                    None => {
                        *fields = None;
                        Some(Self::None)
                    }
                    Some(value) => {
                        let field = fields.insert(Default::default());
                        let value = decode_mut_field::<T, L>(*value, field)?;
                        Some(Self::Some(value))
                    }
                })
            }
        }
    }
    unsafe impl<'d, L: Language, T: ReprC + 'd, E: ReprC + 'd> SoftDecodeMut<'d, L> for Result<T, E>
    where
        T: crate::borrow::BorrowedDecode<'d, L>,
        E: crate::borrow::BorrowedDecode<'d, L>,
        Self: EncodeMove<Store: EmptyStore>
            + RustSpec
            + ReprC<CType = Native<AbiResult<<T as ReprC>::CType, <E as ReprC>::CType>>>,
        &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
        <T as ReprC>::CType: Copy,
        <E as ReprC>::CType: Copy,
    {
        type Store = RefMutSizedDecodeStore<
            Self,
            Option<
                Result<
                    <T as crate::borrow::BorrowedDecode<'d, L>>::Store,
                    <E as crate::borrow::BorrowedDecode<'d, L>>::Store,
                >,
            >,
        >;
        unsafe fn soft_decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let pointer = source.as_ptr();
            let source = unsafe { pointer.as_ref()? };
            unsafe {
                store.decode(pointer, |fields| match source.as_result()? {
                    Ok(value) => {
                        let Ok(field) = fields.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        let value = decode_mut_field::<T, L>(*value, field)?;
                        Some(Self::Ok(value))
                    }
                    Err(value) => {
                        let Err(field) = fields.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        let value = decode_mut_field::<E, L>(*value, field)?;
                        Some(Self::Err(value))
                    }
                })
            }
        }
    }
    unsafe impl<'d, L: Language, T: Niche + 'd, N: rust_spec::layout::LayoutKind> SoftDecodeMut<'d, L>
        for Option<T>
    where
        T: crate::borrow::BorrowedDecode<'d, L>,
        T: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: EncodeMove<Store: EmptyStore>
            + RustSpec
            + ReprC<CType = Native<AbiOptionSome<<T as ReprC>::CType>>>,
        &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    {
        type Store =
            RefMutSizedDecodeStore<Self, Option<<T as crate::borrow::BorrowedDecode<'d, L>>::Store>>;
        unsafe fn soft_decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let pointer = source.as_ptr();
            let value = &unsafe { pointer.as_ref()? }.0;
            unsafe {
                store.decode(pointer, |field| {
                    if T::is_niche(&value.0) {
                        *field = None;
                        return Some(None);
                    }
                    let value = decode_mut_field::<T, L>(value.0, field.insert(Default::default()))?;
                    Some(Some(value))
                })
            }
        }
    }
    unsafe impl<
        'd,
        L: Language,
        T: Niche + 'd,
        E: Default + ReprC + 'd,
        N: rust_spec::layout::LayoutKind,
    > SoftDecodeMut<'d, L> for Result<T, E>
    where
        T: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        T: crate::borrow::BorrowedDecode<'d, L>,
        T: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Self: EncodeMove<Store: EmptyStore>
            + RustSpec
            + ReprC<CType = Native<AbiResultOk<<T as ReprC>::CType, <E as ReprC>::CType>>>,
        &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
        <T as ReprC>::CType: Copy,
        <E as ReprC>::CType: Sized,
    {
        type Store =
            RefMutSizedDecodeStore<Self, Option<<T as crate::borrow::BorrowedDecode<'d, L>>::Store>>;
        unsafe fn soft_decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let pointer = source.as_ptr();
            let value = &unsafe { pointer.as_ref()? }.0;
            unsafe {
                store.decode(pointer, |field| {
                    if T::is_niche(&value.0) {
                        *field = None;
                        return Some(Err(E::default()));
                    }
                    let value = decode_mut_field::<T, L>(value.0, field.insert(Default::default()))?;
                    Some(Ok(value))
                })
            }
        }
    }
    unsafe impl<
        'd,
        L: Language,
        E: Niche + 'd,
        T: Default + ReprC + 'd,
        N: rust_spec::layout::LayoutKind,
    > SoftDecodeMut<'d, L> for Result<T, E>
    where
        E: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        E: crate::borrow::BorrowedDecode<'d, L>,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        T: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Self: EncodeMove<Store: EmptyStore>
            + RustSpec
            + ReprC<CType = Native<AbiResultErr<<T as ReprC>::CType, <E as ReprC>::CType>>>,
        &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
        <E as ReprC>::CType: Copy,
        <T as ReprC>::CType: Sized,
    {
        type Store =
            RefMutSizedDecodeStore<Self, Option<<E as crate::borrow::BorrowedDecode<'d, L>>::Store>>;
        unsafe fn soft_decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let pointer = source.as_ptr();
            let value = &unsafe { pointer.as_ref()? }.0;
            unsafe {
                store.decode(pointer, |field| {
                    if E::is_niche(&value.0) {
                        *field = None;
                        return Some(Ok(T::default()));
                    }
                    let value = decode_mut_field::<E, L>(value.0, field.insert(Default::default()))?;
                    Some(Err(value))
                })
            }
        }
    }
}

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<'d, R: ReprC<CType: CType + Sized + 'd> + RustSpec + 'd, __Interior: ExclusiveKind>
    SoftDecodeRefUnchecked<'d> for [R]
where
    crate::sync::DecodedFieldUnchecked<
        'd,
        R,
        <&'d R as DecodeMoveUnchecked<'d>>::SoftStore,
        <R as RustSpec>::Layout,
    >: TakeDecoded<Value = R>,
    [R]: RustSpec<Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>,
    &'d Self: ReprC<CType = crate::wide::abi::SoftWideRef<<Self as ReprC>::CType>>,
    &'d R: DecodeMoveUnchecked<'d> + ReprC<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
{
    type SoftStore =
        RefSizedDecodeStore<Vec<R>, Box<[<&'d R as DecodeMoveUnchecked<'d>>::SoftStore]>>;

    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d Self {
        store
            .decode(|fields| {
                let source: &'d [<R as ReprC>::CType] =
                    unsafe { source.into_ref().unwrap_unchecked() };
                *fields = core::iter::repeat_with(Default::default)
                    .take(source.len())
                    .collect();
                let mut values = Vec::with_capacity(source.len());
                for (value, field_store) in source.iter().zip(fields.iter_mut()) {
                    let carrier = unsafe { RefCarrier::from_raw(value) };
                    let value = unsafe { decode_ref_field_unchecked::<R>(carrier, field_store) };
                    values.push(value);
                }
                Some(values)
            })
            .expect("unchecked decoding produces a value")
            .as_slice()
    }
}
#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<
    'd,
    L: Language,
    R: ReprC<CType: CType + Sized + 'd> + RustSpec + 'd,
    __Interior: ExclusiveKind,
> SoftDecodeRef<'d, L> for [R]
where
    crate::sync::DecodedField<
        'd,
        R,
        <&'d R as DecodeMove<'d, L>>::Store,
        <R as RustSpec>::Layout,
        L,
    >: TakeDecoded<Value = R>,
    [R]: RustSpec<Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>,
    &'d Self: ReprC<CType = crate::wide::abi::SoftWideRef<<Self as ReprC>::CType>>,
    &'d R: DecodeMove<'d, L> + ReprC<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
{
    type Store = RefSizedDecodeStore<Vec<R>, Box<[<&'d R as DecodeMove<'d, L>>::Store]>>;

    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        store
            .decode(|fields| {
                let source: &'d [<R as ReprC>::CType] = unsafe { source.into_ref()? };
                *fields = core::iter::repeat_with(Default::default)
                    .take(source.len())
                    .collect();
                let mut values = Vec::with_capacity(source.len());
                for (value, field_store) in source.iter().zip(fields.iter_mut()) {
                    let carrier = unsafe { RefCarrier::from_raw(value) };
                    let value = unsafe { decode_ref_field::<R, L>(carrier, field_store)? };
                    values.push(value);
                }
                Some(values)
            })
            .map(Vec::as_slice)
    }
}

unsafe impl<'d, T: ReprC + RustSpec + 'd> SoftDecodeRefUnchecked<'d> for crate::option::Option<T>
where
    crate::sync::DecodedFieldUnchecked<
        'd,
        T,
        <&'d T as DecodeMoveUnchecked<'d>>::SoftStore,
        <T as RustSpec>::Layout,
    >: TakeDecoded<Value = T>,
    Self: ReprC<CType = AbiOption<<T as ReprC>::CType>>,
    &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'d T: DecodeMoveUnchecked<'d> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
{
    type SoftStore =
        RefSizedDecodeStore<Self, Option<<&'d T as DecodeMoveUnchecked<'d>>::SoftStore>>;

    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d Self {
        let source = unsafe { source.into_ref().unwrap_unchecked() };
        store
            .decode(|fields| match source.as_option()? {
                None => {
                    *fields = None;
                    Some(Self::None)
                }
                Some(value) => {
                    let field = fields.insert(Default::default());
                    let value = unsafe {
                        decode_ref_field_unchecked::<T>(RefCarrier::from_raw(value), field)
                    };
                    Some(Self::Some(value))
                }
            })
            .expect("unchecked decoding produces a value")
    }
}
unsafe impl<'d, L: Language, T: ReprC + RustSpec + 'd> SoftDecodeRef<'d, L>
    for crate::option::Option<T>
where
    crate::sync::DecodedField<
        'd,
        T,
        <&'d T as DecodeMove<'d, L>>::Store,
        <T as RustSpec>::Layout,
        L,
    >: TakeDecoded<Value = T>,
    Self: ReprC<CType = AbiOption<<T as ReprC>::CType>>,
    &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'d T: DecodeMove<'d, L> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
{
    type Store = RefSizedDecodeStore<Self, Option<<&'d T as DecodeMove<'d, L>>::Store>>;
    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        let source = unsafe { source.into_ref()? };
        store.decode(|fields| match source.as_option()? {
            None => {
                *fields = None;
                Some(Self::None)
            }
            Some(value) => {
                let field = fields.insert(Default::default());
                let value =
                    unsafe { decode_ref_field::<T, L>(RefCarrier::from_raw(value), field) }?;
                Some(Self::Some(value))
            }
        })
    }
}

unsafe impl<'d, T: ReprC + RustSpec + 'd, E: ReprC + RustSpec + 'd> SoftDecodeRefUnchecked<'d>
    for crate::result::Result<T, E>
where
    crate::sync::DecodedFieldUnchecked<
        'd,
        E,
        <&'d E as DecodeMoveUnchecked<'d>>::SoftStore,
        <E as RustSpec>::Layout,
    >: TakeDecoded<Value = E>,
    crate::sync::DecodedFieldUnchecked<
        'd,
        T,
        <&'d T as DecodeMoveUnchecked<'d>>::SoftStore,
        <T as RustSpec>::Layout,
    >: TakeDecoded<Value = T>,
    Self: ReprC<CType = AbiResult<<T as ReprC>::CType, <E as ReprC>::CType>>,
    &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'d T: DecodeMoveUnchecked<'d> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    &'d E: DecodeMoveUnchecked<'d> + ReprC<CType: RefCarrier<Pointee = <E as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
    <E as ReprC>::CType: Copy,
{
    type SoftStore = RefSizedDecodeStore<
        Self,
        Option<
            Result<
                <&'d T as DecodeMoveUnchecked<'d>>::SoftStore,
                <&'d E as DecodeMoveUnchecked<'d>>::SoftStore,
            >,
        >,
    >;

    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d Self {
        let source = unsafe { source.into_ref().unwrap_unchecked() };
        store
            .decode(|fields| match source.as_result()? {
                Ok(value) => {
                    let Ok(field) = fields.insert(Ok(Default::default())) else {
                        unreachable!()
                    };
                    let value = unsafe {
                        decode_ref_field_unchecked::<T>(RefCarrier::from_raw(value), field)
                    };
                    Some(Self::Ok(value))
                }
                Err(value) => {
                    let Err(field) = fields.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    let value = unsafe {
                        decode_ref_field_unchecked::<E>(RefCarrier::from_raw(value), field)
                    };
                    Some(Self::Err(value))
                }
            })
            .expect("unchecked decoding produces a value")
    }
}
unsafe impl<'d, L: Language, T: ReprC + RustSpec + 'd, E: ReprC + RustSpec + 'd>
    SoftDecodeRef<'d, L> for crate::result::Result<T, E>
where
    crate::sync::DecodedField<
        'd,
        E,
        <&'d E as DecodeMove<'d, L>>::Store,
        <E as RustSpec>::Layout,
        L,
    >: TakeDecoded<Value = E>,
    crate::sync::DecodedField<
        'd,
        T,
        <&'d T as DecodeMove<'d, L>>::Store,
        <T as RustSpec>::Layout,
        L,
    >: TakeDecoded<Value = T>,
    Self: ReprC<CType = AbiResult<<T as ReprC>::CType, <E as ReprC>::CType>>,
    &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'d T: DecodeMove<'d, L> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    &'d E: DecodeMove<'d, L> + ReprC<CType: RefCarrier<Pointee = <E as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
    <E as ReprC>::CType: Copy,
{
    type Store = RefSizedDecodeStore<
        Self,
        Option<Result<<&'d T as DecodeMove<'d, L>>::Store, <&'d E as DecodeMove<'d, L>>::Store>>,
    >;
    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        let source = unsafe { source.into_ref()? };
        store.decode(|fields| match source.as_result()? {
            Ok(value) => {
                let Ok(field) = fields.insert(Ok(Default::default())) else {
                    unreachable!()
                };
                let value =
                    unsafe { decode_ref_field::<T, L>(RefCarrier::from_raw(value), field) }?;
                Some(Self::Ok(value))
            }
            Err(value) => {
                let Err(field) = fields.insert(Err(Default::default())) else {
                    unreachable!()
                };
                let value =
                    unsafe { decode_ref_field::<E, L>(RefCarrier::from_raw(value), field) }?;
                Some(Self::Err(value))
            }
        })
    }
}

// Build an independently owned Vec, copying stable borrowed elements and
// extracting reconstructed elements from their stores.
#[cfg(feature = "alloc")]
unsafe fn decode_vec_ref<'d, 'itm: 'd, T, L: Language>(
    header: AbiVec<T::CType>,
    fields: &'itm mut Box<[<&'d T as DecodeMove<'d, L>>::Store]>,
) -> Option<Vec<T>>
where
    crate::sync::DecodedField<
        'd,
        T,
        <&'d T as DecodeMove<'d, L>>::Store,
        <T as RustSpec>::Layout,
        L,
    >: TakeDecoded<Value = T>,
    T: ReprC<CType: Sized> + RustSpec + 'd,
    &'d T:
        DecodeMove<'d, L, Store: Sized> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
{
    if header.as_ptr().is_null() || header.len() > header.capacity() {
        return None;
    }
    *fields = core::iter::repeat_with(Default::default)
        .take(header.len())
        .collect();
    let mut values = Vec::with_capacity(header.capacity());
    for (index, field) in fields.iter_mut().enumerate() {
        let carrier = unsafe { RefCarrier::from_raw(header.as_ptr().add(index)) };
        let value = unsafe { decode_ref_field::<T, L>(carrier, field)? };
        values.push(value);
    }
    Some(values)
}

#[cfg(feature = "alloc")]
unsafe fn decode_vec_ref_unchecked<'d, 'itm: 'd, T>(
    header: AbiVec<T::CType>,
    fields: &'itm mut Box<[<&'d T as DecodeMoveUnchecked<'d>>::SoftStore]>,
) -> Vec<T>
where
    crate::sync::DecodedFieldUnchecked<
        'd,
        T,
        <&'d T as DecodeMoveUnchecked<'d>>::SoftStore,
        <T as RustSpec>::Layout,
    >: TakeDecoded<Value = T>,
    T: ReprC<CType: Sized> + RustSpec + 'd,
    &'d T: DecodeMoveUnchecked<'d, SoftStore: Sized>
        + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
{
    *fields = core::iter::repeat_with(Default::default)
        .take(header.len())
        .collect();
    let mut values = Vec::with_capacity(header.capacity());
    for (index, field) in fields.iter_mut().enumerate() {
        let carrier = unsafe { RefCarrier::from_raw(header.as_ptr().add(index)) };
        let value = unsafe { decode_ref_field_unchecked::<T>(carrier, field) };
        values.push(value);
    }
    values
}

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<'d> SoftDecodeRefUnchecked<'d> for alloc::string::String {
    type SoftStore =
        RefSizedDecodeStore<Self, Box<[<&'d u8 as DecodeMoveUnchecked<'d>>::SoftStore]>>;

    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d Self {
        let header = unsafe { *source.into_ref().unwrap_unchecked() };
        store
            .decode(|fields| {
                let bytes = unsafe { decode_vec_ref_unchecked::<_>(header.0.0, fields) };
                // The caller guarantees UTF-8 validity; the cloned vector is
                // independently owned and becomes the stored String.
                Some(unsafe { Self::from_utf8_unchecked(bytes) })
            })
            .expect("unchecked decoding produces a value")
    }
}
#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<'d, L: Language> SoftDecodeRef<'d, L> for alloc::string::String {
    type Store = RefSizedDecodeStore<Self, Box<[<&'d u8 as DecodeMove<'d, L>>::Store]>>;

    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        // String is Copy; only the header is read here.
        let header = unsafe { *source.into_ref()? };
        store.decode(|fields| {
            let bytes = unsafe { decode_vec_ref::<_, L>(header.0.0, fields)? };
            crate::lang::checks::utf8::<L>(&bytes)
                .then(|| unsafe { Self::from_utf8_unchecked(bytes) })
        })
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d, T: ReprC + RustSpec + 'd> SoftDecodeRefUnchecked<'d> for alloc::vec::Vec<T>
where
    crate::sync::DecodedFieldUnchecked<
        'd,
        T,
        <&'d T as DecodeMoveUnchecked<'d>>::SoftStore,
        <T as RustSpec>::Layout,
    >: TakeDecoded<Value = T>,
    Self: ReprC<CType = Native<AbiVec<<T as ReprC>::CType>>>,
    &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'d T: DecodeMoveUnchecked<'d> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
{
    type SoftStore =
        RefSizedDecodeStore<Self, Box<[<&'d T as DecodeMoveUnchecked<'d>>::SoftStore]>>;

    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d Self {
        let header = unsafe { *source.into_ref().unwrap_unchecked() };
        store
            .decode(|fields| Some(unsafe { decode_vec_ref_unchecked::<_>(header.0, fields) }))
            .expect("unchecked decoding produces a value")
    }
}
#[cfg(feature = "alloc")]
unsafe impl<'d, L: Language, T: ReprC + RustSpec + 'd> SoftDecodeRef<'d, L> for alloc::vec::Vec<T>
where
    crate::sync::DecodedField<
        'd,
        T,
        <&'d T as DecodeMove<'d, L>>::Store,
        <T as RustSpec>::Layout,
        L,
    >: TakeDecoded<Value = T>,
    Self: ReprC<CType = Native<AbiVec<<T as ReprC>::CType>>>,
    &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'d T: DecodeMove<'d, L> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
{
    type Store = RefSizedDecodeStore<Self, Box<[<&'d T as DecodeMove<'d, L>>::Store]>>;

    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        // AbiVec is Copy; only the header is read here.
        let header = unsafe { *source.into_ref()? };
        store.decode(|fields| unsafe { decode_vec_ref::<_, L>(header.0, fields) })
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d, T: ReprC + RustSpec + 'd> SoftDecodeRefUnchecked<'d> for crate::vec::Vec<T>
where
    crate::sync::DecodedFieldUnchecked<
        'd,
        T,
        <&'d T as DecodeMoveUnchecked<'d>>::SoftStore,
        <T as RustSpec>::Layout,
    >: TakeDecoded<Value = T>,
    Self: ReprC<CType = AbiVec<<T as ReprC>::CType>>,
    &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'d T: DecodeMoveUnchecked<'d> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
{
    type SoftStore =
        RefSizedDecodeStore<Self, Box<[<&'d T as DecodeMoveUnchecked<'d>>::SoftStore]>>;

    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d Self {
        let header = unsafe { *source.into_ref().unwrap_unchecked() };
        store
            .decode(|fields| Some(unsafe { decode_vec_ref_unchecked::<_>(header, fields) }.into()))
            .expect("unchecked decoding produces a value")
    }
}
#[cfg(feature = "alloc")]
unsafe impl<'d, L: Language, T: ReprC + RustSpec + 'd> SoftDecodeRef<'d, L> for crate::vec::Vec<T>
where
    crate::sync::DecodedField<
        'd,
        T,
        <&'d T as DecodeMove<'d, L>>::Store,
        <T as RustSpec>::Layout,
        L,
    >: TakeDecoded<Value = T>,
    Self: ReprC<CType = AbiVec<<T as ReprC>::CType>>,
    &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'d T: DecodeMove<'d, L> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
{
    type Store = RefSizedDecodeStore<Self, Box<[<&'d T as DecodeMove<'d, L>>::Store]>>;

    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        let header = unsafe { *source.into_ref()? };
        store.decode(|fields| unsafe { decode_vec_ref::<_, L>(header, fields).map(Into::into) })
    }
}

unsafe impl<'d, T: ReprC<CType: CType + Sized + 'd> + RustSpec + 'd, const N: usize>
    SoftDecodeRefUnchecked<'d> for [T; N]
where
    crate::sync::DecodedFieldUnchecked<
        'd,
        T,
        <&'d T as DecodeMoveUnchecked<'d>>::SoftStore,
        <T as RustSpec>::Layout,
    >: TakeDecoded<Value = T>,
    Self: ReprC<CType = [<T as ReprC>::CType; N]>,
    &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'d T: DecodeMoveUnchecked<'d> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
{
    type SoftStore =
        RefSizedDecodeStore<Self, ArrayStore<<&'d T as DecodeMoveUnchecked<'d>>::SoftStore, N>>;

    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d Self {
        let source: &'d [<T as ReprC>::CType; N] = unsafe { source.into_ref().unwrap_unchecked() };
        store
            .decode(|fields| {
                let mut fields = fields.0.iter_mut();
                Some(core::array::from_fn(|i| {
                    let carrier = unsafe { RefCarrier::from_raw(&source[i]) };
                    unsafe { decode_ref_field_unchecked::<T>(carrier, fields.next().unwrap()) }
                }))
            })
            .expect("unchecked decoding produces a value")
    }
}
unsafe impl<'d, L: Language, T: ReprC<CType: CType + Sized + 'd> + RustSpec + 'd, const N: usize>
    SoftDecodeRef<'d, L> for [T; N]
where
    crate::sync::DecodedField<
        'd,
        T,
        <&'d T as DecodeMove<'d, L>>::Store,
        <T as RustSpec>::Layout,
        L,
    >: TakeDecoded<Value = T>,
    Self: ReprC<CType = [<T as ReprC>::CType; N]>,
    &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'d T: DecodeMove<'d, L> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
{
    type Store = RefSizedDecodeStore<Self, ArrayStore<<&'d T as DecodeMove<'d, L>>::Store, N>>;

    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        let source: &'d [<T as ReprC>::CType; N] = unsafe { source.into_ref()? };
        store.decode(|fields| {
            let mut fields = fields.0.iter_mut();
            let values: [Option<T>; N] = core::array::from_fn(|i| {
                let carrier = unsafe { RefCarrier::from_raw(&source[i]) };
                unsafe { decode_ref_field::<T, L>(carrier, fields.next().unwrap()) }
            });
            if values.iter().any(Option::is_none) {
                return None;
            }
            Some(values.map(Option::unwrap))
        })
    }
}

unsafe impl<'d, T: ReprC + 'd> SoftDecodeRefUnchecked<'d> for core::mem::ManuallyDrop<T>
where
    Self: ReprC,
    &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'d T: DecodeMoveUnchecked<'d> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
    <Self as ReprC>::CType: Sized,
{
    type SoftStore = <&'d T as DecodeMoveUnchecked<'d>>::SoftStore;

    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d Self {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { RefCarrier::from_raw(source.as_ptr().cast::<T::CType>()) };
        let inner =
            unsafe { <&'d T as DecodeMoveUnchecked<'d>>::soft_decode_unchecked(source, store) };
        unsafe { &*core::ptr::from_ref(inner).cast::<Self>() }
    }
}
unsafe impl<'d, L: Language, T: ReprC + 'd> SoftDecodeRef<'d, L> for core::mem::ManuallyDrop<T>
where
    Self: ReprC,
    &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'d T: DecodeMove<'d, L> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
    <Self as ReprC>::CType: Sized,
{
    type Store = <&'d T as DecodeMove<'d, L>>::Store;
    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { RefCarrier::from_raw(source.as_ptr().cast::<T::CType>()) };
        let inner = unsafe { <&'d T as DecodeMove<'d, L>>::soft_decode(source, store)? };
        // Both Rust and C wrappers are transparent; this does not read the value.
        Some(unsafe { &*core::ptr::from_ref(inner).cast::<Self>() })
    }
}

unsafe impl<'d, 'a: 'd, T: ?Sized + 'a> SoftDecodeRefUnchecked<'d> for &'a T
where
    Self: DecodeMoveUnchecked<'d, CType: Copy>,
    &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
{
    type SoftStore = RefSizedDecodeStore<Self, <Self as DecodeMoveUnchecked<'d>>::SoftStore>;

    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d Self {
        let carrier = unsafe { *source.into_ref().unwrap_unchecked() };
        store
            .decode(|field| {
                Some(unsafe {
                    <Self as crate::convert::DecodeMoveUnchecked<'d>>::soft_decode_unchecked(
                        carrier, field,
                    )
                })
            })
            .expect("unchecked decoding produces a value")
    }
}
unsafe impl<'d, 'a: 'd, L: Language, T: ?Sized + 'a> SoftDecodeRef<'d, L> for &'a T
where
    Self: DecodeMove<'d, L, CType: Copy>,
    &'d Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
{
    type Store = RefSizedDecodeStore<Self, <Self as DecodeMove<'d, L>>::Store>;
    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        let carrier = unsafe { *source.into_ref()? };
        store.decode(|field| unsafe {
            <Self as crate::convert::DecodeMove<'d, L>>::soft_decode(carrier, field)
        })
    }
}

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<'d, R: ReprC + EncodeMoveUnchecked<SoftStore: EmptyStore> + 'd>
    SoftDecodeMutUnchecked<'d> for [R]
where
    R: crate::borrow::BorrowedDecodeUnchecked<'d>,
    &'d mut Self: SoftDecodeMutUnchecked<'d>
        + ReprC<CType = crate::wide::abi::SoftWideRefMut<<Self as ReprC>::CType>>,
    [R]: RustSpec<Size = MetaSized<SliceLike>>,
    <R as ReprC>::CType: Copy,
{
    type SoftStore = RefMutSliceDecodeStoreUnchecked<
        R,
        Box<[<R as crate::borrow::BorrowedDecodeUnchecked<'d>>::SoftStore]>,
    >;

    #[inline(always)]
    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut <Self as SoftDecodeMutUnchecked<'d>>::SoftStore,
    ) -> &'d mut Self {
        unsafe {
            store.decode(source, |values, fields| {
                *fields = core::iter::repeat_with(Default::default)
                    .take(values.len())
                    .collect();
                Some(
                    values
                        .iter()
                        .copied()
                        .zip(fields.iter_mut())
                        .map(|(value, field)| decode_mut_field_unchecked::<R>(value, field))
                        .collect(),
                )
            })
        }
        .expect("unchecked decoding produces a value")
    }
}
#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<'d, L: Language, R: ReprC + EncodeMove<Store: EmptyStore> + 'd> SoftDecodeMut<'d, L>
    for [R]
where
    R: crate::borrow::BorrowedDecode<'d, L>,
    &'d mut Self: ReprC<CType = crate::wide::abi::SoftWideRefMut<<Self as ReprC>::CType>>,
    [R]: RustSpec<Size = MetaSized<SliceLike>>,
    <R as ReprC>::CType: Copy,
{
    type Store =
        RefMutSliceDecodeStore<R, Box<[<R as crate::borrow::BorrowedDecode<'d, L>>::Store]>>;

    #[inline(always)]
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut <Self as SoftDecodeMut<'d, L>>::Store,
    ) -> Option<&'d mut Self> {
        unsafe {
            store.decode(source, |values, fields| {
                *fields = core::iter::repeat_with(Default::default)
                    .take(values.len())
                    .collect();
                values
                    .iter()
                    .copied()
                    .zip(fields.iter_mut())
                    .map(|(value, field)| decode_mut_field::<R, L>(value, field))
                    .collect()
            })
        }
    }
}

unsafe impl<'d, T: ReprC + 'd> SoftDecodeMutUnchecked<'d> for crate::option::Option<T>
where
    T: crate::borrow::BorrowedDecodeUnchecked<'d>,
    Self: EncodeMoveUnchecked<SoftStore: EmptyStore>
        + RustSpec
        + ReprC<CType = AbiOption<<T as ReprC>::CType>>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
{
    type SoftStore = RefMutSizedDecodeStoreUnchecked<
        Self,
        Option<<T as crate::borrow::BorrowedDecodeUnchecked<'d>>::SoftStore>,
    >;

    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d mut Self {
        let pointer = source.as_ptr();
        let source = unsafe { &*pointer };
        unsafe {
            store.decode(pointer, |fields| match source.as_option()? {
                None => {
                    *fields = None;
                    Some(Self::None)
                }
                Some(value) => {
                    let field = fields.insert(Default::default());
                    let value = decode_mut_field_unchecked::<T>(*value, field);
                    Some(Self::Some(value))
                }
            })
        }
        .expect("unchecked decoding produces a value")
    }
}
unsafe impl<'d, L: Language, T: ReprC + 'd> SoftDecodeMut<'d, L> for crate::option::Option<T>
where
    T: crate::borrow::BorrowedDecode<'d, L>,
    Self: EncodeMove<Store: EmptyStore> + RustSpec + ReprC<CType = AbiOption<<T as ReprC>::CType>>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
{
    type Store =
        RefMutSizedDecodeStore<Self, Option<<T as crate::borrow::BorrowedDecode<'d, L>>::Store>>;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        let pointer = source.as_ptr();
        let source = unsafe { pointer.as_ref()? };
        unsafe {
            store.decode(pointer, |fields| match source.as_option()? {
                None => {
                    *fields = None;
                    Some(Self::None)
                }
                Some(value) => {
                    let field = fields.insert(Default::default());
                    let value = decode_mut_field::<T, L>(*value, field)?;
                    Some(Self::Some(value))
                }
            })
        }
    }
}

unsafe impl<'d, T: ReprC + 'd, E: ReprC + 'd> SoftDecodeMutUnchecked<'d>
    for crate::result::Result<T, E>
where
    T: crate::borrow::BorrowedDecodeUnchecked<'d>,
    E: crate::borrow::BorrowedDecodeUnchecked<'d>,
    Self: EncodeMoveUnchecked<SoftStore: EmptyStore>
        + RustSpec
        + ReprC<CType = AbiResult<<T as ReprC>::CType, <E as ReprC>::CType>>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
    <E as ReprC>::CType: Copy,
{
    type SoftStore = RefMutSizedDecodeStoreUnchecked<
        Self,
        Option<
            Result<
                <T as crate::borrow::BorrowedDecodeUnchecked<'d>>::SoftStore,
                <E as crate::borrow::BorrowedDecodeUnchecked<'d>>::SoftStore,
            >,
        >,
    >;

    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d mut Self {
        let pointer = source.as_ptr();
        let source = unsafe { &*pointer };
        unsafe {
            store.decode(pointer, |fields| match source.as_result()? {
                Ok(value) => {
                    let Ok(field) = fields.insert(Ok(Default::default())) else {
                        unreachable!()
                    };
                    let value = decode_mut_field_unchecked::<T>(*value, field);
                    Some(Self::Ok(value))
                }
                Err(value) => {
                    let Err(field) = fields.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    let value = decode_mut_field_unchecked::<E>(*value, field);
                    Some(Self::Err(value))
                }
            })
        }
        .expect("unchecked decoding produces a value")
    }
}
unsafe impl<'d, L: Language, T: ReprC + 'd, E: ReprC + 'd> SoftDecodeMut<'d, L>
    for crate::result::Result<T, E>
where
    T: crate::borrow::BorrowedDecode<'d, L>,
    E: crate::borrow::BorrowedDecode<'d, L>,
    Self: EncodeMove<Store: EmptyStore>
        + RustSpec
        + ReprC<CType = AbiResult<<T as ReprC>::CType, <E as ReprC>::CType>>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
    <E as ReprC>::CType: Copy,
{
    type Store = RefMutSizedDecodeStore<
        Self,
        Option<
            Result<
                <T as crate::borrow::BorrowedDecode<'d, L>>::Store,
                <E as crate::borrow::BorrowedDecode<'d, L>>::Store,
            >,
        >,
    >;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        let pointer = source.as_ptr();
        let source = unsafe { pointer.as_ref()? };
        unsafe {
            store.decode(pointer, |fields| match source.as_result()? {
                Ok(value) => {
                    let Ok(field) = fields.insert(Ok(Default::default())) else {
                        unreachable!()
                    };
                    let value = decode_mut_field::<T, L>(*value, field)?;
                    Some(Self::Ok(value))
                }
                Err(value) => {
                    let Err(field) = fields.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    let value = decode_mut_field::<E, L>(*value, field)?;
                    Some(Self::Err(value))
                }
            })
        }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d, T: ReprC + Clone + 'd> SoftDecodeMutUnchecked<'d> for Box<T>
where
    Self: RustSpec + ReprC<CType = AbiBox<<T as ReprC>::CType>>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    &'d mut T: DecodeMoveUnchecked<'d> + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
{
    type SoftStore = RefMutSizedDecodeStoreUnchecked<
        Self,
        Box<<&'d mut T as DecodeMoveUnchecked<'d>>::SoftStore>,
        *mut T,
    >;

    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d mut Self {
        let header = unsafe { *source.as_ptr().as_ref().unwrap_unchecked() };
        unsafe {
            store.decode_with(
                |fields| {
                    let carrier = <&'d mut T as ReprC>::CType::from_raw(header.as_ptr());
                    let value = <&'d mut T as DecodeMoveUnchecked<'d>>::soft_decode_unchecked(
                        carrier,
                        &mut **fields,
                    );
                    Some((Box::new(T::clone(value)), core::ptr::from_mut(value)))
                },
                |value, destination| {
                    *destination = *value;
                    Some(())
                },
            )
        }
        .expect("unchecked decoding produces a value")
    }
}
#[cfg(feature = "alloc")]
unsafe impl<'d, L: Language, T: ReprC + Clone + 'd> SoftDecodeMut<'d, L> for Box<T>
where
    Self: RustSpec + ReprC<CType = AbiBox<<T as ReprC>::CType>>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    &'d mut T: DecodeMove<'d, L> + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
{
    type Store = RefMutSizedDecodeStore<Self, Box<<&'d mut T as DecodeMove<'d, L>>::Store>, *mut T>;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        let header = unsafe { *source.as_ptr().as_ref()? };
        unsafe {
            store.decode_with(
                |fields| {
                    let carrier = <&'d mut T as ReprC>::CType::from_raw(header.as_ptr());
                    let value =
                        <&'d mut T as DecodeMove<'d, L>>::soft_decode(carrier, &mut **fields)?;
                    Some((Box::new(T::clone(value)), core::ptr::from_mut(value)))
                },
                |value, destination| {
                    *destination = *value;
                    Some(())
                },
            )
        }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d, T: ReprC + Clone + 'd> SoftDecodeMutUnchecked<'d> for Box<[T]>
where
    Self: RustSpec + ReprC<CType = AbiWideBox<<[T] as ReprC>::CType>>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    &'d mut T: DecodeMoveUnchecked<'d> + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
{
    type SoftStore = RefMutSizedDecodeStoreUnchecked<
        Self,
        Box<[<&'d mut T as DecodeMoveUnchecked<'d>>::SoftStore]>,
        Box<[*mut T]>,
    >;

    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d mut Self {
        let header = unsafe { *source.as_ptr().as_ref().unwrap_unchecked() };
        unsafe {
            store.decode_with(
                |fields| {
                    if header.as_ptr().is_null() {
                        return None;
                    }
                    *fields = core::iter::repeat_with(Default::default)
                        .take(header.metadata())
                        .collect();
                    let mut values = Vec::with_capacity(header.metadata());
                    let mut destinations = Vec::with_capacity(header.metadata());
                    for (i, field) in fields.iter_mut().enumerate() {
                        let carrier = <&'d mut T as ReprC>::CType::from_raw(header.as_ptr().add(i));
                        let value = <&'d mut T as DecodeMoveUnchecked<'d>>::soft_decode_unchecked(
                            carrier, field,
                        );
                        values.push(T::clone(value));
                        destinations.push(core::ptr::from_mut(value));
                    }
                    Some((values.into_boxed_slice(), destinations.into_boxed_slice()))
                },
                |values, destinations| {
                    if values.len() != destinations.len() {
                        return None;
                    }
                    for (value, destination) in
                        values.into_vec().into_iter().zip(destinations.iter())
                    {
                        **destination = value;
                    }
                    Some(())
                },
            )
        }
        .expect("unchecked decoding produces a value")
    }
}
#[cfg(feature = "alloc")]
unsafe impl<'d, L: Language, T: ReprC + Clone + 'd> SoftDecodeMut<'d, L> for Box<[T]>
where
    Self: RustSpec + ReprC<CType = AbiWideBox<<[T] as ReprC>::CType>>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    &'d mut T: DecodeMove<'d, L> + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
{
    type Store =
        RefMutSizedDecodeStore<Self, Box<[<&'d mut T as DecodeMove<'d, L>>::Store]>, Box<[*mut T]>>;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        let header = unsafe { *source.as_ptr().as_ref()? };
        unsafe {
            store.decode_with(
                |fields| {
                    if header.as_ptr().is_null() {
                        return None;
                    }
                    *fields = core::iter::repeat_with(Default::default)
                        .take(header.metadata())
                        .collect();
                    let mut values = Vec::with_capacity(header.metadata());
                    let mut destinations = Vec::with_capacity(header.metadata());
                    for (i, field) in fields.iter_mut().enumerate() {
                        let carrier = <&'d mut T as ReprC>::CType::from_raw(header.as_ptr().add(i));
                        let value = <&'d mut T as DecodeMove<'d, L>>::soft_decode(carrier, field)?;
                        values.push(T::clone(value));
                        destinations.push(core::ptr::from_mut(value));
                    }
                    Some((values.into_boxed_slice(), destinations.into_boxed_slice()))
                },
                |values, destinations| {
                    if values.len() != destinations.len() {
                        return None;
                    }
                    for (value, destination) in
                        values.into_vec().into_iter().zip(destinations.iter())
                    {
                        **destination = value;
                    }
                    Some(())
                },
            )
        }
    }
}

unsafe impl<'d, T: ReprC + 'd, const N: usize> SoftDecodeMutUnchecked<'d> for [T; N]
where
    T: crate::borrow::BorrowedDecodeUnchecked<'d>,
    Self: RustSpec
        + ReprC<CType = [<T as ReprC>::CType; N]>
        + EncodeMoveUnchecked<SoftStore: EmptyStore>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
{
    type SoftStore = RefMutSizedDecodeStoreUnchecked<
        Self,
        ArrayStore<<T as crate::borrow::BorrowedDecodeUnchecked<'d>>::SoftStore, N>,
    >;

    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d mut Self {
        let source = source.as_ptr();
        unsafe {
            store.decode(source, |fields| {
                let values = source.read();
                let mut fields = fields.0.iter_mut();
                Some(
                    values.map(|value| {
                        decode_mut_field_unchecked::<T>(value, fields.next().unwrap())
                    }),
                )
            })
        }
        .expect("unchecked decoding produces a value")
    }
}
unsafe impl<'d, L: Language, T: ReprC + 'd, const N: usize> SoftDecodeMut<'d, L> for [T; N]
where
    T: crate::borrow::BorrowedDecode<'d, L>,
    Self: RustSpec + ReprC<CType = [<T as ReprC>::CType; N]> + EncodeMove<Store: EmptyStore>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
{
    type Store = RefMutSizedDecodeStore<
        Self,
        ArrayStore<<T as crate::borrow::BorrowedDecode<'d, L>>::Store, N>,
    >;

    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        let source = source.as_ptr();
        unsafe {
            store.decode(source, |fields| {
                // The source carrier is Copy; the Rust element need not be.
                let values = source.read();
                let mut fields = fields.0.iter_mut();
                let values =
                    values.map(|value| decode_mut_field::<T, L>(value, fields.next().unwrap()));
                if values.iter().any(Option::is_none) {
                    return None;
                }
                Some(values.map(Option::unwrap))
            })
        }
    }
}

unsafe impl<'d, T: ReprC + 'd> SoftDecodeMutUnchecked<'d> for core::mem::ManuallyDrop<T>
where
    Self: RustSpec + ReprC,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    &'d mut T: DecodeMoveUnchecked<'d> + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
    <Self as ReprC>::CType: Sized,
{
    type SoftStore = <&'d mut T as DecodeMoveUnchecked<'d>>::SoftStore;

    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d mut Self {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { <&'d mut T as ReprC>::CType::from_raw(source.as_ptr().cast()) };
        let inner =
            unsafe { <&'d mut T as DecodeMoveUnchecked<'d>>::soft_decode_unchecked(source, store) };
        unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() }
    }
}
unsafe impl<'d, L: Language, T: ReprC + 'd> SoftDecodeMut<'d, L> for core::mem::ManuallyDrop<T>
where
    Self: RustSpec + ReprC,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    &'d mut T: DecodeMove<'d, L> + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
    <Self as ReprC>::CType: Sized,
{
    type Store = <&'d mut T as DecodeMove<'d, L>>::Store;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { <&'d mut T as ReprC>::CType::from_raw(source.as_ptr().cast()) };
        let inner = unsafe { <&'d mut T as DecodeMove<'d, L>>::soft_decode(source, store)? };
        // The exclusive borrow permits delegation into a cell; no shared cell is read.
        Some(unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() })
    }
}

unsafe impl<'d, T: ReprC + 'd> SoftDecodeMutUnchecked<'d> for core::cell::Cell<T>
where
    Self: RustSpec + ReprC,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    &'d mut T: DecodeMoveUnchecked<'d> + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
    <Self as ReprC>::CType: Sized,
{
    type SoftStore = <&'d mut T as DecodeMoveUnchecked<'d>>::SoftStore;

    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d mut Self {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { <&'d mut T as ReprC>::CType::from_raw(source.as_ptr().cast()) };
        let inner =
            unsafe { <&'d mut T as DecodeMoveUnchecked<'d>>::soft_decode_unchecked(source, store) };
        unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() }
    }
}
unsafe impl<'d, L: Language, T: ReprC + 'd> SoftDecodeMut<'d, L> for core::cell::Cell<T>
where
    Self: RustSpec + ReprC,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    &'d mut T: DecodeMove<'d, L> + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
    <Self as ReprC>::CType: Sized,
{
    type Store = <&'d mut T as DecodeMove<'d, L>>::Store;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { <&'d mut T as ReprC>::CType::from_raw(source.as_ptr().cast()) };
        let inner = unsafe { <&'d mut T as DecodeMove<'d, L>>::soft_decode(source, store)? };
        // The exclusive borrow permits delegation into a cell; no shared cell is read.
        Some(unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() })
    }
}

unsafe impl<'d, T: ReprC + 'd> SoftDecodeMutUnchecked<'d> for core::cell::UnsafeCell<T>
where
    Self: RustSpec + ReprC,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    &'d mut T: DecodeMoveUnchecked<'d> + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
    <Self as ReprC>::CType: Sized,
{
    type SoftStore = <&'d mut T as DecodeMoveUnchecked<'d>>::SoftStore;

    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d mut Self {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { <&'d mut T as ReprC>::CType::from_raw(source.as_ptr().cast()) };
        let inner =
            unsafe { <&'d mut T as DecodeMoveUnchecked<'d>>::soft_decode_unchecked(source, store) };
        unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() }
    }
}
unsafe impl<'d, L: Language, T: ReprC + 'd> SoftDecodeMut<'d, L> for core::cell::UnsafeCell<T>
where
    Self: RustSpec + ReprC,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    &'d mut T: DecodeMove<'d, L> + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
    <Self as ReprC>::CType: Sized,
{
    type Store = <&'d mut T as DecodeMove<'d, L>>::Store;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { <&'d mut T as ReprC>::CType::from_raw(source.as_ptr().cast()) };
        let inner = unsafe { <&'d mut T as DecodeMove<'d, L>>::soft_decode(source, store)? };
        // The exclusive borrow permits delegation into a cell; no shared cell is read.
        Some(unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() })
    }
}

unsafe impl<'d, 'a: 'd, T: ?Sized + 'a> SoftDecodeMutUnchecked<'d> for &'a T
where
    Self: RustSpec
        + DecodeMoveUnchecked<'d, CType: Copy>
        + EncodeMoveUnchecked<SoftStore: EmptyStore>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
{
    type SoftStore =
        RefMutSizedDecodeStoreUnchecked<Self, <Self as DecodeMoveUnchecked<'d>>::SoftStore>;

    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d mut Self {
        let source = source.as_ptr();
        unsafe {
            store.decode(source, |field| {
                Some(
                    <Self as crate::convert::DecodeMoveUnchecked<'d>>::soft_decode_unchecked(
                        source.read(),
                        field,
                    ),
                )
            })
        }
        .expect("unchecked decoding produces a value")
    }
}
unsafe impl<'d, 'a: 'd, L: Language, T: ?Sized + 'a> SoftDecodeMut<'d, L> for &'a T
where
    Self: RustSpec + DecodeMove<'d, L, CType: Copy> + EncodeMove<Store: EmptyStore>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
{
    type Store = RefMutSizedDecodeStore<Self, <Self as DecodeMove<'d, L>>::Store>;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        let source = source.as_ptr();
        unsafe {
            store.decode(source, |field| {
                <Self as crate::convert::DecodeMove<'d, L>>::soft_decode(source.read(), field)
            })
        }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d, T: ReprC + Clone + 'd> SoftDecodeMutUnchecked<'d> for alloc::vec::Vec<T>
where
    Self: RustSpec + ReprC<CType = Native<AbiVec<<T as ReprC>::CType>>>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    &'d mut T: DecodeMoveUnchecked<'d> + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
{
    type SoftStore = RefMutSizedDecodeStoreUnchecked<
        Self,
        Box<[<&'d mut T as DecodeMoveUnchecked<'d>>::SoftStore]>,
        (Box<[*mut T]>, usize),
    >;

    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d mut Self {
        let header = unsafe { *source.as_ptr().as_ref().unwrap_unchecked() };
        unsafe {
            store.decode_with(
                |fields| {
                    *fields = core::iter::repeat_with(Default::default)
                        .take(header.len())
                        .collect();
                    let mut values = Vec::with_capacity(header.capacity());
                    let mut destinations = Vec::with_capacity(header.len());
                    for (i, field) in fields.iter_mut().enumerate() {
                        let carrier = <&'d mut T as ReprC>::CType::from_raw(header.as_ptr().add(i));
                        let value = <&'d mut T as DecodeMoveUnchecked<'d>>::soft_decode_unchecked(
                            carrier, field,
                        );
                        values.push(T::clone(value));
                        destinations.push(core::ptr::from_mut(value));
                    }
                    let capacity = values.capacity();
                    Some((values, (destinations.into_boxed_slice(), capacity)))
                },
                |values, (destinations, capacity)| {
                    // A borrowed vector cannot replace the foreign allocation or its shape.
                    if values.len() != destinations.len() || values.capacity() != capacity {
                        return None;
                    }
                    for (value, destination) in values.into_iter().zip(destinations.iter()) {
                        **destination = value;
                    }
                    Some(())
                },
            )
        }
        .expect("unchecked decoding produces a value")
    }
}
#[cfg(feature = "alloc")]
unsafe impl<'d, L: Language, T: ReprC + Clone + 'd> SoftDecodeMut<'d, L> for alloc::vec::Vec<T>
where
    Self: RustSpec + ReprC<CType = Native<AbiVec<<T as ReprC>::CType>>>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    &'d mut T: DecodeMove<'d, L> + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
{
    type Store = RefMutSizedDecodeStore<
        Self,
        Box<[<&'d mut T as DecodeMove<'d, L>>::Store]>,
        (Box<[*mut T]>, usize),
    >;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        let header = unsafe { *source.as_ptr().as_ref()? };
        if header.as_ptr().is_null() || header.len() > header.capacity() {
            return None;
        }
        unsafe {
            store.decode_with(
                |fields| {
                    *fields = core::iter::repeat_with(Default::default)
                        .take(header.len())
                        .collect();
                    let mut values = Vec::with_capacity(header.capacity());
                    let mut destinations = Vec::with_capacity(header.len());
                    for (i, field) in fields.iter_mut().enumerate() {
                        let carrier = <&'d mut T as ReprC>::CType::from_raw(header.as_ptr().add(i));
                        let value = <&'d mut T as DecodeMove<'d, L>>::soft_decode(carrier, field)?;
                        values.push(T::clone(value));
                        destinations.push(core::ptr::from_mut(value));
                    }
                    let capacity = values.capacity();
                    Some((values, (destinations.into_boxed_slice(), capacity)))
                },
                |values, (destinations, capacity)| {
                    // A borrowed vector cannot replace the foreign allocation or its shape.
                    if values.len() != destinations.len() || values.capacity() != capacity {
                        return None;
                    }
                    for (value, destination) in values.into_iter().zip(destinations.iter()) {
                        **destination = value;
                    }
                    Some(())
                },
            )
        }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d, T: ReprC + Clone + 'd> SoftDecodeMutUnchecked<'d> for crate::vec::Vec<T>
where
    Self: RustSpec + ReprC<CType = AbiVec<<T as ReprC>::CType>>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    &'d mut T: DecodeMoveUnchecked<'d> + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
{
    type SoftStore = RefMutSizedDecodeStoreUnchecked<
        Self,
        Box<[<&'d mut T as DecodeMoveUnchecked<'d>>::SoftStore]>,
        (Box<[*mut T]>, usize),
    >;

    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d mut Self {
        let header = unsafe { *source.as_ptr().as_ref().unwrap_unchecked() };
        unsafe {
            store.decode_with(
                |fields| {
                    *fields = core::iter::repeat_with(Default::default)
                        .take(header.len())
                        .collect();
                    let mut values = Vec::with_capacity(header.capacity());
                    let mut destinations = Vec::with_capacity(header.len());
                    for (i, field) in fields.iter_mut().enumerate() {
                        let carrier = <&'d mut T as ReprC>::CType::from_raw(header.as_ptr().add(i));
                        let value = <&'d mut T as DecodeMoveUnchecked<'d>>::soft_decode_unchecked(
                            carrier, field,
                        );
                        values.push(T::clone(value));
                        destinations.push(core::ptr::from_mut(value));
                    }
                    let capacity = values.capacity();
                    Some((values.into(), (destinations.into_boxed_slice(), capacity)))
                },
                |values, (destinations, capacity)| {
                    // A borrowed vector cannot replace the foreign allocation or its shape.
                    if values.len() != destinations.len() || values.capacity() != capacity {
                        return None;
                    }
                    let values: Vec<T> = values.into();
                    for (value, destination) in values.into_iter().zip(destinations.iter()) {
                        **destination = value;
                    }
                    Some(())
                },
            )
        }
        .expect("unchecked decoding produces a value")
    }
}
#[cfg(feature = "alloc")]
unsafe impl<'d, L: Language, T: ReprC + Clone + 'd> SoftDecodeMut<'d, L> for crate::vec::Vec<T>
where
    Self: RustSpec + ReprC<CType = AbiVec<<T as ReprC>::CType>>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    &'d mut T: DecodeMove<'d, L> + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
{
    type Store = RefMutSizedDecodeStore<
        Self,
        Box<[<&'d mut T as DecodeMove<'d, L>>::Store]>,
        (Box<[*mut T]>, usize),
    >;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        let header = unsafe { *source.as_ptr().as_ref()? };
        if header.as_ptr().is_null() || header.len() > header.capacity() {
            return None;
        }
        unsafe {
            store.decode_with(
                |fields| {
                    *fields = core::iter::repeat_with(Default::default)
                        .take(header.len())
                        .collect();
                    let mut values = Vec::with_capacity(header.capacity());
                    let mut destinations = Vec::with_capacity(header.len());
                    for (i, field) in fields.iter_mut().enumerate() {
                        let carrier = <&'d mut T as ReprC>::CType::from_raw(header.as_ptr().add(i));
                        let value = <&'d mut T as DecodeMove<'d, L>>::soft_decode(carrier, field)?;
                        values.push(T::clone(value));
                        destinations.push(core::ptr::from_mut(value));
                    }
                    let capacity = values.capacity();
                    Some((values.into(), (destinations.into_boxed_slice(), capacity)))
                },
                |values, (destinations, capacity)| {
                    // A borrowed vector cannot replace the foreign allocation or its shape.
                    if values.len() != destinations.len() || values.capacity() != capacity {
                        return None;
                    }
                    let values: Vec<T> = values.into();
                    for (value, destination) in values.into_iter().zip(destinations.iter()) {
                        **destination = value;
                    }
                    Some(())
                },
            )
        }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d, 'a: 'd, T: ReprC + 'a> SoftDecodeMutUnchecked<'d> for &'a mut T
where
    Self: RustSpec + DecodeMoveUnchecked<'d, CType = SoftRefMut<<T as ReprC>::CType>>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
{
    type SoftStore = RefMutSizedDecodeStoreUnchecked<
        Self,
        Box<<Self as DecodeMoveUnchecked<'d>>::SoftStore>,
        *mut T,
    >;

    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::SoftStore,
    ) -> &'d mut Self {
        let carrier = unsafe { *source.as_ptr() };
        unsafe {
            store.decode_with(
                |field| {
                    let value =
                        <Self as crate::convert::DecodeMoveUnchecked<'d>>::soft_decode_unchecked(
                            carrier,
                            &mut **field,
                        );
                    let pointer = core::ptr::from_mut(&mut *value);
                    Some((value, pointer))
                },
                |value, pointer| core::ptr::eq(core::ptr::from_mut(value), pointer).then_some(()),
            )
        }
        .expect("unchecked decoding produces a value")
    }
}
#[cfg(feature = "alloc")]
unsafe impl<'d, 'a: 'd, L: Language, T: ReprC + 'a> SoftDecodeMut<'d, L> for &'a mut T
where
    Self: RustSpec + DecodeMove<'d, L, CType = SoftRefMut<<T as ReprC>::CType>>,
    &'d mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
    <T as ReprC>::CType: Sized,
{
    type Store = RefMutSizedDecodeStore<Self, Box<<Self as DecodeMove<'d, L>>::Store>, *mut T>;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        let carrier = unsafe { *source.as_ptr().as_ref()? };
        unsafe {
            store.decode_with(
                |field| {
                    let value = <Self as crate::convert::DecodeMove<'d, L>>::soft_decode(
                        carrier,
                        &mut **field,
                    )?;
                    let pointer = core::ptr::from_mut(&mut *value);
                    Some((value, pointer))
                },
                |value, pointer| core::ptr::eq(core::ptr::from_mut(value), pointer).then_some(()),
            )
        }
    }
}

// Decode the transparent borrowed carrier, never an owning copy of a field.
pub(crate) unsafe fn decode_mut_field<'d, 'itm: 'd, T, L: Language>(
    source: <T as ReprC>::CType,
    store: &'itm mut <T as crate::borrow::BorrowedDecode<'d, L>>::Store,
) -> Option<T>
where
    T: ReprC<CType: Sized>,
    T: crate::borrow::BorrowedDecode<'d, L>,
{
    unsafe { <T as crate::borrow::BorrowedDecode<'d, L>>::soft_decode(source, store) }
}

pub(crate) unsafe fn decode_mut_field_unchecked<'d, 'itm: 'd, T>(
    source: <T as ReprC>::CType,
    store: &'itm mut <T as crate::borrow::BorrowedDecodeUnchecked<'d>>::SoftStore,
) -> T
where
    T: ReprC<CType: Sized>,
    T: crate::borrow::BorrowedDecodeUnchecked<'d>,
{
    unsafe {
        <T as crate::borrow::BorrowedDecodeUnchecked<'d>>::soft_decode_unchecked(source, store)
    }
}

#[cfg(all(test, feature = "alloc", feature = "derive"))]
mod tests {
    use alloc::{boxed::Box, vec};

    use static_assertions::assert_not_impl_any;

    use crate::{
        ReprC,
        convert::{
            DecodeMove, DecodeMoveUnchecked, SoftDecodeMut, SoftDecodeMutUnchecked, SoftDecodeRef,
            SoftDecodeRefUnchecked, Store,
        },
        lang,
        marker::Native,
        reference::abi::{SoftRef, SoftRefMut},
        rust_spec::RustSpec,
    };

    #[test]
    fn borrowed_boxed_stable_slice_clones_without_consuming_source() {
        use crate::wide::abi::WideBox as AbiWideBox;

        assert_not_impl_any!(Box<[core::cell::Cell<u8>]>: SoftDecodeRef<'static>);
        for unchecked in [false, true] {
            let source = AbiWideBox::<[u8]>::from_box(vec![7, 11].into_boxed_slice());
            let mut store = Default::default();
            let decoded = if unchecked {
                unsafe {
                    <Box<[u8]> as crate::borrow::BorrowedDecodeUnchecked>::soft_decode_unchecked(
                        source, &mut store,
                    )
                }
            } else {
                unsafe {
                    <Box<[u8]> as crate::borrow::BorrowedDecode>::soft_decode(source, &mut store)
                }
                .unwrap()
            };
            assert_eq!(&*decoded, &[7, 11]);
            assert_ne!(decoded.as_ptr(), source.as_ptr().cast_const());
            drop(decoded);
            store.sync().unwrap();
            let original = unsafe { source.into_box() }.unwrap();
            assert_eq!(&*original, &[7, 11]);
        }
    }

    #[test]
    fn borrowed_boxed_stable_slice_validates_elements() {
        use crate::wide::abi::WideBox as AbiWideBox;

        for raw in [0, 1, 2] {
            let source = AbiWideBox::<<[bool] as crate::ReprC>::CType>::from_box(
                vec![crate::primitive::abi::bool(raw)].into_boxed_slice(),
            );
            let mut store = Default::default();
            let decoded = unsafe {
                <Box<[bool]> as crate::borrow::BorrowedDecode>::soft_decode(source, &mut store)
            };
            if raw < 2 {
                assert_eq!(&*decoded.unwrap(), &[raw != 0]);
            } else {
                assert!(decoded.is_none());
            }
            store.sync().unwrap();
            let original = unsafe { source.into_box() }.unwrap();
            assert_eq!(original[0].0, raw);
        }
    }

    #[test]
    fn borrowed_boxed_unstable_slice_clones_without_consuming_source() {
        use crate::{tuple::abi::Tuple2 as AbiTuple2, wide::abi::WideBox as AbiWideBox};

        for unchecked in [false, true] {
            let source = AbiWideBox::<<[(u8, u16)] as crate::ReprC>::CType>::from_box(
                vec![Native(AbiTuple2(7, 11))].into_boxed_slice(),
            );
            let mut store = Default::default();
            let decoded = if unchecked {
                unsafe {
                    <Box<[(u8, u16)]> as crate::borrow::BorrowedDecodeUnchecked>::soft_decode_unchecked(
                        source, &mut store,
                    )
                }
            } else {
                unsafe {
                    <Box<[(u8, u16)]> as crate::borrow::BorrowedDecode>::soft_decode(
                        source, &mut store,
                    )
                }
                .unwrap()
            };
            assert_eq!(&*decoded, &[(7, 11)]);
            assert_ne!(
                decoded.as_ptr().cast::<()>(),
                source.as_ptr().cast_const().cast::<()>()
            );
            drop(decoded);
            store.sync().unwrap();
            let original = unsafe { source.into_box() }.unwrap();
            assert_eq!(original[0], Native(AbiTuple2(7, 11)));
        }
    }

    // Reference-only downstream decoding must not require borrowed or owned value decoding.
    #[derive(RustSpec)]
    struct ReferenceOnly(u8);
    impl ReprC for ReferenceOnly {
        type CType = u8;
    }

    #[test]
    fn transparent_reference_decode_does_not_require_clone_or_value_decoding() {
        #[derive(RustSpec, crate::ReprC)]
        #[repr(transparent)]
        struct Wrapper(ReferenceOnly);

        #[derive(RustSpec, crate::ReprC)]
        #[repr(transparent)]
        enum EnumWrapper {
            Value(ReferenceOnly),
        }

        assert_not_impl_any!(ReferenceOnly: Clone, DecodeMove<'static>);
        let source = unsafe { <Wrapper as ReprC>::CType::from_fields(7) };
        let mut store = Default::default();
        let value = unsafe {
            crate::soft_decode::<&Wrapper, lang::C>(SoftRef::from_ref(&source), &mut store)
        }
        .unwrap();
        assert_eq!(value.0.0, 7);
        store.sync().unwrap();

        let mut store = Default::default();
        let value = unsafe {
            crate::soft_decode_unchecked::<&Wrapper>(SoftRef::from_ref(&source), &mut store)
        };
        assert_eq!(value.0.0, 7);
        store.sync().unwrap();

        let zero = unsafe { <Wrapper as ReprC>::CType::from_fields(0) };
        let mut store = Default::default();
        assert!(
            unsafe {
                crate::soft_decode::<&Wrapper, lang::C>(SoftRef::from_ref(&zero), &mut store)
            }
            .is_none()
        );
        assert!(
            unsafe { crate::soft_decode::<&Wrapper, lang::C>(SoftRef::null(), &mut store) }
                .is_none()
        );

        let source = unsafe { <EnumWrapper as ReprC>::CType::from_fields(9) };
        let mut store = Default::default();
        let value = unsafe {
            crate::soft_decode::<&EnumWrapper, lang::C>(SoftRef::from_ref(&source), &mut store)
        }
        .unwrap();
        let EnumWrapper::Value(field) = value;
        assert_eq!(field.0, 9);
        store.sync().unwrap();
    }

    fn check_shared_decode<R, S>(source: &R::CType, verify: impl Fn(&R))
    where
        R: ReprC + 'static,
        S: Store + Default,
        for<'d> &'d R: DecodeMove<'d, Store = S>
            + DecodeMoveUnchecked<'d, SoftStore = S>
            + ReprC<CType = SoftRef<<R as ReprC>::CType>>,
        <R as ReprC>::CType: Sized,
    {
        for unchecked in [false, true] {
            let mut store = Default::default();
            let value = if unchecked {
                unsafe { crate::soft_decode_unchecked::<&R>(SoftRef::from_ref(source), &mut store) }
            } else {
                unsafe { crate::soft_decode::<&R, lang::C>(SoftRef::from_ref(source), &mut store) }
                    .unwrap()
            };
            verify(value);
            store.sync().unwrap();
        }
    }

    #[test]
    fn shared_containers_extract_non_clone_elements() {
        use crate::{
            boxed::abi::Box as AbiBox, option::abi::Option as AbiOption,
            result::abi::Result as AbiResult, vec::abi::Vec as AbiVec,
            wide::abi::WideBox as AbiWideBox,
        };

        assert_not_impl_any!(ReferenceOnly: Clone, DecodeMove<'static>);

        let source = AbiBox::<<ReferenceOnly as crate::ReprC>::CType>::from_box(Box::new(7));
        check_shared_decode::<Box<ReferenceOnly>, _>(&source, |value| assert_eq!(value.0, 7));
        assert_eq!(*unsafe { source.into_box() }.unwrap(), 7);

        check_shared_decode::<[ReferenceOnly; 2], _>(&[7, 11], |value| {
            assert_eq!([value[0].0, value[1].0], [7, 11]);
        });

        let source = AbiVec::<<ReferenceOnly as crate::ReprC>::CType>::from_vec(vec![7, 11]);
        check_shared_decode::<alloc::vec::Vec<ReferenceOnly>, _>(&Native(source), |value| {
            assert_eq!([value[0].0, value[1].0], [7, 11]);
        });
        assert_eq!(unsafe { source.into_vec() }.unwrap(), [7, 11]);

        let source = AbiVec::<<ReferenceOnly as crate::ReprC>::CType>::from_vec(vec![7, 11]);
        check_shared_decode::<crate::vec::Vec<ReferenceOnly>, _>(&source, |value| {
            assert_eq!([value[0].0, value[1].0], [7, 11]);
        });
        assert_eq!(unsafe { source.into_vec() }.unwrap(), [7, 11]);

        let source = AbiWideBox::<<[ReferenceOnly] as crate::ReprC>::CType>::from_box(
            vec![7, 11].into_boxed_slice(),
        );
        check_shared_decode::<Box<[ReferenceOnly]>, _>(&source, |value| {
            assert_eq!([value[0].0, value[1].0], [7, 11]);
        });
        assert_eq!(&*unsafe { source.into_box() }.unwrap(), &[7, 11]);

        check_shared_decode::<Option<ReferenceOnly>, _>(
            &Native(AbiOption::new(Some(7))),
            |value| {
                assert_eq!(value.as_ref().unwrap().0, 7);
            },
        );
        check_shared_decode::<Option<ReferenceOnly>, _>(&Native(AbiOption::new(None)), |value| {
            assert!(value.is_none());
        });
        check_shared_decode::<crate::option::Option<ReferenceOnly>, _>(
            &Native(AbiOption::new(Some(7))),
            |value| {
                assert!(matches!(
                    value,
                    crate::option::Option::Some(ReferenceOnly(7))
                ))
            },
        );
        check_shared_decode::<Result<ReferenceOnly, ReferenceOnly>, _>(
            &Native(AbiResult::new(Ok(7))),
            |value| {
                assert_eq!(value.as_ref().ok().unwrap().0, 7);
            },
        );
        check_shared_decode::<Result<ReferenceOnly, ReferenceOnly>, _>(
            &Native(AbiResult::new(Err(11))),
            |value| {
                assert_eq!(value.as_ref().err().unwrap().0, 11);
            },
        );
        check_shared_decode::<crate::result::Result<ReferenceOnly, ReferenceOnly>, _>(
            &AbiResult::new(Ok(7)),
            |value| assert!(matches!(value, crate::result::Result::Ok(ReferenceOnly(7)))),
        );
    }

    #[test]
    fn extracted_reference_keeps_its_child_store_borrow() {
        use crate::tuple::abi::Tuple1;

        let inner = Native(Tuple1(7_u8));
        let source = Native(Tuple1(SoftRef::from_ref(&inner)));
        for unchecked in [false, true] {
            let mut store = Default::default();
            let mut unchecked_store = Default::default();
            let carrier = SoftRef::from_ref(&source);
            let decoded = if unchecked {
                unsafe { crate::soft_decode_unchecked::<&(&(u8,),)>(carrier, &mut unchecked_store) }
            } else {
                unsafe { crate::soft_decode::<&(&(u8,),), lang::C>(carrier, &mut store) }.unwrap()
            };
            assert_eq!(decoded.0, &(7,));
        }
    }

    #[test]
    fn mixed_aggregates_extract_non_clone_fields() {
        #[derive(RustSpec, crate::ReprC)]
        struct Record(ReferenceOnly, u32);

        check_shared_decode::<(u32, ReferenceOnly), _>(
            &Native(crate::tuple::abi::Tuple2(5, 7)),
            |value| assert_eq!((value.0, value.1.0), (5, 7)),
        );
        check_shared_decode::<crate::tuple::Tuple2<u32, ReferenceOnly>, _>(
            &crate::tuple::abi::Tuple2::<_, _>(5, 7),
            |value| assert_eq!((value.0, value.1.0), (5, 7)),
        );
        let source = unsafe { <Record as ReprC>::CType::from_fields(7, 5) };
        check_shared_decode::<Record, _>(&source, |value| assert_eq!((value.0.0, value.1), (7, 5)));
    }

    #[derive(Default)]
    struct SharedStore(Option<ReferenceOnly>);
    impl Store for SharedStore {
        fn sync(self) -> Option<()> {
            Some(())
        }
    }

    unsafe impl crate::sync::TakeDecoded for SharedStore {
        type Value = ReferenceOnly;

        unsafe fn take_decoded(store: *mut Self) -> Option<Self::Value> {
            unsafe { (*store).0.take() }
        }
    }

    unsafe impl<'d> SoftDecodeRefUnchecked<'d> for ReferenceOnly {
        type SoftStore = SharedStore;

        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut SharedStore,
        ) -> &'d Self {
            let value = *unsafe { source.into_ref().unwrap_unchecked() };
            store.0.insert(Self(value))
        }
    }
    unsafe impl<'d> SoftDecodeRef<'d, lang::C> for ReferenceOnly {
        type Store = SharedStore;

        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut SharedStore,
        ) -> Option<&'d Self> {
            let value = *unsafe { source.into_ref()? };
            if value == 0 {
                return None;
            }
            Some(store.0.insert(Self(value)))
        }
    }

    #[derive(Default)]
    struct MutableStore {
        source: Option<core::ptr::NonNull<u8>>,
        value: Option<ReferenceOnly>,
    }
    impl Store for MutableStore {
        fn sync(self) -> Option<()> {
            if let (Some(source), Some(value)) = (self.source, self.value) {
                // The decode contract keeps the foreign storage exclusively accessible through sync.
                unsafe { source.as_ptr().write(value.0) };
            }
            Some(())
        }
    }

    unsafe impl<'d> SoftDecodeMutUnchecked<'d> for ReferenceOnly {
        type SoftStore = MutableStore;

        unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::SoftStore,
        ) -> &'d mut Self {
            let source = unsafe { source.into_mut().unwrap_unchecked() };
            let value = ReferenceOnly(*source);
            store.source = Some(core::ptr::NonNull::from(source));
            store.value.insert(value)
        }
    }
    unsafe impl<'d> SoftDecodeMut<'d, lang::C> for ReferenceOnly {
        type Store = MutableStore;

        unsafe fn soft_decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let source = unsafe { source.into_mut()? };
            if *source == 0 {
                return None;
            }
            let value = ReferenceOnly(*source);
            store.source = Some(core::ptr::NonNull::from(source));
            Some(store.value.insert(value))
        }
    }

    #[test]
    fn downstream_shared_decoder_is_used_without_owned_decoding() {
        assert_not_impl_any!(ReferenceOnly: crate::borrow::BorrowedDecode<'static>, DecodeMove<'static>);
        let source = 7;
        let mut store = Default::default();
        let value = unsafe {
            crate::soft_decode::<&ReferenceOnly, lang::C>(SoftRef::from_ref(&source), &mut store)
        }
        .unwrap();
        assert_eq!(value.0, 7);
        store.sync().unwrap();

        let mut store = Default::default();
        assert!(
            unsafe { crate::soft_decode::<&ReferenceOnly, lang::C>(SoftRef::null(), &mut store) }
                .is_none()
        );
        let mut store = Default::default();
        let zero = 0;
        assert!(
            unsafe {
                crate::soft_decode::<&ReferenceOnly, lang::C>(SoftRef::from_ref(&zero), &mut store)
            }
            .is_none()
        );
        // Exercise unchecked dispatch with a valid source as well.
        let mut store = Default::default();
        let value = unsafe {
            crate::soft_decode_unchecked::<&ReferenceOnly>(SoftRef::from_ref(&source), &mut store)
        };
        assert_eq!(value.0, 7);
    }

    #[test]
    fn downstream_mutable_decoder_synchronizes_the_source() {
        for unchecked in [false, true] {
            let mut source = 7;
            let mut store = Default::default();
            let carrier = SoftRefMut::from_mut(&mut source);
            let value = if unchecked {
                unsafe { crate::soft_decode_unchecked::<&mut ReferenceOnly>(carrier, &mut store) }
            } else {
                unsafe { crate::soft_decode::<&mut ReferenceOnly, lang::C>(carrier, &mut store) }
                    .unwrap()
            };
            value.0 = 11;
            store.sync().unwrap();
            assert_eq!(source, 11);
        }
        let mut store = Default::default();
        assert!(
            unsafe {
                crate::soft_decode::<&mut ReferenceOnly, lang::C>(SoftRefMut::null(), &mut store)
            }
            .is_none()
        );
    }

    #[test]
    fn mutable_slice_decode_writes_changes_back() {
        use crate::{
            tuple::abi::Tuple2 as AbiTuple2, wide::abi::SoftWideRefMut as AbiSoftWideRefMut,
        };

        let mut values = [Native(AbiTuple2(1_u8, 2_u8))];
        let source = AbiSoftWideRefMut::from_mut(&mut values[..]);
        let mut store = Default::default();
        let decoded =
            unsafe { crate::soft_decode::<&mut [(u8, u8)], lang::C>(source, &mut store) }.unwrap();
        decoded[0] = (3, 4);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(values[0], Native(AbiTuple2(3, 4)));
    }
}
