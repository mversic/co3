//! Referent-specific decoding and mutable writeback for unstable layouts.
#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};

use disjoint_impls::disjoint_impls;
use rust_spec::{RustSpec, Unstable};
#[cfg(feature = "alloc")]
use rust_spec::{
    mutability::{Exclusive, ExclusiveKind},
    size::{MetaSized, SliceLike},
};

use crate::{
    CType, ReprC,
    borrow::Borrowed,
    ffi::NulTerminatedBuf,
    niche::Niche,
    option::{COption, COptionSome},
    reference::{CRef, CRefMut},
    result::{CResult, CResultErr, CResultOk},
    stored::{DecodeMove, EmptyStore, EncodeMove, Store},
    sync::{ArrayStore, RefMutSizedDecodeStore, RefSizedDecodeStore},
};
#[cfg(feature = "alloc")]
use crate::{
    boxed::{CBox, CWideBox},
    slice::CWideRef,
    sync::{RefDstDecodeStore, RefMutSliceDecodeStore},
    vec::CVec,
};

disjoint_impls! {
    /// Decodes a shared reference to a referent from its reference carrier.
    ///
    /// # Safety
    /// Implementations must preserve the carrier's borrowing and ownership rules.
    /// The decoded reference may borrow from the input storage or the conversion
    /// store, both of which must remain valid for `'d`. An `EmptyStore` must not
    /// back any returned reference. Shared interior-mutable contents must never
    /// be read during conversion or validation.
    pub unsafe trait DecodeRef<'d>: ReprC + 'd {
        type Store: Store + Default;

        /// Validate and decode the reference without taking ownership of its source.
        ///
        /// # Safety
        /// The source must satisfy the pointer, lifetime and aliasing requirements
        /// of `crate::soft_decode`.
        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self>
        where
            &'d Self: ReprC<CType: Sized>;

        /// Decode a reference whose value validity is guaranteed by the caller.
        ///
        /// # Safety
        /// The source must satisfy `crate::soft_decode_unchecked`, including
        /// validity, lifetime and aliasing requirements.
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self
        where
            &'d Self: ReprC<CType: Sized>,
        {
            unsafe { Self::decode_ref(source, store) }.unwrap()
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: Clone + 'd, S: rust_spec::size::SizedKind> DecodeRef<'d> for Box<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CBox<T>>,
        T: ReprC<CType: CType + Sized> + RustSpec<Size = rust_spec::size::Sized<S>>,
        &'d Self: ReprC<CType = CRef<Self>>,
        &'d T: DecodeMove<'d> + ReprC<CType = CRef<T>>,
    {
        type Store = RefSizedDecodeStore<Self, <&'d T as DecodeMove<'d>>::Store>;

        unsafe fn decode_ref<'itm: 'd>(source: <&'d Self as ReprC>::CType, store: &'itm mut Self::Store) -> Option<&'d Self> {
            let header = unsafe { *source.into_ref()? };
            let carrier = unsafe { CRef::from_raw(header.as_ptr()) };
            store.decode(|field| {
                let value = unsafe { <&'d T>::soft_decode(carrier, field)? };
                Some(Box::new(T::clone(value)))
            })
        }
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let header = unsafe { *source.into_ref().unwrap_unchecked() };
            let carrier = unsafe { CRef::from_raw(header.as_ptr()) };
            store
                .decode(|field| {
                    let value = unsafe { <&'d T>::soft_decode_unchecked(carrier, field) };
                    Some(Box::new(T::clone(value)))
                })
                .expect("unchecked decoding produces a value")
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: ?Sized + alloc::borrow::ToOwned + 'd> DecodeRef<'d> for Box<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CWideBox<T>>,
        T: ReprC<
                CType: crate::wide::Wide<
                    Header: crate::wide::WideHeader<Data: CType>,
                    Metadata = usize,
                >,
            > + RustSpec<Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>>,
        &'d Self: ReprC<CType = CRef<Self>>,
        &'d T: DecodeMove<'d> + ReprC<CType = CWideRef<T>>,
        Self: From<<T as alloc::borrow::ToOwned>::Owned>,
    {
        type Store = RefSizedDecodeStore<Self, <&'d T as DecodeMove<'d>>::Store>;
        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let header = unsafe { *source.into_ref()? };
            let carrier = unsafe { CWideRef::from_raw_parts(header.as_ptr(), header.metadata()) };
            store.decode(|field| {
                let value = unsafe { <&'d T>::soft_decode(carrier, field)? };
                Some(alloc::borrow::ToOwned::to_owned(value).into())
            })
        }
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let header = unsafe { *source.into_ref().unwrap_unchecked() };
            let carrier = unsafe { CWideRef::from_raw_parts(header.as_ptr(), header.metadata()) };
            store
                .decode(|field| {
                    let value = unsafe { <&'d T>::soft_decode_unchecked(carrier, field) };
                    Some(alloc::borrow::ToOwned::to_owned(value).into())
                })
                .expect("unchecked decoding produces a value")
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: ReprC<CType: CType + Sized + 'd> + Clone + 'd, __Interior: ExclusiveKind>
        DecodeRef<'d> for [R]
    where
        &'d Self: RustSpec<Layout = Unstable>,
        [R]: RustSpec<Layout = Unstable, Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>,
        &'d R: DecodeMove<'d> + ReprC<CType = CRef<R>>,
    {
        type Store = RefDstDecodeStore<[R], Box<[<&'d R as DecodeMove<'d>>::Store]>>;

        unsafe fn decode_ref<'itm: 'd>(
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
                        let carrier = CRef::from_ref(value);
                        let value = unsafe { <&'d R>::soft_decode(carrier, field_store)? };
                        values.push(R::clone(value));
                    }
                    Some(values)
                })
                .map(Vec::as_slice)
        }

        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
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
                        let carrier = CRef::from_ref(value);
                        let value = unsafe { <&'d R>::soft_decode_unchecked(carrier, field_store) };
                        values.push(R::clone(value));
                    }
                    Some(values)
                })
                .expect("unchecked decoding produces a value")
                .as_slice()
        }
    }

    unsafe impl<'d, T: ReprC<CType: CType + Copy> + Clone + 'd> DecodeRef<'d> for Option<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = COption<T>>,
        &'d Self: ReprC<CType = CRef<Self>>,
        T: RustSpec<Niche = rust_spec::niche::WithoutNiche>,
        &'d T: DecodeMove<'d> + ReprC<CType = CRef<T>>,
    {
        type Store = RefSizedDecodeStore<Self, Option<<&'d T as DecodeMove<'d>>::Store>>;
        unsafe fn decode_ref<'itm: 'd>(
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
                    let value = unsafe { <&'d T>::soft_decode(CRef::from_ref(value), field) }?;
                    Some(Self::Some(T::clone(value)))
                }
            })
        }
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
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
                        let value =
                            unsafe { <&'d T>::soft_decode_unchecked(CRef::from_ref(value), field) };
                        Some(Self::Some(T::clone(value)))
                    }
                })
                .expect("unchecked decoding produces a value")
        }
    }

    unsafe impl<'d, T: ReprC<CType: CType + Copy> + Clone + 'd> DecodeRef<'d>
        for crate::option::Option<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = COption<T>>,
        &'d Self: ReprC<CType = CRef<Self>>,
        &'d T: DecodeMove<'d> + ReprC<CType = CRef<T>>,
    {
        type Store = RefSizedDecodeStore<Self, Option<<&'d T as DecodeMove<'d>>::Store>>;
        unsafe fn decode_ref<'itm: 'd>(
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
                    let value = unsafe { <&'d T>::soft_decode(CRef::from_ref(value), field) }?;
                    Some(Self::Some(T::clone(value)))
                }
            })
        }
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
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
                        let value =
                            unsafe { <&'d T>::soft_decode_unchecked(CRef::from_ref(value), field) };
                        Some(Self::Some(T::clone(value)))
                    }
                })
                .expect("unchecked decoding produces a value")
        }
    }

    unsafe impl<
        'd,
        T: ReprC<CType: CType + Copy> + Clone + 'd,
        E: ReprC<CType: CType + Copy> + Clone + 'd,
        K: rust_spec::size::SizedKind,
    > DecodeRef<'d> for Result<T, E>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CResult<T, E>>,
        &'d Self: ReprC<CType = CRef<Self>>,
        T: RustSpec<Size = rust_spec::size::Sized<K>>,
        E: RustSpec<Size = rust_spec::size::Sized<K>>,
        &'d T: DecodeMove<'d> + ReprC<CType = CRef<T>>,
        &'d E: DecodeMove<'d> + ReprC<CType = CRef<E>>,
    {
        type Store = RefSizedDecodeStore<
            Self,
            Option<Result<<&'d T as DecodeMove<'d>>::Store, <&'d E as DecodeMove<'d>>::Store>>,
        >;
        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let source = unsafe { source.into_ref()? };
            store.decode(|fields| match source.as_result()? {
                Ok(value) => {
                    let Ok(field) = fields.insert(Ok(Default::default())) else {
                        unreachable!()
                    };
                    let value = unsafe { <&'d T>::soft_decode(CRef::from_ref(value), field) }?;
                    Some(Self::Ok(T::clone(value)))
                }
                Err(value) => {
                    let Err(field) = fields.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    let value = unsafe { <&'d E>::soft_decode(CRef::from_ref(value), field) }?;
                    Some(Self::Err(E::clone(value)))
                }
            })
        }
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let source = unsafe { source.into_ref().unwrap_unchecked() };
            store
                .decode(|fields| match source.as_result()? {
                    Ok(value) => {
                        let Ok(field) = fields.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        let value =
                            unsafe { <&'d T>::soft_decode_unchecked(CRef::from_ref(value), field) };
                        Some(Self::Ok(T::clone(value)))
                    }
                    Err(value) => {
                        let Err(field) = fields.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        let value =
                            unsafe { <&'d E>::soft_decode_unchecked(CRef::from_ref(value), field) };
                        Some(Self::Err(E::clone(value)))
                    }
                })
                .expect("unchecked decoding produces a value")
        }
    }

    unsafe impl<
        'd,
        T: ReprC<CType: CType + Copy> + Clone + 'd,
        E: ReprC<CType: CType + Copy> + Clone + 'd,
    > DecodeRef<'d> for crate::result::Result<T, E>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CResult<T, E>>,
        &'d Self: ReprC<CType = CRef<Self>>,
        &'d T: DecodeMove<'d> + ReprC<CType = CRef<T>>,
        &'d E: DecodeMove<'d> + ReprC<CType = CRef<E>>,
    {
        type Store = RefSizedDecodeStore<
            Self,
            Option<Result<<&'d T as DecodeMove<'d>>::Store, <&'d E as DecodeMove<'d>>::Store>>,
        >;
        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let source = unsafe { source.into_ref()? };
            store.decode(|fields| match source.as_result()? {
                Ok(value) => {
                    let Ok(field) = fields.insert(Ok(Default::default())) else {
                        unreachable!()
                    };
                    let value = unsafe { <&'d T>::soft_decode(CRef::from_ref(value), field) }?;
                    Some(Self::Ok(T::clone(value)))
                }
                Err(value) => {
                    let Err(field) = fields.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    let value = unsafe { <&'d E>::soft_decode(CRef::from_ref(value), field) }?;
                    Some(Self::Err(E::clone(value)))
                }
            })
        }
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let source = unsafe { source.into_ref().unwrap_unchecked() };
            store
                .decode(|fields| match source.as_result()? {
                    Ok(value) => {
                        let Ok(field) = fields.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        let value =
                            unsafe { <&'d T>::soft_decode_unchecked(CRef::from_ref(value), field) };
                        Some(Self::Ok(T::clone(value)))
                    }
                    Err(value) => {
                        let Err(field) = fields.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        let value =
                            unsafe { <&'d E>::soft_decode_unchecked(CRef::from_ref(value), field) };
                        Some(Self::Err(E::clone(value)))
                    }
                })
                .expect("unchecked decoding produces a value")
        }
    }
    unsafe impl<
        'd,
        T: Niche<CType: CType + PartialEq + Sized> + Clone + 'd,
        N: rust_spec::layout::LayoutKind,
    > DecodeRef<'d> for Option<T>
    where
        T: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: RustSpec<Layout = Unstable> + ReprC<CType = COptionSome<T>>,
        &'d Self: ReprC<CType = CRef<Self>>,
        &'d T: DecodeMove<'d> + ReprC<CType = CRef<T>>,
    {
        type Store = RefSizedDecodeStore<Self, Option<<&'d T as DecodeMove<'d>>::Store>>;
        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let value = unsafe { source.into_ref()? };
            store.decode(|field| {
                if value.is_niche() {
                    *field = None;
                    return Some(None);
                }
                let value = unsafe {
                    <&'d T>::soft_decode(
                        CRef::from_ref(value.carrier()),
                        field.insert(Default::default()),
                    )
                }?;
                Some(Some(T::clone(value)))
            })
        }
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let value = unsafe { source.into_ref().unwrap_unchecked() };
            store
                .decode(|field| {
                    if value.is_niche() {
                        *field = None;
                        return Some(None);
                    }
                    let value = unsafe {
                        <&'d T>::soft_decode_unchecked(
                            CRef::from_ref(value.carrier()),
                            field.insert(Default::default()),
                        )
                    };
                    Some(Some(T::clone(value)))
                })
                .expect("unchecked decoding produces a value")
        }
    }
    unsafe impl<
        'd,
        T: Niche<CType: CType + PartialEq + Sized> + Clone + 'd,
        N: rust_spec::layout::LayoutKind,
        E: Default + ReprC + 'd,
    > DecodeRef<'d> for Result<T, E>
    where
        T: RustSpec<
                Niche = rust_spec::niche::WithNiche<N>,
                Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>,
            >,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CResultOk<T, E>>,
        &'d Self: ReprC<CType = CRef<Self>>,
        &'d T: DecodeMove<'d> + ReprC<CType = CRef<T>>,
    {
        type Store = RefSizedDecodeStore<Self, Option<<&'d T as DecodeMove<'d>>::Store>>;
        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let value = unsafe { source.into_ref()? };
            store.decode(|field| {
                if value.is_niche() {
                    *field = None;
                    return Some(Err(E::default()));
                }
                let value = unsafe {
                    <&'d T>::soft_decode(
                        CRef::from_ref(value.carrier()),
                        field.insert(Default::default()),
                    )
                }?;
                Some(Ok(T::clone(value)))
            })
        }
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let value = unsafe { source.into_ref().unwrap_unchecked() };
            store
                .decode(|field| {
                    if value.is_niche() {
                        *field = None;
                        return Some(Err(E::default()));
                    }
                    let value = unsafe {
                        <&'d T>::soft_decode_unchecked(
                            CRef::from_ref(value.carrier()),
                            field.insert(Default::default()),
                        )
                    };
                    Some(Ok(T::clone(value)))
                })
                .expect("unchecked decoding produces a value")
        }
    }
    unsafe impl<
        'd,
        E: Niche<CType: CType + PartialEq + Sized> + Clone + 'd,
        N: rust_spec::layout::LayoutKind,
        T: Default + ReprC + 'd,
    > DecodeRef<'d> for Result<T, E>
    where
        E: RustSpec<
                Niche = rust_spec::niche::WithNiche<N>,
                Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>,
            >,
        T: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CResultErr<T, E>>,
        &'d Self: ReprC<CType = CRef<Self>>,
        &'d E: DecodeMove<'d> + ReprC<CType = CRef<E>>,
    {
        type Store = RefSizedDecodeStore<Self, Option<<&'d E as DecodeMove<'d>>::Store>>;
        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let value = unsafe { source.into_ref()? };
            store.decode(|field| {
                if value.is_niche() {
                    *field = None;
                    return Some(Ok(T::default()));
                }
                let value = unsafe {
                    <&'d E>::soft_decode(
                        CRef::from_ref(value.carrier()),
                        field.insert(Default::default()),
                    )
                }?;
                Some(Err(E::clone(value)))
            })
        }
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let value = unsafe { source.into_ref().unwrap_unchecked() };
            store
                .decode(|field| {
                    if value.is_niche() {
                        *field = None;
                        return Some(Ok(T::default()));
                    }
                    let value = unsafe {
                        <&'d E>::soft_decode_unchecked(
                            CRef::from_ref(value.carrier()),
                            field.insert(Default::default()),
                        )
                    };
                    Some(Err(E::clone(value)))
                })
                .expect("unchecked decoding produces a value")
        }
    }
    // Decode borrowed vector headers without taking ownership of their allocations.
    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: ReprC<CType: CType + Sized> + Clone + 'd> DecodeRef<'d> for alloc::vec::Vec<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CVec<T>>,
        &'d Self: ReprC<CType = CRef<Self>>,
        &'d T: DecodeMove<'d> + ReprC<CType = CRef<T>>,
    {
        type Store = RefSizedDecodeStore<Self, Box<[<&'d T as DecodeMove<'d>>::Store]>>;

        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            // CVec is Copy; only the header is read here.
            let header = unsafe { *source.into_ref()? };
            if header.as_ptr().is_null() || header.len() > header.capacity() {
                return None;
            }
            store.decode(|fields| {
                *fields = core::iter::repeat_with(Default::default)
                    .take(header.len())
                    .collect();
                let mut values = Vec::with_capacity(header.capacity());
                for (index, field) in fields.iter_mut().enumerate() {
                    let carrier = unsafe { CRef::from_raw(header.as_ptr().add(index)) };
                    let value = unsafe { <&'d T>::soft_decode(carrier, field)? };
                    values.push(T::clone(value));
                }
                Some(values)
            })
        }

        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let header = unsafe { *source.into_ref().unwrap_unchecked() };
            store
                .decode(|fields| {
                    *fields = core::iter::repeat_with(Default::default)
                        .take(header.len())
                        .collect();
                    let mut values = Vec::with_capacity(header.capacity());
                    for (index, field) in fields.iter_mut().enumerate() {
                        let carrier = unsafe { CRef::from_raw(header.as_ptr().add(index)) };
                        let value = unsafe { <&'d T>::soft_decode_unchecked(carrier, field) };
                        values.push(T::clone(value));
                    }
                    Some(values)
                })
                .expect("unchecked decoding produces a value")
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: ReprC<CType: CType + Sized> + Clone + 'd> DecodeRef<'d> for crate::vec::Vec<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CVec<T>>,
        &'d Self: ReprC<CType = CRef<Self>>,
        &'d T: DecodeMove<'d> + ReprC<CType = CRef<T>>,
    {
        type Store = RefSizedDecodeStore<Self, Box<[<&'d T as DecodeMove<'d>>::Store]>>;

        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            // CVec is Copy; only the header is read here.
            let header = unsafe { *source.into_ref()? };
            if header.as_ptr().is_null() || header.len() > header.capacity() {
                return None;
            }
            store.decode(|fields| {
                *fields = core::iter::repeat_with(Default::default)
                    .take(header.len())
                    .collect();
                let mut values = Vec::with_capacity(header.capacity());
                for (index, field) in fields.iter_mut().enumerate() {
                    let carrier = unsafe { CRef::from_raw(header.as_ptr().add(index)) };
                    let value = unsafe { <&'d T>::soft_decode(carrier, field)? };
                    values.push(T::clone(value));
                }
                Some(values.into())
            })
        }

        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let header = unsafe { *source.into_ref().unwrap_unchecked() };
            store
                .decode(|fields| {
                    *fields = core::iter::repeat_with(Default::default)
                        .take(header.len())
                        .collect();
                    let mut values = Vec::with_capacity(header.capacity());
                    for (index, field) in fields.iter_mut().enumerate() {
                        let carrier = unsafe { CRef::from_raw(header.as_ptr().add(index)) };
                        let value = unsafe { <&'d T>::soft_decode_unchecked(carrier, field) };
                        values.push(T::clone(value));
                    }
                    Some(values.into())
                })
                .expect("unchecked decoding produces a value")
        }
    }

    unsafe impl<'d, T: ReprC<CType: CType + Sized + 'd> + Clone + 'd, const N: usize> DecodeRef<'d>
        for [T; N]
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = [<T as ReprC>::CType; N]>,
        &'d Self: ReprC<CType = CRef<Self>>,
        &'d T: DecodeMove<'d> + ReprC<CType = CRef<T>>,
    {
        type Store = RefSizedDecodeStore<Self, ArrayStore<<&'d T as DecodeMove<'d>>::Store, N>>;

        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let source: &'d [<T as ReprC>::CType; N] = unsafe { source.into_ref()? };
            store.decode(|fields| {
                let mut fields = fields.0.iter_mut();
                let values: [Option<T>; N] = core::array::from_fn(|i| {
                    let carrier = CRef::from_ref(&source[i]);
                    unsafe { <&'d T>::soft_decode(carrier, fields.next().unwrap()) }.cloned()
                });
                if values.iter().any(Option::is_none) {
                    return None;
                }
                Some(values.map(Option::unwrap))
            })
        }

        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let source: &'d [<T as ReprC>::CType; N] = unsafe { source.into_ref().unwrap_unchecked() };
            store
                .decode(|fields| {
                    let mut fields = fields.0.iter_mut();
                    Some(core::array::from_fn(|i| {
                        let carrier = CRef::from_ref(&source[i]);
                        T::clone(unsafe {
                            <&'d T>::soft_decode_unchecked(carrier, fields.next().unwrap())
                        })
                    }))
                })
                .expect("unchecked decoding produces a value")
        }
    }

    unsafe impl<'d, T: ReprC<CType: CType + Sized> + 'd> DecodeRef<'d> for core::mem::ManuallyDrop<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
        &'d Self: ReprC<CType = CRef<Self>>,
        &'d T: DecodeMove<'d> + ReprC<CType = CRef<T>>,
    {
        type Store = <&'d T as DecodeMove<'d>>::Store;
        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let source = unsafe { CRef::from_raw(source.as_ptr().cast()) };
            let inner = unsafe { <&'d T>::soft_decode(source, store)? };
            // Both Rust and C wrappers are transparent; this does not read the value.
            Some(unsafe { &*core::ptr::from_ref(inner).cast::<Self>() })
        }
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let source = unsafe { CRef::from_raw(source.as_ptr().cast()) };
            let inner = unsafe { <&'d T>::soft_decode_unchecked(source, store) };
            unsafe { &*core::ptr::from_ref(inner).cast::<Self>() }
        }
    }

    unsafe impl<'d, T: ReprC<CType: CType + Sized> + 'd> DecodeRef<'d> for crate::borrow::Borrowed<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
        &'d Self: ReprC<CType = CRef<Self>>,
        &'d T: DecodeMove<'d> + ReprC<CType = CRef<T>>,
    {
        type Store = <&'d T as DecodeMove<'d>>::Store;
        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let source = unsafe { CRef::from_raw(source.as_ptr().cast()) };
            let inner = unsafe { <&'d T>::soft_decode(source, store)? };
            // Both Rust and C wrappers are transparent; this does not read the value.
            Some(unsafe { &*core::ptr::from_ref(inner).cast::<Self>() })
        }
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let source = unsafe { CRef::from_raw(source.as_ptr().cast()) };
            let inner = unsafe { <&'d T>::soft_decode_unchecked(source, store) };
            unsafe { &*core::ptr::from_ref(inner).cast::<Self>() }
        }
    }

    unsafe impl<'d, 'a: 'd, T: ?Sized + 'a> DecodeRef<'d> for &'a T
    where
        Self: RustSpec<Layout = Unstable> + DecodeMove<'d, CType: Copy>,
        &'d Self: ReprC<CType = CRef<Self>>,
    {
        type Store = RefSizedDecodeStore<Self, <Self as DecodeMove<'d>>::Store>;
        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let carrier = unsafe { *source.into_ref()? };
            store.decode(|field| unsafe { Self::soft_decode(carrier, field) })
        }
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let carrier = unsafe { *source.into_ref().unwrap_unchecked() };
            store
                .decode(|field| Some(unsafe { Self::soft_decode_unchecked(carrier, field) }))
                .expect("unchecked decoding produces a value")
        }
    }
}

// Null-terminated referents are implemented individually so downstream types
// can provide their own reference decoder.
macro_rules! impl_nul_decode_ref {
    ($ty:ty) => {
        unsafe impl<'d> DecodeRef<'d> for $ty {
            type Store = ();

            unsafe fn decode_ref<'itm: 'd>(
                source: <&'d Self as ReprC>::CType,
                store: &'itm mut (),
            ) -> Option<&'d Self> {
                if source.as_ptr().is_null() {
                    return None;
                }
                Some(unsafe { Self::decode_ref_unchecked(source, store) })
            }

            unsafe fn decode_ref_unchecked<'itm: 'd>(
                source: <&'d Self as ReprC>::CType,
                _: &'itm mut (),
            ) -> &'d Self {
                unsafe { <Self as NulTerminatedBuf>::from_raw(source.as_ptr().cast()) }
            }
        }
    };
}

impl_nul_decode_ref!(core::ffi::CStr);
impl_nul_decode_ref!(core::mem::ManuallyDrop<core::ffi::CStr>);

disjoint_impls! {
    /// Decodes a mutable reference to a referent from its reference carrier.
    ///
    /// # Safety
    /// Implementations must preserve the carrier's borrowing and ownership rules.
    /// The decoded reference may borrow from the input storage or the conversion
    /// store, both of which must remain valid for `'d`. An `EmptyStore` must not
    /// back any returned reference. Shared interior-mutable contents must never
    /// be read during conversion or validation.
    pub unsafe trait DecodeMut<'d>: ReprC + 'd {
        type Store: Store + Default;

        /// Validate and decode the reference without taking ownership of its source.
        ///
        /// # Safety
        /// The source must satisfy the pointer, lifetime and aliasing requirements
        /// of `crate::soft_decode`. Mutable writes must be synchronized back without
        /// transferring ownership.
        unsafe fn decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self>
        where
            &'d mut Self: ReprC<CType: Sized>;

        /// Decode a reference whose value validity is guaranteed by the caller.
        ///
        /// # Safety
        /// The source must satisfy `crate::soft_decode_unchecked`, including
        /// validity, lifetime and aliasing requirements.
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self
        where
            &'d mut Self: ReprC<CType: Sized>,
        {
            unsafe { Self::decode_mut(source, store) }.unwrap()
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: ReprC<CType: CType + Copy> + EncodeMove<Store: EmptyStore> + 'd> DecodeMut<'d>
        for [R]
    where
        Borrowed<R>: DecodeMove<'d> + ReprC<CType = Borrowed<<R as ReprC>::CType>>,
        &'d mut Self: RustSpec<Layout = Unstable>,
        [R]: RustSpec<Layout = Unstable, Size = MetaSized<SliceLike>>,
    {
        type Store = RefMutSliceDecodeStore<R, Box<[<Borrowed<R> as DecodeMove<'d>>::Store]>>;

        #[inline(always)]
        unsafe fn decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut <Self as DecodeMut<'d>>::Store,
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
                        .map(|(value, field)| decode_mut_field::<R>(value, field))
                        .collect()
                })
            }
        }

        #[inline(always)]
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut <Self as DecodeMut<'d>>::Store,
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
    unsafe impl<'d, T: ReprC<CType: CType + Copy> + 'd> DecodeMut<'d> for Option<T>
    where
        Borrowed<T>: DecodeMove<'d> + ReprC<CType = Borrowed<<T as ReprC>::CType>>,
        Self: EncodeMove<Store: EmptyStore> + RustSpec<Layout = Unstable> + ReprC<CType = COption<T>>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
        T: RustSpec<Niche = rust_spec::niche::WithoutNiche>,
    {
        type Store = RefMutSizedDecodeStore<Self, Option<<Borrowed<T> as DecodeMove<'d>>::Store>>;
        unsafe fn decode_mut<'itm: 'd>(
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
                        let value = decode_mut_field::<T>(*value, field)?;
                        Some(Self::Some(value))
                    }
                })
            }
        }
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
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

    unsafe impl<'d, T: ReprC<CType: CType + Copy> + 'd> DecodeMut<'d> for crate::option::Option<T>
    where
        Borrowed<T>: DecodeMove<'d> + ReprC<CType = Borrowed<<T as ReprC>::CType>>,
        Self: EncodeMove<Store: EmptyStore> + RustSpec<Layout = Unstable> + ReprC<CType = COption<T>>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
    {
        type Store = RefMutSizedDecodeStore<Self, Option<<Borrowed<T> as DecodeMove<'d>>::Store>>;
        unsafe fn decode_mut<'itm: 'd>(
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
                        let value = decode_mut_field::<T>(*value, field)?;
                        Some(Self::Some(value))
                    }
                })
            }
        }
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
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

    unsafe impl<
        'd,
        T: ReprC<CType: CType + Copy> + 'd,
        E: ReprC<CType: CType + Copy> + 'd,
        K: rust_spec::size::SizedKind,
    > DecodeMut<'d> for Result<T, E>
    where
        Borrowed<T>: DecodeMove<'d> + ReprC<CType = Borrowed<<T as ReprC>::CType>>,
        Borrowed<E>: DecodeMove<'d> + ReprC<CType = Borrowed<<E as ReprC>::CType>>,
        Self:
            EncodeMove<Store: EmptyStore> + RustSpec<Layout = Unstable> + ReprC<CType = CResult<T, E>>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
        T: RustSpec<Size = rust_spec::size::Sized<K>>,
        E: RustSpec<Size = rust_spec::size::Sized<K>>,
    {
        type Store = RefMutSizedDecodeStore<
            Self,
            Option<
                Result<<Borrowed<T> as DecodeMove<'d>>::Store, <Borrowed<E> as DecodeMove<'d>>::Store>,
            >,
        >;
        unsafe fn decode_mut<'itm: 'd>(
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
                        let value = decode_mut_field::<T>(*value, field)?;
                        Some(Self::Ok(value))
                    }
                    Err(value) => {
                        let Err(field) = fields.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        let value = decode_mut_field::<E>(*value, field)?;
                        Some(Self::Err(value))
                    }
                })
            }
        }
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
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

    unsafe impl<'d, T: ReprC<CType: CType + Copy> + 'd, E: ReprC<CType: CType + Copy> + 'd>
        DecodeMut<'d> for crate::result::Result<T, E>
    where
        Borrowed<T>: DecodeMove<'d> + ReprC<CType = Borrowed<<T as ReprC>::CType>>,
        Borrowed<E>: DecodeMove<'d> + ReprC<CType = Borrowed<<E as ReprC>::CType>>,
        Self:
            EncodeMove<Store: EmptyStore> + RustSpec<Layout = Unstable> + ReprC<CType = CResult<T, E>>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
    {
        type Store = RefMutSizedDecodeStore<
            Self,
            Option<
                Result<<Borrowed<T> as DecodeMove<'d>>::Store, <Borrowed<E> as DecodeMove<'d>>::Store>,
            >,
        >;
        unsafe fn decode_mut<'itm: 'd>(
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
                        let value = decode_mut_field::<T>(*value, field)?;
                        Some(Self::Ok(value))
                    }
                    Err(value) => {
                        let Err(field) = fields.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        let value = decode_mut_field::<E>(*value, field)?;
                        Some(Self::Err(value))
                    }
                })
            }
        }
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
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
    unsafe impl<
        'd,
        T: Niche<CType: CType + Copy + PartialEq + Sized> + 'd,
        N: rust_spec::layout::LayoutKind,
    > DecodeMut<'d> for Option<T>
    where
        Borrowed<T>: DecodeMove<'d> + ReprC<CType = Borrowed<<T as ReprC>::CType>>,
        T: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self:
            EncodeMove<Store: EmptyStore> + RustSpec<Layout = Unstable> + ReprC<CType = COptionSome<T>>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
    {
        type Store = RefMutSizedDecodeStore<Self, Option<<Borrowed<T> as DecodeMove<'d>>::Store>>;
        unsafe fn decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let pointer = source.as_ptr();
            let value = unsafe { pointer.as_ref()? };
            unsafe {
                store.decode(pointer, |field| {
                    if value.is_niche() {
                        *field = None;
                        return Some(None);
                    }
                    let value =
                        decode_mut_field::<T>(*value.carrier(), field.insert(Default::default()))?;
                    Some(Some(value))
                })
            }
        }
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self {
            let pointer = source.as_ptr();
            let value = unsafe { &*pointer };
            unsafe {
                store.decode(pointer, |field| {
                    if value.is_niche() {
                        *field = None;
                        return Some(None);
                    }
                    let value = decode_mut_field_unchecked::<T>(
                        *value.carrier(),
                        field.insert(Default::default()),
                    );
                    Some(Some(value))
                })
            }
            .expect("unchecked decoding produces a value")
        }
    }
    unsafe impl<
        'd,
        T: Niche<CType: CType + Copy + PartialEq + Sized> + 'd,
        N: rust_spec::layout::LayoutKind,
        E: Default + ReprC + 'd,
    > DecodeMut<'d> for Result<T, E>
    where
        Borrowed<T>: DecodeMove<'d> + ReprC<CType = Borrowed<<T as ReprC>::CType>>,
        T: RustSpec<
                Niche = rust_spec::niche::WithNiche<N>,
                Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>,
            >,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Self: EncodeMove<Store: EmptyStore>
            + RustSpec<Layout = Unstable>
            + ReprC<CType = CResultOk<T, E>>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
    {
        type Store = RefMutSizedDecodeStore<Self, Option<<Borrowed<T> as DecodeMove<'d>>::Store>>;
        unsafe fn decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let pointer = source.as_ptr();
            let value = unsafe { pointer.as_ref()? };
            unsafe {
                store.decode(pointer, |field| {
                    if value.is_niche() {
                        *field = None;
                        return Some(Err(E::default()));
                    }
                    let value =
                        decode_mut_field::<T>(*value.carrier(), field.insert(Default::default()))?;
                    Some(Ok(value))
                })
            }
        }
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self {
            let pointer = source.as_ptr();
            let value = unsafe { &*pointer };
            unsafe {
                store.decode(pointer, |field| {
                    if value.is_niche() {
                        *field = None;
                        return Some(Err(E::default()));
                    }
                    let value = decode_mut_field_unchecked::<T>(
                        *value.carrier(),
                        field.insert(Default::default()),
                    );
                    Some(Ok(value))
                })
            }
            .expect("unchecked decoding produces a value")
        }
    }
    unsafe impl<
        'd,
        E: Niche<CType: CType + Copy + PartialEq + Sized> + 'd,
        N: rust_spec::layout::LayoutKind,
        T: Default + ReprC + 'd,
    > DecodeMut<'d> for Result<T, E>
    where
        Borrowed<E>: DecodeMove<'d> + ReprC<CType = Borrowed<<E as ReprC>::CType>>,
        E: RustSpec<
                Niche = rust_spec::niche::WithNiche<N>,
                Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>,
            >,
        T: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Self: EncodeMove<Store: EmptyStore>
            + RustSpec<Layout = Unstable>
            + ReprC<CType = CResultErr<T, E>>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
    {
        type Store = RefMutSizedDecodeStore<Self, Option<<Borrowed<E> as DecodeMove<'d>>::Store>>;
        unsafe fn decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let pointer = source.as_ptr();
            let value = unsafe { pointer.as_ref()? };
            unsafe {
                store.decode(pointer, |field| {
                    if value.is_niche() {
                        *field = None;
                        return Some(Ok(T::default()));
                    }
                    let value =
                        decode_mut_field::<E>(*value.carrier(), field.insert(Default::default()))?;
                    Some(Err(value))
                })
            }
        }
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self {
            let pointer = source.as_ptr();
            let value = unsafe { &*pointer };
            unsafe {
                store.decode(pointer, |field| {
                    if value.is_niche() {
                        *field = None;
                        return Some(Ok(T::default()));
                    }
                    let value = decode_mut_field_unchecked::<E>(
                        *value.carrier(),
                        field.insert(Default::default()),
                    );
                    Some(Err(value))
                })
            }
            .expect("unchecked decoding produces a value")
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: ReprC<CType: CType + Sized> + Clone + 'd> DecodeMut<'d> for Box<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CBox<T>>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
        &'d mut T: DecodeMove<'d> + ReprC<CType = CRefMut<T, true>>,
    {
        type Store = RefMutSizedDecodeStore<Self, Box<<&'d mut T as DecodeMove<'d>>::Store>, *mut T>;
        unsafe fn decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let header = unsafe { *source.as_ptr().as_ref()? };
            unsafe {
                store.decode_with(
                    |fields| {
                        let carrier = CRefMut::from_raw(header.as_ptr());
                        let value = <&'d mut T>::soft_decode(carrier, &mut **fields)?;
                        Some((Box::new(T::clone(value)), core::ptr::from_mut(value)))
                    },
                    |value, destination| {
                        *destination = *value;
                        Some(())
                    },
                )
            }
        }
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self {
            let header = unsafe { *source.as_ptr().as_ref().unwrap_unchecked() };
            unsafe {
                store.decode_with(
                    |fields| {
                        let carrier = CRefMut::from_raw(header.as_ptr());
                        let value = <&'d mut T>::soft_decode_unchecked(carrier, &mut **fields);
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
    unsafe impl<'d, T: ReprC<CType: CType + Sized> + Clone + 'd> DecodeMut<'d> for Box<[T]>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CWideBox<[T]>>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
        &'d mut T: DecodeMove<'d> + ReprC<CType = CRefMut<T, true>>,
    {
        type Store =
            RefMutSizedDecodeStore<Self, Box<[<&'d mut T as DecodeMove<'d>>::Store]>, Box<[*mut T]>>;
        unsafe fn decode_mut<'itm: 'd>(
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
                            let carrier = CRefMut::from_raw(header.as_ptr().add(i));
                            let value = <&'d mut T>::soft_decode(carrier, field)?;
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
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
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
                            let carrier = CRefMut::from_raw(header.as_ptr().add(i));
                            let value = <&'d mut T>::soft_decode_unchecked(carrier, field);
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

    // Only carriers are copied; Rust elements are decoded through Borrowed<T>.
    // Retain each element's decode store and re-encode the edited array on sync.
    unsafe impl<'d, T: ReprC<CType: CType + Copy> + 'd, const N: usize> DecodeMut<'d> for [T; N]
    where
        Borrowed<T>: DecodeMove<'d> + ReprC<CType = Borrowed<<T as ReprC>::CType>>,
        Self: RustSpec<Layout = Unstable>
            + ReprC<CType = [<T as ReprC>::CType; N]>
            + EncodeMove<Store: EmptyStore>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
    {
        type Store =
            RefMutSizedDecodeStore<Self, ArrayStore<<Borrowed<T> as DecodeMove<'d>>::Store, N>>;

        unsafe fn decode_mut<'itm: 'd>(
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
                        values.map(|value| decode_mut_field::<T>(value, fields.next().unwrap()));
                    if values.iter().any(Option::is_none) {
                        return None;
                    }
                    Some(values.map(Option::unwrap))
                })
            }
        }

        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
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

    // There is no reconstructing shared decoder for Cell/UnsafeCell: reading their
    // contents through a shared reference is forbidden. Stable cell references use
    // the direct DecodeMove path. Likewise, reconstructing &&mut T would duplicate
    // an exclusive reference. COption/CResult are stable identity carriers and use
    // the direct reference path rather than an unstable referent decoder.

    unsafe impl<'d, T: ReprC<CType: CType + Sized> + 'd> DecodeMut<'d> for core::mem::ManuallyDrop<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
        &'d mut T: DecodeMove<'d> + ReprC<CType = CRefMut<T, true>>,
    {
        type Store = <&'d mut T as DecodeMove<'d>>::Store;
        unsafe fn decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let source = unsafe { CRefMut::from_raw(source.as_ptr().cast()) };
            let inner = unsafe { <&'d mut T>::soft_decode(source, store)? };
            // The exclusive borrow permits delegation into a cell; no shared cell is read.
            Some(unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() })
        }
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self {
            let source = unsafe { CRefMut::from_raw(source.as_ptr().cast()) };
            let inner = unsafe { <&'d mut T>::soft_decode_unchecked(source, store) };
            unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() }
        }
    }

    unsafe impl<'d, T: ReprC<CType: CType + Sized> + 'd> DecodeMut<'d> for crate::borrow::Borrowed<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
        &'d mut T: DecodeMove<'d> + ReprC<CType = CRefMut<T, true>>,
    {
        type Store = <&'d mut T as DecodeMove<'d>>::Store;
        unsafe fn decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let source = unsafe { CRefMut::from_raw(source.as_ptr().cast()) };
            let inner = unsafe { <&'d mut T>::soft_decode(source, store)? };
            // The exclusive borrow permits delegation into a cell; no shared cell is read.
            Some(unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() })
        }
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self {
            let source = unsafe { CRefMut::from_raw(source.as_ptr().cast()) };
            let inner = unsafe { <&'d mut T>::soft_decode_unchecked(source, store) };
            unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() }
        }
    }

    unsafe impl<'d, T: ReprC<CType: CType + Sized> + 'd> DecodeMut<'d> for core::cell::Cell<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
        &'d mut T: DecodeMove<'d> + ReprC<CType = CRefMut<T, true>>,
    {
        type Store = <&'d mut T as DecodeMove<'d>>::Store;
        unsafe fn decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let source = unsafe { CRefMut::from_raw(source.as_ptr().cast()) };
            let inner = unsafe { <&'d mut T>::soft_decode(source, store)? };
            // The exclusive borrow permits delegation into a cell; no shared cell is read.
            Some(unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() })
        }
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self {
            let source = unsafe { CRefMut::from_raw(source.as_ptr().cast()) };
            let inner = unsafe { <&'d mut T>::soft_decode_unchecked(source, store) };
            unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() }
        }
    }

    unsafe impl<'d, T: ReprC<CType: CType + Sized> + 'd> DecodeMut<'d> for core::cell::UnsafeCell<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
        &'d mut T: DecodeMove<'d> + ReprC<CType = CRefMut<T, true>>,
    {
        type Store = <&'d mut T as DecodeMove<'d>>::Store;
        unsafe fn decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let source = unsafe { CRefMut::from_raw(source.as_ptr().cast()) };
            let inner = unsafe { <&'d mut T>::soft_decode(source, store)? };
            // The exclusive borrow permits delegation into a cell; no shared cell is read.
            Some(unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() })
        }
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self {
            let source = unsafe { CRefMut::from_raw(source.as_ptr().cast()) };
            let inner = unsafe { <&'d mut T>::soft_decode_unchecked(source, store) };
            unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() }
        }
    }

    unsafe impl<'d, 'a: 'd, T: ?Sized + 'a> DecodeMut<'d> for &'a T
    where
        Self: RustSpec<Layout = Unstable> + DecodeMove<'d, CType: Copy> + EncodeMove<Store: EmptyStore>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
    {
        type Store = RefMutSizedDecodeStore<Self, <Self as DecodeMove<'d>>::Store>;
        unsafe fn decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let source = source.as_ptr();
            unsafe { store.decode(source, |field| Self::soft_decode(source.read(), field)) }
        }
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self {
            let source = source.as_ptr();
            unsafe {
                store.decode(source, |field| {
                    Some(Self::soft_decode_unchecked(source.read(), field))
                })
            }
            .expect("unchecked decoding produces a value")
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: ReprC<CType: CType + Sized> + Clone + 'd> DecodeMut<'d> for alloc::vec::Vec<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CVec<T>>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
        &'d mut T: DecodeMove<'d> + ReprC<CType = CRefMut<T, true>>,
    {
        type Store = RefMutSizedDecodeStore<
            Self,
            Box<[<&'d mut T as DecodeMove<'d>>::Store]>,
            (Box<[*mut T]>, usize),
        >;
        unsafe fn decode_mut<'itm: 'd>(
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
                            let carrier = CRefMut::from_raw(header.as_ptr().add(i));
                            let value = <&'d mut T>::soft_decode(carrier, field)?;
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
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
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
                            let carrier = CRefMut::from_raw(header.as_ptr().add(i));
                            let value = <&'d mut T>::soft_decode_unchecked(carrier, field);
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
    unsafe impl<'d, T: ReprC<CType: CType + Sized> + Clone + 'd> DecodeMut<'d> for crate::vec::Vec<T>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CVec<T>>,
        &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
        &'d mut T: DecodeMove<'d> + ReprC<CType = CRefMut<T, true>>,
    {
        type Store = RefMutSizedDecodeStore<
            Self,
            Box<[<&'d mut T as DecodeMove<'d>>::Store]>,
            (Box<[*mut T]>, usize),
        >;
        unsafe fn decode_mut<'itm: 'd>(
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
                            let carrier = CRefMut::from_raw(header.as_ptr().add(i));
                            let value = <&'d mut T>::soft_decode(carrier, field)?;
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
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
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
                            let carrier = CRefMut::from_raw(header.as_ptr().add(i));
                            let value = <&'d mut T>::soft_decode_unchecked(carrier, field);
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
}

#[cfg(feature = "alloc")]
unsafe impl<'d, 'a: 'd, T: ReprC<CType: CType + Sized> + 'a> DecodeMut<'d> for &'a mut T
where
    Self: RustSpec<Layout = Unstable> + DecodeMove<'d, CType = CRefMut<T, true>>,
    &'d mut Self: ReprC<CType = CRefMut<Self, true>>,
{
    type Store = RefMutSizedDecodeStore<Self, Box<<Self as DecodeMove<'d>>::Store>, *mut T>;
    unsafe fn decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        let carrier = unsafe { *source.as_ptr().as_ref()? };
        unsafe {
            store.decode_with(
                |field| {
                    let value = Self::soft_decode(carrier, &mut **field)?;
                    let pointer = core::ptr::from_mut(&mut *value);
                    Some((value, pointer))
                },
                |value, pointer| core::ptr::eq(core::ptr::from_mut(value), pointer).then_some(()),
            )
        }
    }
    unsafe fn decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> &'d mut Self {
        let carrier = unsafe { *source.as_ptr() };
        unsafe {
            store.decode_with(
                |field| {
                    let value = Self::soft_decode_unchecked(carrier, &mut **field);
                    let pointer = core::ptr::from_mut(&mut *value);
                    Some((value, pointer))
                },
                |value, pointer| core::ptr::eq(core::ptr::from_mut(value), pointer).then_some(()),
            )
        }
        .expect("unchecked decoding produces a value")
    }
}

// Decode the transparent borrowed carrier, never an owning copy of a field.
pub(crate) unsafe fn decode_mut_field<'d, 'itm: 'd, T>(
    source: <T as ReprC>::CType,
    store: &'itm mut <Borrowed<T> as DecodeMove<'d>>::Store,
) -> Option<T>
where
    T: ReprC<CType: Sized>,
    Borrowed<T>: DecodeMove<'d> + ReprC<CType = Borrowed<<T as ReprC>::CType>>,
{
    unsafe { Borrowed::<T>::soft_decode(Borrowed::new(source), store) }.map(Borrowed::into_inner)
}

pub(crate) unsafe fn decode_mut_field_unchecked<'d, 'itm: 'd, T>(
    source: <T as ReprC>::CType,
    store: &'itm mut <Borrowed<T> as DecodeMove<'d>>::Store,
) -> T
where
    T: ReprC<CType: Sized>,
    Borrowed<T>: DecodeMove<'d> + ReprC<CType = Borrowed<<T as ReprC>::CType>>,
{
    Borrowed::into_inner(unsafe {
        Borrowed::<T>::soft_decode_unchecked(Borrowed::new(source), store)
    })
}
