//! Referent-specific decoding and mutable writeback for unstable layouts.
#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};

use disjoint_impls::disjoint_impls;
use rust_spec::RustSpec;
#[cfg(feature = "alloc")]
use rust_spec::{
    mutability::{Exclusive, ExclusiveKind},
    size::{MetaSized, SliceLike},
};

use crate::{
    CType, ReprC,
    convert::{DecodeMove, EmptyStore, EncodeMove, Store},
    ffi::NulTerminatedBuf,
    niche::Niche,
    option::{COption, COptionSome},
    reference::{CSoftRef, CSoftRefMut, MutRefCarrier, SharedRefCarrier as _},
    result::{CResult, CResultErr, CResultOk},
    sync::{ArrayStore, RefMutSizedDecodeStore, RefSizedDecodeStore, TakeDecoded},
};
#[cfg(feature = "alloc")]
use crate::{
    boxed::CBox,
    sync::RefMutSliceDecodeStore,
    vec::CVec,
    wide::{CWideBox, PointeeData, WideRefCarrier},
};

// Decode the field through the ordinary reference path, then obtain its value
// using the adapter's stable-clone or unstable-extraction implementation.
pub(crate) unsafe fn decode_ref_field<'d, 'itm: 'd, T: ReprC + RustSpec + 'd>(
    source: <&'d T as ReprC>::CType,
    store: &'itm mut <&'d T as DecodeMove<'d>>::Store,
) -> Option<T>
where
    &'d T: DecodeMove<'d> + ReprC<CType: Sized>,
    crate::sync::DecodedField<'d, T, <&'d T as DecodeMove<'d>>::Store, T::Layout>:
        TakeDecoded<Value = T>,
{
    let pointer = core::ptr::from_mut(store);
    let value = unsafe { <&'d T>::soft_decode(source, store)? };
    let mut field = unsafe { crate::sync::DecodedField::<_, _, T::Layout>::new(value, pointer) };
    unsafe { TakeDecoded::take_decoded(&mut field) }
}

pub(crate) unsafe fn decode_ref_field_unchecked<'d, 'itm: 'd, T: ReprC + RustSpec + 'd>(
    source: <&'d T as ReprC>::CType,
    store: &'itm mut <&'d T as DecodeMove<'d>>::Store,
) -> T
where
    &'d T: DecodeMove<'d> + ReprC<CType: Sized>,
    crate::sync::DecodedField<'d, T, <&'d T as DecodeMove<'d>>::Store, T::Layout>:
        TakeDecoded<Value = T>,
{
    let pointer = core::ptr::from_mut(store);
    let value = unsafe { <&'d T>::soft_decode_unchecked(source, store) };
    let mut field = unsafe { crate::sync::DecodedField::<_, _, T::Layout>::new(value, pointer) };
    unsafe { TakeDecoded::take_decoded(&mut field) }.expect("unchecked decoding produces a value")
}

disjoint_impls! {
    /// Decodes a shared reference from its carrier using conversion storage.
    ///
    /// # Safety
    /// Implementations must preserve the carrier's borrowing and ownership rules.
    /// The decoded reference may borrow from the input storage or the conversion
    /// store, both of which must remain valid for `'d`. An `EmptyStore` must not
    /// back any returned reference. Shared interior-mutable contents must never
    /// be read during conversion or validation.
    pub unsafe trait SoftDecodeRef<'d>: ReprC + 'd {
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

        /// Decode a reference whose value validity is guaranteed by the caller.
        ///
        /// # Safety
        /// The source must satisfy `crate::soft_decode_unchecked`, including
        /// validity, lifetime and aliasing requirements.
        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self
        where
            &'d Self: ReprC<CType: Sized>,
        {
            unsafe { Self::soft_decode_ref(source, store) }.unwrap()
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: 'd, S: rust_spec::size::SizedKind>
        SoftDecodeRef<'d> for Box<T>
    where
        Self: ReprC<CType = CBox<T>>,
        T: ReprC<CType: CType + Sized> + RustSpec<Size = rust_spec::size::Sized<S>>,
        &'d Self: ReprC<CType = CSoftRef<Self>>,
        &'d T: DecodeMove<'d, Store: Store + Default + Sized + TakeDecoded<Value = T>>
            + ReprC<CType: crate::reference::SharedRefCarrier<T>>,
    {
        type Store = RefSizedDecodeStore<Self, <&'d T as DecodeMove<'d>>::Store>;

        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let header = unsafe { *source.into_ref()? };
            let carrier =
                unsafe { crate::reference::SharedRefCarrier::from_shared_ptr(header.as_ptr()) };
            store.decode(|field| {
                let pointer = core::ptr::from_mut(field);
                let _ = unsafe { <&'d T>::soft_decode(carrier, field)? };
                let value = unsafe { TakeDecoded::take_decoded(pointer)? };
                Some(Box::new(value))
            })
        }
        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let header = unsafe { *source.into_ref().unwrap_unchecked() };
            let carrier =
                unsafe { crate::reference::SharedRefCarrier::from_shared_ptr(header.as_ptr()) };
            store
                .decode(|field| {
                    let pointer = core::ptr::from_mut(field);
                    let _ = unsafe { <&'d T>::soft_decode_unchecked(carrier, field) };
                    let value = unsafe { TakeDecoded::take_decoded(pointer) }
                        .expect("unchecked decoding produces a value");
                    Some(Box::new(value))
                })
                .expect("unchecked decoding produces a value")
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: ?Sized + alloc::borrow::ToOwned + 'd> SoftDecodeRef<'d> for Box<T>
    where
        Self: ReprC<CType = CWideBox<T>>,
        T: ReprC<
                CType: crate::wide::Wide<
                    Header: crate::wide::WideHeader<Data: CType>,
                    Metadata = usize,
                >,
            > + RustSpec<
                Layout = rust_spec::Stable,
                Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>,
            >,
        &'d Self: ReprC<CType = CSoftRef<Self>>,
        &'d T: DecodeMove<'d, Store: Store + Default + Sized>
            + ReprC<CType: WideRefCarrier<T, Pointer = *const PointeeData<<T as ReprC>::CType>>>,
        Self: From<<T as alloc::borrow::ToOwned>::Owned>,
    {
        type Store = RefSizedDecodeStore<Self, <&'d T as DecodeMove<'d>>::Store>;
        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let header = unsafe { *source.into_ref()? };
            let carrier =
                unsafe { WideRefCarrier::from_parts(header.as_ptr().cast_const(), header.metadata()) };
            store.decode(|field| {
                let value = unsafe { <&'d T>::soft_decode(carrier, field)? };
                Some(alloc::borrow::ToOwned::to_owned(value).into())
            })
        }
        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let header = unsafe { *source.into_ref().unwrap_unchecked() };
            let carrier =
                unsafe { WideRefCarrier::from_parts(header.as_ptr().cast_const(), header.metadata()) };
            store
                .decode(|field| {
                    let value = unsafe { <&'d T>::soft_decode_unchecked(carrier, field) };
                    Some(alloc::borrow::ToOwned::to_owned(value).into())
                })
                .expect("unchecked decoding produces a value")
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: ?Sized + 'd> SoftDecodeRef<'d> for Box<T>
    where
        Self: ReprC<CType = CWideBox<T>>,
        T: ReprC<
                CType: crate::wide::Wide<
                    Header: crate::wide::WideHeader<Data: CType>,
                    Metadata = usize,
                >,
            > + RustSpec<
                Layout = rust_spec::Unstable,
                Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>,
            >,
        &'d Self: ReprC<CType = CSoftRef<Self>>,
        &'d T: DecodeMove<'d, Store: Store + Default + Sized + TakeDecoded>
            + ReprC<CType: WideRefCarrier<T, Pointer = *const PointeeData<<T as ReprC>::CType>>>,
        Self: From<<<&'d T as DecodeMove<'d>>::Store as TakeDecoded>::Value>,
    {
        type Store = RefSizedDecodeStore<Self, <&'d T as DecodeMove<'d>>::Store>;
        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let header = unsafe { *source.into_ref()? };
            let carrier =
                unsafe { WideRefCarrier::from_parts(header.as_ptr().cast_const(), header.metadata()) };
            store.decode(|field| {
                let pointer = core::ptr::from_mut(field);
                let _ = unsafe { <&'d T>::soft_decode(carrier, field)? };
                let value = unsafe { TakeDecoded::take_decoded(pointer)? };
                Some(value.into())
            })
        }
        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let header = unsafe { *source.into_ref().unwrap_unchecked() };
            let carrier =
                unsafe { WideRefCarrier::from_parts(header.as_ptr().cast_const(), header.metadata()) };
            store
                .decode(|field| {
                    let pointer = core::ptr::from_mut(field);
                    let _ = unsafe { <&'d T>::soft_decode_unchecked(carrier, field) };
                    let value = unsafe { TakeDecoded::take_decoded(pointer) }
                        .expect("unchecked decoding produces a value");
                    Some(value.into())
                })
                .expect("unchecked decoding produces a value")
        }
    }

    unsafe impl<'d, T: ReprC<CType: CType + Copy> + RustSpec<Layout: Sized> + 'd>
        SoftDecodeRef<'d> for Option<T>
    where
        crate::sync::DecodedField<
            'd, T, <&'d T as DecodeMove<'d>>::Store, <T as RustSpec>::Layout,
        >: TakeDecoded<Value = T>,
        Self: ReprC<CType = COption<T>>,
        &'d Self: ReprC<CType = CSoftRef<Self>>,
        T: RustSpec<Niche = rust_spec::niche::WithoutNiche>,
        &'d T: DecodeMove<'d, Store: Store + Default + Sized>
            + ReprC<CType: crate::reference::SharedRefCarrier<T>>,
    {
        type Store = RefSizedDecodeStore<Self, Option<<&'d T as DecodeMove<'d>>::Store>>;
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
                    let value = unsafe {
                        decode_ref_field::<T>(
                            crate::reference::SharedRefCarrier::from_shared_ptr(value),
                            field,
                        )
                    }?;
                    Some(Self::Some(value))
                }
            })
        }
        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
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
                        let value = unsafe {
                            decode_ref_field_unchecked::<T>(
                                crate::reference::SharedRefCarrier::from_shared_ptr(value),
                                field,
                            )
                        };
                        Some(Self::Some(value))
                    }
                })
                .expect("unchecked decoding produces a value")
        }
    }

    unsafe impl<
        'd,
        T: ReprC<CType: CType + Copy> + RustSpec<Layout: Sized> + 'd,
        E: ReprC<CType: CType + Copy> + RustSpec<Layout: Sized> + 'd,
        K: rust_spec::size::SizedKind,
    > SoftDecodeRef<'d> for Result<T, E>
    where
        crate::sync::DecodedField<
            'd, E, <&'d E as DecodeMove<'d>>::Store, <E as RustSpec>::Layout,
        >: TakeDecoded<Value = E>,
        crate::sync::DecodedField<
            'd, T, <&'d T as DecodeMove<'d>>::Store, <T as RustSpec>::Layout,
        >: TakeDecoded<Value = T>,
        Self: ReprC<CType = CResult<T, E>>,
        &'d Self: ReprC<CType = CSoftRef<Self>>,
        T: RustSpec<Size = rust_spec::size::Sized<K>>,
        E: RustSpec<Size = rust_spec::size::Sized<K>>,
        &'d T: DecodeMove<'d, Store: Store + Default + Sized>
            + ReprC<CType: crate::reference::SharedRefCarrier<T>>,
        &'d E: DecodeMove<'d, Store: Store + Default + Sized>
            + ReprC<CType: crate::reference::SharedRefCarrier<E>>,
    {
        type Store = RefSizedDecodeStore<
            Self,
            Option<Result<<&'d T as DecodeMove<'d>>::Store, <&'d E as DecodeMove<'d>>::Store>>,
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
                    let value = unsafe {
                        decode_ref_field::<T>(
                            crate::reference::SharedRefCarrier::from_shared_ptr(value),
                            field,
                        )
                    }?;
                    Some(Self::Ok(value))
                }
                Err(value) => {
                    let Err(field) = fields.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    let value = unsafe {
                        decode_ref_field::<E>(
                            crate::reference::SharedRefCarrier::from_shared_ptr(value),
                            field,
                        )
                    }?;
                    Some(Self::Err(value))
                }
            })
        }
        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
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
                        let value = unsafe {
                            decode_ref_field_unchecked::<T>(
                                crate::reference::SharedRefCarrier::from_shared_ptr(value),
                                field,
                            )
                        };
                        Some(Self::Ok(value))
                    }
                    Err(value) => {
                        let Err(field) = fields.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        let value = unsafe {
                            decode_ref_field_unchecked::<E>(
                                crate::reference::SharedRefCarrier::from_shared_ptr(value),
                                field,
                            )
                        };
                        Some(Self::Err(value))
                    }
                })
                .expect("unchecked decoding produces a value")
        }
    }

    unsafe impl<'d, T: Niche<CType: CType + Sized> + RustSpec<Layout: Sized> + 'd, N: rust_spec::layout::LayoutKind>
        SoftDecodeRef<'d> for Option<T>
    where
        crate::sync::DecodedField<
            'd, T, <&'d T as DecodeMove<'d>>::Store, <T as RustSpec>::Layout,
        >: TakeDecoded<Value = T>,
        T: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<CType = COptionSome<T>>,
        &'d Self: ReprC<CType = CSoftRef<Self>>,
        &'d T: DecodeMove<'d, Store: Store + Default + Sized>
            + ReprC<CType: crate::reference::SharedRefCarrier<T>>,
    {
        type Store = RefSizedDecodeStore<Self, Option<<&'d T as DecodeMove<'d>>::Store>>;
        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let value = unsafe { source.into_ref()? };
            store.decode(|field| {
                if T::is_niche(&value.0) {
                    *field = None;
                    return Some(None);
                }
                let value = unsafe {
                    decode_ref_field::<T>(
                        crate::reference::SharedRefCarrier::from_shared_ptr(&value.0),
                        field.insert(Default::default()),
                    )
                }?;
                Some(Some(value))
            })
        }
        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let value = unsafe { source.into_ref().unwrap_unchecked() };
            store
                .decode(|field| {
                    if T::is_niche(&value.0) {
                        *field = None;
                        return Some(None);
                    }
                    let value = unsafe {
                        decode_ref_field_unchecked::<T>(
                            crate::reference::SharedRefCarrier::from_shared_ptr(&value.0),
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
        T: Niche<CType: CType + Sized> + RustSpec<Layout: Sized> + 'd,
        N: rust_spec::layout::LayoutKind,
        E: Default + ReprC + 'd,
    > SoftDecodeRef<'d> for Result<T, E>
    where
        crate::sync::DecodedField<
            'd, T, <&'d T as DecodeMove<'d>>::Store, <T as RustSpec>::Layout,
        >: TakeDecoded<Value = T>,
        T: RustSpec<
                Niche = rust_spec::niche::WithNiche<N>,
                Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>,
            >,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Self: ReprC<CType = CResultOk<T, E>>,
        &'d Self: ReprC<CType = CSoftRef<Self>>,
        &'d T: DecodeMove<'d, Store: Store + Default + Sized>
            + ReprC<CType: crate::reference::SharedRefCarrier<T>>,
    {
        type Store = RefSizedDecodeStore<Self, Option<<&'d T as DecodeMove<'d>>::Store>>;
        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let value = unsafe { source.into_ref()? };
            store.decode(|field| {
                if T::is_niche(&value.0) {
                    *field = None;
                    return Some(Err(E::default()));
                }
                let value = unsafe {
                    decode_ref_field::<T>(
                        crate::reference::SharedRefCarrier::from_shared_ptr(&value.0),
                        field.insert(Default::default()),
                    )
                }?;
                Some(Ok(value))
            })
        }
        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let value = unsafe { source.into_ref().unwrap_unchecked() };
            store
                .decode(|field| {
                    if T::is_niche(&value.0) {
                        *field = None;
                        return Some(Err(E::default()));
                    }
                    let value = unsafe {
                        decode_ref_field_unchecked::<T>(
                            crate::reference::SharedRefCarrier::from_shared_ptr(&value.0),
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
        E: Niche<CType: CType + Sized> + RustSpec<Layout: Sized> + 'd,
        N: rust_spec::layout::LayoutKind,
        T: Default + ReprC + 'd,
    > SoftDecodeRef<'d> for Result<T, E>
    where
        crate::sync::DecodedField<
            'd, E, <&'d E as DecodeMove<'d>>::Store, <E as RustSpec>::Layout,
        >: TakeDecoded<Value = E>,
        E: RustSpec<
                Niche = rust_spec::niche::WithNiche<N>,
                Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>,
            >,
        T: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Self: ReprC<CType = CResultErr<T, E>>,
        &'d Self: ReprC<CType = CSoftRef<Self>>,
        &'d E: DecodeMove<'d, Store: Store + Default + Sized>
            + ReprC<CType: crate::reference::SharedRefCarrier<E>>,
    {
        type Store = RefSizedDecodeStore<Self, Option<<&'d E as DecodeMove<'d>>::Store>>;
        unsafe fn soft_decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d Self> {
            let value = unsafe { source.into_ref()? };
            store.decode(|field| {
                if E::is_niche(&value.0) {
                    *field = None;
                    return Some(Ok(T::default()));
                }
                let value = unsafe {
                    decode_ref_field::<E>(
                        crate::reference::SharedRefCarrier::from_shared_ptr(&value.0),
                        field.insert(Default::default()),
                    )
                }?;
                Some(Err(value))
            })
        }
        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d Self {
            let value = unsafe { source.into_ref().unwrap_unchecked() };
            store
                .decode(|field| {
                    if E::is_niche(&value.0) {
                        *field = None;
                        return Some(Ok(T::default()));
                    }
                    let value = unsafe {
                        decode_ref_field_unchecked::<E>(
                            crate::reference::SharedRefCarrier::from_shared_ptr(&value.0),
                            field.insert(Default::default()),
                        )
                    };
                    Some(Err(value))
                })
                .expect("unchecked decoding produces a value")
        }
    }
    // Decode borrowed vector headers without taking ownership of their allocations.

}

// Null-terminated referents are implemented individually so downstream types
// can provide their own reference decoder.
macro_rules! impl_nul_decode_ref {
    ($ty:ty) => {
        unsafe impl<'d> SoftDecodeRef<'d> for $ty {
            type Store = ();

            unsafe fn soft_decode_ref<'itm: 'd>(
                source: <&'d Self as ReprC>::CType,
                store: &'itm mut (),
            ) -> Option<&'d Self> {
                if source.as_ptr().is_null() {
                    return None;
                }
                Some(unsafe { Self::soft_decode_ref_unchecked(source, store) })
            }

            unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
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
    /// Decodes a mutable reference from its carrier using conversion storage.
    ///
    /// # Safety
    /// Implementations must preserve the carrier's borrowing and ownership rules.
    /// The decoded reference may borrow from the input storage or the conversion
    /// store, both of which must remain valid for `'d`. An `EmptyStore` must not
    /// back any returned reference. Shared interior-mutable contents must never
    /// be read during conversion or validation.
    pub unsafe trait SoftDecodeMut<'d>: ReprC + 'd {
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

        /// Decode a reference whose value validity is guaranteed by the caller.
        ///
        /// # Safety
        /// The source must satisfy `crate::soft_decode_unchecked`, including
        /// validity, lifetime and aliasing requirements.
        unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self
        where
            &'d mut Self: ReprC<CType: Sized>,
        {
            unsafe { Self::soft_decode_mut(source, store) }.unwrap()
        }
    }

    unsafe impl<'d, T: ReprC<CType: CType + Copy> + 'd> SoftDecodeMut<'d> for Option<T>
    where
        T: crate::borrow::BorrowedDecode<'d>,
        Self: EncodeMove<Store: EmptyStore> + RustSpec + ReprC<CType = COption<T>>,
        &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
        T: RustSpec<Niche = rust_spec::niche::WithoutNiche>,
    {
        type Store =
            RefMutSizedDecodeStore<Self, Option<<T as crate::borrow::BorrowedDecode<'d>>::Store>>;
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
                        let value = decode_mut_field::<T>(*value, field)?;
                        Some(Self::Some(value))
                    }
                })
            }
        }
        unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
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
    > SoftDecodeMut<'d> for Result<T, E>
    where
        T: crate::borrow::BorrowedDecode<'d>,
        E: crate::borrow::BorrowedDecode<'d>,
        Self: EncodeMove<Store: EmptyStore> + RustSpec + ReprC<CType = CResult<T, E>>,
        &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
        T: RustSpec<Size = rust_spec::size::Sized<K>>,
        E: RustSpec<Size = rust_spec::size::Sized<K>>,
    {
        type Store = RefMutSizedDecodeStore<
            Self,
            Option<
                Result<
                    <T as crate::borrow::BorrowedDecode<'d>>::Store,
                    <E as crate::borrow::BorrowedDecode<'d>>::Store,
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
        unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
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

    unsafe impl<'d, T: Niche<CType: CType + Copy + Sized> + 'd, N: rust_spec::layout::LayoutKind>
        SoftDecodeMut<'d> for Option<T>
    where
        T: crate::borrow::BorrowedDecode<'d>,
        T: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: EncodeMove<Store: EmptyStore> + RustSpec + ReprC<CType = COptionSome<T>>,
        &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
    {
        type Store =
            RefMutSizedDecodeStore<Self, Option<<T as crate::borrow::BorrowedDecode<'d>>::Store>>;
        unsafe fn soft_decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let pointer = source.as_ptr();
            let value = unsafe { pointer.as_ref()? };
            unsafe {
                store.decode(pointer, |field| {
                    if T::is_niche(&value.0) {
                        *field = None;
                        return Some(None);
                    }
                    let value = decode_mut_field::<T>(value.0, field.insert(Default::default()))?;
                    Some(Some(value))
                })
            }
        }
        unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self {
            let pointer = source.as_ptr();
            let value = unsafe { &*pointer };
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
    unsafe impl<
        'd,
        T: Niche<CType: CType + Copy + Sized> + 'd,
        N: rust_spec::layout::LayoutKind,
        E: Default + ReprC + 'd,
    > SoftDecodeMut<'d> for Result<T, E>
    where
        T: crate::borrow::BorrowedDecode<'d>,
        T: RustSpec<
                Niche = rust_spec::niche::WithNiche<N>,
                Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>,
            >,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Self: EncodeMove<Store: EmptyStore> + RustSpec + ReprC<CType = CResultOk<T, E>>,
        &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
    {
        type Store =
            RefMutSizedDecodeStore<Self, Option<<T as crate::borrow::BorrowedDecode<'d>>::Store>>;
        unsafe fn soft_decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let pointer = source.as_ptr();
            let value = unsafe { pointer.as_ref()? };
            unsafe {
                store.decode(pointer, |field| {
                    if T::is_niche(&value.0) {
                        *field = None;
                        return Some(Err(E::default()));
                    }
                    let value = decode_mut_field::<T>(value.0, field.insert(Default::default()))?;
                    Some(Ok(value))
                })
            }
        }
        unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self {
            let pointer = source.as_ptr();
            let value = unsafe { &*pointer };
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
    unsafe impl<
        'd,
        E: Niche<CType: CType + Copy + Sized> + 'd,
        N: rust_spec::layout::LayoutKind,
        T: Default + ReprC + 'd,
    > SoftDecodeMut<'d> for Result<T, E>
    where
        E: crate::borrow::BorrowedDecode<'d>,
        E: RustSpec<
                Niche = rust_spec::niche::WithNiche<N>,
                Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>,
            >,
        T: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Self: EncodeMove<Store: EmptyStore> + RustSpec + ReprC<CType = CResultErr<T, E>>,
        &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
    {
        type Store =
            RefMutSizedDecodeStore<Self, Option<<E as crate::borrow::BorrowedDecode<'d>>::Store>>;
        unsafe fn soft_decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> Option<&'d mut Self> {
            let pointer = source.as_ptr();
            let value = unsafe { pointer.as_ref()? };
            unsafe {
                store.decode(pointer, |field| {
                    if E::is_niche(&value.0) {
                        *field = None;
                        return Some(Ok(T::default()));
                    }
                    let value = decode_mut_field::<E>(value.0, field.insert(Default::default()))?;
                    Some(Err(value))
                })
            }
        }
        unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self {
            let pointer = source.as_ptr();
            let value = unsafe { &*pointer };
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

    // Only carriers are copied; Rust elements are decoded through Borrowed<T>.
    // Retain each element's decode store and re-encode the edited array on sync.

    // There is no reconstructing shared decoder for Cell/UnsafeCell: reading their
    // contents through a shared reference is forbidden. Stable cell references use
    // the direct DecodeMove path. Likewise, reconstructing &&mut T would duplicate
    // an exclusive reference. COption/CResult are stable identity carriers and use
    // the direct reference path rather than an unstable referent decoder.

}

#[cfg(feature = "alloc")]
unsafe impl<'d, R: ReprC<CType: CType + Sized + 'd> + RustSpec + 'd, __Interior: ExclusiveKind>
    SoftDecodeRef<'d> for [R]
where
    crate::sync::DecodedField<'d, R, <&'d R as DecodeMove<'d>>::Store, <R as RustSpec>::Layout>:
        TakeDecoded<Value = R>,
    [R]: RustSpec<Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>,
    &'d Self: ReprC<CType = crate::CSoftWideRef<Self>>,
    &'d R: DecodeMove<'d, Store: Store + Default + Sized>
        + ReprC<CType: crate::reference::SharedRefCarrier<R>>,
{
    type Store = RefSizedDecodeStore<Vec<R>, Box<[<&'d R as DecodeMove<'d>>::Store]>>;

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
                    let carrier =
                        unsafe { crate::reference::SharedRefCarrier::from_shared_ptr(value) };
                    let value = unsafe { decode_ref_field::<R>(carrier, field_store)? };
                    values.push(value);
                }
                Some(values)
            })
            .map(Vec::as_slice)
    }

    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
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
                    let carrier =
                        unsafe { crate::reference::SharedRefCarrier::from_shared_ptr(value) };
                    let value = unsafe { decode_ref_field_unchecked::<R>(carrier, field_store) };
                    values.push(value);
                }
                Some(values)
            })
            .expect("unchecked decoding produces a value")
            .as_slice()
    }
}

unsafe impl<'d, T: ReprC<CType: CType + Copy> + RustSpec + 'd> SoftDecodeRef<'d>
    for crate::option::Option<T>
where
    crate::sync::DecodedField<'d, T, <&'d T as DecodeMove<'d>>::Store, <T as RustSpec>::Layout>:
        TakeDecoded<Value = T>,
    Self: ReprC<CType = COption<T, true>>,
    &'d Self: ReprC<CType = CSoftRef<Self>>,
    &'d T: DecodeMove<'d, Store: Store + Default + Sized>
        + ReprC<CType: crate::reference::SharedRefCarrier<T>>,
{
    type Store = RefSizedDecodeStore<Self, Option<<&'d T as DecodeMove<'d>>::Store>>;
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
                let value = unsafe {
                    decode_ref_field::<T>(
                        crate::reference::SharedRefCarrier::from_shared_ptr(value),
                        field,
                    )
                }?;
                Some(Self::Some(value))
            }
        })
    }
    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
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
                    let value = unsafe {
                        decode_ref_field_unchecked::<T>(
                            crate::reference::SharedRefCarrier::from_shared_ptr(value),
                            field,
                        )
                    };
                    Some(Self::Some(value))
                }
            })
            .expect("unchecked decoding produces a value")
    }
}

unsafe impl<
    'd,
    T: ReprC<CType: CType + Copy> + RustSpec + 'd,
    E: ReprC<CType: CType + Copy> + RustSpec + 'd,
> SoftDecodeRef<'d> for crate::result::Result<T, E>
where
    crate::sync::DecodedField<'d, E, <&'d E as DecodeMove<'d>>::Store, <E as RustSpec>::Layout>:
        TakeDecoded<Value = E>,
    crate::sync::DecodedField<'d, T, <&'d T as DecodeMove<'d>>::Store, <T as RustSpec>::Layout>:
        TakeDecoded<Value = T>,
    Self: ReprC<CType = CResult<T, E, true>>,
    &'d Self: ReprC<CType = CSoftRef<Self>>,
    &'d T: DecodeMove<'d, Store: Store + Default + Sized>
        + ReprC<CType: crate::reference::SharedRefCarrier<T>>,
    &'d E: DecodeMove<'d, Store: Store + Default + Sized>
        + ReprC<CType: crate::reference::SharedRefCarrier<E>>,
{
    type Store = RefSizedDecodeStore<
        Self,
        Option<Result<<&'d T as DecodeMove<'d>>::Store, <&'d E as DecodeMove<'d>>::Store>>,
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
                let value = unsafe {
                    decode_ref_field::<T>(
                        crate::reference::SharedRefCarrier::from_shared_ptr(value),
                        field,
                    )
                }?;
                Some(Self::Ok(value))
            }
            Err(value) => {
                let Err(field) = fields.insert(Err(Default::default())) else {
                    unreachable!()
                };
                let value = unsafe {
                    decode_ref_field::<E>(
                        crate::reference::SharedRefCarrier::from_shared_ptr(value),
                        field,
                    )
                }?;
                Some(Self::Err(value))
            }
        })
    }
    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
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
                    let value = unsafe {
                        decode_ref_field_unchecked::<T>(
                            crate::reference::SharedRefCarrier::from_shared_ptr(value),
                            field,
                        )
                    };
                    Some(Self::Ok(value))
                }
                Err(value) => {
                    let Err(field) = fields.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    let value = unsafe {
                        decode_ref_field_unchecked::<E>(
                            crate::reference::SharedRefCarrier::from_shared_ptr(value),
                            field,
                        )
                    };
                    Some(Self::Err(value))
                }
            })
            .expect("unchecked decoding produces a value")
    }
}

// Build an independently owned Vec, copying stable borrowed elements and
// extracting reconstructed elements from their stores.
#[cfg(feature = "alloc")]
unsafe fn decode_vec_ref<'d, 'itm: 'd, T, const STABLE: bool>(
    header: CVec<T, STABLE>,
    fields: &'itm mut Box<[<&'d T as DecodeMove<'d>>::Store]>,
) -> Option<Vec<T>>
where
    crate::sync::DecodedField<'d, T, <&'d T as DecodeMove<'d>>::Store, <T as RustSpec>::Layout>:
        TakeDecoded<Value = T>,
    T: ReprC<CType: CType + Sized> + RustSpec + 'd,
    &'d T: DecodeMove<'d, Store: Store + Default + Sized>
        + ReprC<CType: crate::reference::SharedRefCarrier<T>>,
{
    if header.as_ptr().is_null() || header.len() > header.capacity() {
        return None;
    }
    *fields = core::iter::repeat_with(Default::default)
        .take(header.len())
        .collect();
    let mut values = Vec::with_capacity(header.capacity());
    for (index, field) in fields.iter_mut().enumerate() {
        let carrier = unsafe {
            crate::reference::SharedRefCarrier::from_shared_ptr(header.as_ptr().add(index))
        };
        let value = unsafe { decode_ref_field::<T>(carrier, field)? };
        values.push(value);
    }
    Some(values)
}

#[cfg(feature = "alloc")]
unsafe fn decode_vec_ref_unchecked<'d, 'itm: 'd, T, const STABLE: bool>(
    header: CVec<T, STABLE>,
    fields: &'itm mut Box<[<&'d T as DecodeMove<'d>>::Store]>,
) -> Vec<T>
where
    crate::sync::DecodedField<'d, T, <&'d T as DecodeMove<'d>>::Store, <T as RustSpec>::Layout>:
        TakeDecoded<Value = T>,
    T: ReprC<CType: CType + Sized> + RustSpec + 'd,
    &'d T: DecodeMove<'d, Store: Store + Default + Sized>
        + ReprC<CType: crate::reference::SharedRefCarrier<T>>,
{
    *fields = core::iter::repeat_with(Default::default)
        .take(header.len())
        .collect();
    let mut values = Vec::with_capacity(header.capacity());
    for (index, field) in fields.iter_mut().enumerate() {
        let carrier = unsafe {
            crate::reference::SharedRefCarrier::from_shared_ptr(header.as_ptr().add(index))
        };
        let value = unsafe { decode_ref_field_unchecked::<T>(carrier, field) };
        values.push(value);
    }
    values
}

#[cfg(feature = "alloc")]
unsafe impl<'d> SoftDecodeRef<'d> for alloc::string::String {
    type Store = RefSizedDecodeStore<Self, Box<[<&'d u8 as DecodeMove<'d>>::Store]>>;

    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        // Utf8String is Copy; only the header is read here.
        let header = unsafe { *source.into_ref()? };
        store.decode(|fields| {
            let bytes = unsafe { decode_vec_ref(header.0, fields)? };
            Self::from_utf8(bytes).ok()
        })
    }

    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> &'d Self {
        let header = unsafe { *source.into_ref().unwrap_unchecked() };
        store
            .decode(|fields| {
                let bytes = unsafe { decode_vec_ref_unchecked(header.0, fields) };
                // The caller guarantees UTF-8 validity; the cloned vector is
                // independently owned and becomes the stored String.
                Some(unsafe { Self::from_utf8_unchecked(bytes) })
            })
            .expect("unchecked decoding produces a value")
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d, T: ReprC<CType: CType + Sized> + RustSpec + 'd> SoftDecodeRef<'d>
    for alloc::vec::Vec<T>
where
    crate::sync::DecodedField<'d, T, <&'d T as DecodeMove<'d>>::Store, <T as RustSpec>::Layout>:
        TakeDecoded<Value = T>,
    Self: ReprC<CType = CVec<T>>,
    &'d Self: ReprC<CType = CSoftRef<Self>>,
    &'d T: DecodeMove<'d, Store: Store + Default + Sized>
        + ReprC<CType: crate::reference::SharedRefCarrier<T>>,
{
    type Store = RefSizedDecodeStore<Self, Box<[<&'d T as DecodeMove<'d>>::Store]>>;

    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        // CVec is Copy; only the header is read here.
        let header = unsafe { *source.into_ref()? };
        store.decode(|fields| unsafe { decode_vec_ref(header, fields) })
    }

    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> &'d Self {
        let header = unsafe { *source.into_ref().unwrap_unchecked() };
        store
            .decode(|fields| Some(unsafe { decode_vec_ref_unchecked(header, fields) }))
            .expect("unchecked decoding produces a value")
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d, T: ReprC<CType: CType + Sized> + RustSpec + 'd> SoftDecodeRef<'d>
    for crate::vec::Vec<T>
where
    crate::sync::DecodedField<'d, T, <&'d T as DecodeMove<'d>>::Store, <T as RustSpec>::Layout>:
        TakeDecoded<Value = T>,
    Self: ReprC<CType = CVec<T, true>>,
    &'d Self: ReprC<CType = CSoftRef<Self>>,
    &'d T: DecodeMove<'d, Store: Store + Default + Sized>
        + ReprC<CType: crate::reference::SharedRefCarrier<T>>,
{
    type Store = RefSizedDecodeStore<Self, Box<[<&'d T as DecodeMove<'d>>::Store]>>;

    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        let header = unsafe { *source.into_ref()? };
        store.decode(|fields| unsafe { decode_vec_ref(header, fields).map(Into::into) })
    }

    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> &'d Self {
        let header = unsafe { *source.into_ref().unwrap_unchecked() };
        store
            .decode(|fields| Some(unsafe { decode_vec_ref_unchecked(header, fields) }.into()))
            .expect("unchecked decoding produces a value")
    }
}

unsafe impl<'d, T: ReprC<CType: CType + Sized + 'd> + RustSpec + 'd, const N: usize>
    SoftDecodeRef<'d> for [T; N]
where
    crate::sync::DecodedField<'d, T, <&'d T as DecodeMove<'d>>::Store, <T as RustSpec>::Layout>:
        TakeDecoded<Value = T>,
    Self: ReprC<CType = [<T as ReprC>::CType; N]>,
    &'d Self: ReprC<CType = CSoftRef<Self>>,
    &'d T: DecodeMove<'d, Store: Store + Default + Sized>
        + ReprC<CType: crate::reference::SharedRefCarrier<T>>,
{
    type Store = RefSizedDecodeStore<Self, ArrayStore<<&'d T as DecodeMove<'d>>::Store, N>>;

    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        let source: &'d [<T as ReprC>::CType; N] = unsafe { source.into_ref()? };
        store.decode(|fields| {
            let mut fields = fields.0.iter_mut();
            let values: [Option<T>; N] = core::array::from_fn(|i| {
                let carrier =
                    unsafe { crate::reference::SharedRefCarrier::from_shared_ptr(&source[i]) };
                unsafe { decode_ref_field::<T>(carrier, fields.next().unwrap()) }
            });
            if values.iter().any(Option::is_none) {
                return None;
            }
            Some(values.map(Option::unwrap))
        })
    }

    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> &'d Self {
        let source: &'d [<T as ReprC>::CType; N] = unsafe { source.into_ref().unwrap_unchecked() };
        store
            .decode(|fields| {
                let mut fields = fields.0.iter_mut();
                Some(core::array::from_fn(|i| {
                    let carrier =
                        unsafe { crate::reference::SharedRefCarrier::from_shared_ptr(&source[i]) };
                    unsafe { decode_ref_field_unchecked::<T>(carrier, fields.next().unwrap()) }
                }))
            })
            .expect("unchecked decoding produces a value")
    }
}

unsafe impl<'d, T: ReprC<CType: CType + Sized> + 'd> SoftDecodeRef<'d>
    for core::mem::ManuallyDrop<T>
where
    Self: ReprC<CType: CType + Sized>,
    &'d Self: ReprC<CType = CSoftRef<Self>>,
    &'d T: DecodeMove<'d> + ReprC<CType: crate::reference::SharedRefCarrier<T>>,
{
    type Store = <&'d T as DecodeMove<'d>>::Store;
    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { source.cast_shared::<T, _>() };
        let inner = unsafe { <&'d T>::soft_decode(source, store)? };
        // Both Rust and C wrappers are transparent; this does not read the value.
        Some(unsafe { &*core::ptr::from_ref(inner).cast::<Self>() })
    }
    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> &'d Self {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { source.cast_shared::<T, _>() };
        let inner = unsafe { <&'d T>::soft_decode_unchecked(source, store) };
        unsafe { &*core::ptr::from_ref(inner).cast::<Self>() }
    }
}

unsafe impl<'d, 'a: 'd, T: ?Sized + 'a> SoftDecodeRef<'d> for &'a T
where
    Self: DecodeMove<'d, CType: Copy>,
    &'d Self: ReprC<CType = CSoftRef<Self>>,
{
    type Store = RefSizedDecodeStore<Self, <Self as DecodeMove<'d>>::Store>;
    unsafe fn soft_decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d Self> {
        let carrier = unsafe { *source.into_ref()? };
        store.decode(|field| unsafe { Self::soft_decode(carrier, field) })
    }
    unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> &'d Self {
        let carrier = unsafe { *source.into_ref().unwrap_unchecked() };
        store
            .decode(|field| Some(unsafe { Self::soft_decode_unchecked(carrier, field) }))
            .expect("unchecked decoding produces a value")
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d, R: ReprC<CType: CType + Copy> + EncodeMove<Store: EmptyStore> + 'd>
    SoftDecodeMut<'d> for [R]
where
    R: crate::borrow::BorrowedDecode<'d>,
    &'d mut Self: ReprC<CType = crate::CSoftWideRefMut<Self, true>>,
    [R]: RustSpec<Size = MetaSized<SliceLike>>,
{
    type Store = RefMutSliceDecodeStore<R, Box<[<R as crate::borrow::BorrowedDecode<'d>>::Store]>>;

    #[inline(always)]
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut <Self as SoftDecodeMut<'d>>::Store,
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
    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut <Self as SoftDecodeMut<'d>>::Store,
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

unsafe impl<'d, T: ReprC<CType: CType + Copy> + 'd> SoftDecodeMut<'d> for crate::option::Option<T>
where
    T: crate::borrow::BorrowedDecode<'d>,
    Self: EncodeMove<Store: EmptyStore> + RustSpec + ReprC<CType = COption<T, true>>,
    &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
{
    type Store =
        RefMutSizedDecodeStore<Self, Option<<T as crate::borrow::BorrowedDecode<'d>>::Store>>;
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
                    let value = decode_mut_field::<T>(*value, field)?;
                    Some(Self::Some(value))
                }
            })
        }
    }
    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
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

unsafe impl<'d, T: ReprC<CType: CType + Copy> + 'd, E: ReprC<CType: CType + Copy> + 'd>
    SoftDecodeMut<'d> for crate::result::Result<T, E>
where
    T: crate::borrow::BorrowedDecode<'d>,
    E: crate::borrow::BorrowedDecode<'d>,
    Self: EncodeMove<Store: EmptyStore> + RustSpec + ReprC<CType = CResult<T, E, true>>,
    &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
{
    type Store = RefMutSizedDecodeStore<
        Self,
        Option<
            Result<
                <T as crate::borrow::BorrowedDecode<'d>>::Store,
                <E as crate::borrow::BorrowedDecode<'d>>::Store,
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
    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
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

#[cfg(feature = "alloc")]
unsafe impl<'d, T: ReprC<CType: CType + Sized> + Clone + 'd> SoftDecodeMut<'d> for Box<T>
where
    Self: RustSpec + ReprC<CType = CBox<T>>,
    &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
    &'d mut T: DecodeMove<'d> + ReprC<CType: MutRefCarrier<T, true>>,
{
    type Store = RefMutSizedDecodeStore<Self, Box<<&'d mut T as DecodeMove<'d>>::Store>, *mut T>;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        let header = unsafe { *source.as_ptr().as_ref()? };
        unsafe {
            store.decode_with(
                |fields| {
                    let carrier = <&'d mut T as ReprC>::CType::from_mut_ptr(header.as_ptr());
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
    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> &'d mut Self {
        let header = unsafe { *source.as_ptr().as_ref().unwrap_unchecked() };
        unsafe {
            store.decode_with(
                |fields| {
                    let carrier = <&'d mut T as ReprC>::CType::from_mut_ptr(header.as_ptr());
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
unsafe impl<'d, T: ReprC<CType: CType + Sized> + Clone + 'd> SoftDecodeMut<'d> for Box<[T]>
where
    Self: RustSpec + ReprC<CType = CWideBox<[T]>>,
    &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
    &'d mut T: DecodeMove<'d> + ReprC<CType: MutRefCarrier<T, true>>,
{
    type Store =
        RefMutSizedDecodeStore<Self, Box<[<&'d mut T as DecodeMove<'d>>::Store]>, Box<[*mut T]>>;
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
                        let carrier =
                            <&'d mut T as ReprC>::CType::from_mut_ptr(header.as_ptr().add(i));
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
    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
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
                        let carrier =
                            <&'d mut T as ReprC>::CType::from_mut_ptr(header.as_ptr().add(i));
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

unsafe impl<'d, T: ReprC<CType: CType + Copy> + 'd, const N: usize> SoftDecodeMut<'d> for [T; N]
where
    T: crate::borrow::BorrowedDecode<'d>,
    Self: RustSpec + ReprC<CType = [<T as ReprC>::CType; N]> + EncodeMove<Store: EmptyStore>,
    &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
{
    type Store = RefMutSizedDecodeStore<
        Self,
        ArrayStore<<T as crate::borrow::BorrowedDecode<'d>>::Store, N>,
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
                    values.map(|value| decode_mut_field::<T>(value, fields.next().unwrap()));
                if values.iter().any(Option::is_none) {
                    return None;
                }
                Some(values.map(Option::unwrap))
            })
        }
    }

    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
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

unsafe impl<'d, T: ReprC<CType: CType + Sized> + 'd> SoftDecodeMut<'d>
    for core::mem::ManuallyDrop<T>
where
    Self: RustSpec + ReprC<CType: CType + Sized>,
    &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
    &'d mut T: DecodeMove<'d> + ReprC<CType: MutRefCarrier<T, true>>,
{
    type Store = <&'d mut T as DecodeMove<'d>>::Store;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { <&'d mut T as ReprC>::CType::from_mut_ptr(source.as_ptr().cast()) };
        let inner = unsafe { <&'d mut T>::soft_decode(source, store)? };
        // The exclusive borrow permits delegation into a cell; no shared cell is read.
        Some(unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() })
    }
    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> &'d mut Self {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { <&'d mut T as ReprC>::CType::from_mut_ptr(source.as_ptr().cast()) };
        let inner = unsafe { <&'d mut T>::soft_decode_unchecked(source, store) };
        unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() }
    }
}

unsafe impl<'d, T: ReprC<CType: CType + Sized> + 'd> SoftDecodeMut<'d> for core::cell::Cell<T>
where
    Self: RustSpec + ReprC<CType: CType + Sized>,
    &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
    &'d mut T: DecodeMove<'d> + ReprC<CType: MutRefCarrier<T, true>>,
{
    type Store = <&'d mut T as DecodeMove<'d>>::Store;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { <&'d mut T as ReprC>::CType::from_mut_ptr(source.as_ptr().cast()) };
        let inner = unsafe { <&'d mut T>::soft_decode(source, store)? };
        // The exclusive borrow permits delegation into a cell; no shared cell is read.
        Some(unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() })
    }
    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> &'d mut Self {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { <&'d mut T as ReprC>::CType::from_mut_ptr(source.as_ptr().cast()) };
        let inner = unsafe { <&'d mut T>::soft_decode_unchecked(source, store) };
        unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() }
    }
}

unsafe impl<'d, T: ReprC<CType: CType + Sized> + 'd> SoftDecodeMut<'d> for core::cell::UnsafeCell<T>
where
    Self: RustSpec + ReprC<CType: CType + Sized>,
    &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
    &'d mut T: DecodeMove<'d> + ReprC<CType: MutRefCarrier<T, true>>,
{
    type Store = <&'d mut T as DecodeMove<'d>>::Store;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { <&'d mut T as ReprC>::CType::from_mut_ptr(source.as_ptr().cast()) };
        let inner = unsafe { <&'d mut T>::soft_decode(source, store)? };
        // The exclusive borrow permits delegation into a cell; no shared cell is read.
        Some(unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() })
    }
    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> &'d mut Self {
        // SAFETY: The wrapper preserves the inner representation and access kind.
        let source = unsafe { <&'d mut T as ReprC>::CType::from_mut_ptr(source.as_ptr().cast()) };
        let inner = unsafe { <&'d mut T>::soft_decode_unchecked(source, store) };
        unsafe { &mut *core::ptr::from_mut(inner).cast::<Self>() }
    }
}

unsafe impl<'d, 'a: 'd, T: ?Sized + 'a> SoftDecodeMut<'d> for &'a T
where
    Self: RustSpec + DecodeMove<'d, CType: Copy> + EncodeMove<Store: EmptyStore>,
    &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
{
    type Store = RefMutSizedDecodeStore<Self, <Self as DecodeMove<'d>>::Store>;
    unsafe fn soft_decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        let source = source.as_ptr();
        unsafe { store.decode(source, |field| Self::soft_decode(source.read(), field)) }
    }
    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
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
unsafe impl<'d, T: ReprC<CType: CType + Sized> + Clone + 'd> SoftDecodeMut<'d>
    for alloc::vec::Vec<T>
where
    Self: RustSpec + ReprC<CType = CVec<T>>,
    &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
    &'d mut T: DecodeMove<'d> + ReprC<CType: MutRefCarrier<T, true>>,
{
    type Store = RefMutSizedDecodeStore<
        Self,
        Box<[<&'d mut T as DecodeMove<'d>>::Store]>,
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
                        let carrier =
                            <&'d mut T as ReprC>::CType::from_mut_ptr(header.as_ptr().add(i));
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
    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
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
                        let carrier =
                            <&'d mut T as ReprC>::CType::from_mut_ptr(header.as_ptr().add(i));
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
unsafe impl<'d, T: ReprC<CType: CType + Sized> + Clone + 'd> SoftDecodeMut<'d>
    for crate::vec::Vec<T>
where
    Self: RustSpec + ReprC<CType = CVec<T, true>>,
    &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
    &'d mut T: DecodeMove<'d> + ReprC<CType: MutRefCarrier<T, true>>,
{
    type Store = RefMutSizedDecodeStore<
        Self,
        Box<[<&'d mut T as DecodeMove<'d>>::Store]>,
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
                        let carrier =
                            <&'d mut T as ReprC>::CType::from_mut_ptr(header.as_ptr().add(i));
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
    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
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
                        let carrier =
                            <&'d mut T as ReprC>::CType::from_mut_ptr(header.as_ptr().add(i));
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

#[cfg(feature = "alloc")]
unsafe impl<'d, 'a: 'd, T: ReprC<CType: CType + Sized> + 'a> SoftDecodeMut<'d> for &'a mut T
where
    Self: RustSpec + DecodeMove<'d, CType = CSoftRefMut<T, true>>,
    &'d mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
{
    type Store = RefMutSizedDecodeStore<Self, Box<<Self as DecodeMove<'d>>::Store>, *mut T>;
    unsafe fn soft_decode_mut<'itm: 'd>(
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
    unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
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
    store: &'itm mut <T as crate::borrow::BorrowedDecode<'d>>::Store,
) -> Option<T>
where
    T: ReprC<CType: Sized>,
    T: crate::borrow::BorrowedDecode<'d>,
{
    unsafe { <T as crate::borrow::BorrowedDecode<'d>>::soft_decode(source, store) }
}

pub(crate) unsafe fn decode_mut_field_unchecked<'d, 'itm: 'd, T>(
    source: <T as ReprC>::CType,
    store: &'itm mut <T as crate::borrow::BorrowedDecode<'d>>::Store,
) -> T
where
    T: ReprC<CType: Sized>,
    T: crate::borrow::BorrowedDecode<'d>,
{
    unsafe { <T as crate::borrow::BorrowedDecode<'d>>::soft_decode_unchecked(source, store) }
}

#[cfg(all(test, feature = "alloc", feature = "derive"))]
mod tests {
    use alloc::{boxed::Box, vec};

    use co3::{
        ReprC,
        convert::{DecodeMove, SoftDecodeMut, SoftDecodeRef, Store},
        reference::{CSoftRef, CSoftRefMut},
        rust_spec::RustSpec,
    };
    use static_assertions::assert_not_impl_any;

    #[test]
    fn borrowed_boxed_stable_slice_clones_without_consuming_source() {
        use co3::wide::CWideBox;

        assert_not_impl_any!(Box<[core::cell::Cell<u8>]>: SoftDecodeRef<'static>);
        for unchecked in [false, true] {
            let source = CWideBox::<[u8]>::from_box(vec![7, 11].into_boxed_slice());
            let mut store = Default::default();
            let decoded = if unchecked {
                unsafe {
                    <Box<[u8]> as crate::borrow::BorrowedDecode>::soft_decode_unchecked(
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
        use co3::{primitives::Bool, wide::CWideBox};

        for raw in [0, 1, 2] {
            let source = CWideBox::<[bool]>::from_box(vec![Bool(raw)].into_boxed_slice());
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
        use co3::{tuple::CTuple2, wide::CWideBox};

        for unchecked in [false, true] {
            let source = CWideBox::<[(u8, u16)]>::from_box(vec![CTuple2(7, 11)].into_boxed_slice());
            let mut store = Default::default();
            let decoded = if unchecked {
                unsafe {
                    <Box<[(u8, u16)]> as crate::borrow::BorrowedDecode>::soft_decode_unchecked(
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
            assert_eq!(original[0], CTuple2(7, 11));
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
        let value =
            unsafe { co3::soft_decode::<&Wrapper>(CSoftRef::from_ref(&source), &mut store) }
                .unwrap();
        assert_eq!(value.0.0, 7);
        store.sync().unwrap();

        let mut store = Default::default();
        let value = unsafe {
            co3::soft_decode_unchecked::<&Wrapper>(CSoftRef::from_ref(&source), &mut store)
        };
        assert_eq!(value.0.0, 7);
        store.sync().unwrap();

        let zero = unsafe { <Wrapper as ReprC>::CType::from_fields(0) };
        let mut store = Default::default();
        assert!(
            unsafe { co3::soft_decode::<&Wrapper>(CSoftRef::from_ref(&zero), &mut store) }
                .is_none()
        );
        assert!(unsafe { co3::soft_decode::<&Wrapper>(CSoftRef::null(), &mut store) }.is_none());

        let source = unsafe { <EnumWrapper as ReprC>::CType::from_fields(9) };
        let mut store = Default::default();
        let value =
            unsafe { co3::soft_decode::<&EnumWrapper>(CSoftRef::from_ref(&source), &mut store) }
                .unwrap();
        let EnumWrapper::Value(field) = value;
        assert_eq!(field.0, 9);
        store.sync().unwrap();
    }

    fn check_shared_decode<R, S>(source: &R::CType, verify: impl Fn(&R))
    where
        R: ReprC<CType: Sized> + 'static,
        S: Store + Default,
        for<'d> &'d R: DecodeMove<'d, Store = S> + ReprC<CType = CSoftRef<R>>,
    {
        for unchecked in [false, true] {
            let mut store = Default::default();
            let value = if unchecked {
                unsafe { co3::soft_decode_unchecked::<&R>(CSoftRef::from_ref(source), &mut store) }
            } else {
                unsafe { co3::soft_decode::<&R>(CSoftRef::from_ref(source), &mut store) }.unwrap()
            };
            verify(value);
            store.sync().unwrap();
        }
    }

    #[test]
    fn shared_containers_extract_non_clone_elements() {
        use co3::{boxed::CBox, option::COption, result::CResult, vec::CVec, wide::CWideBox};

        assert_not_impl_any!(ReferenceOnly: Clone, DecodeMove<'static>);

        let source = CBox::<ReferenceOnly>::from_box(Box::new(7));
        check_shared_decode::<Box<ReferenceOnly>, _>(&source, |value| assert_eq!(value.0, 7));
        assert_eq!(*unsafe { source.into_box() }.unwrap(), 7);

        check_shared_decode::<[ReferenceOnly; 2], _>(&[7, 11], |value| {
            assert_eq!([value[0].0, value[1].0], [7, 11]);
        });

        let source = CVec::<ReferenceOnly>::from_vec(vec![7, 11]);
        check_shared_decode::<alloc::vec::Vec<ReferenceOnly>, _>(&source, |value| {
            assert_eq!([value[0].0, value[1].0], [7, 11]);
        });
        assert_eq!(unsafe { source.into_vec() }.unwrap(), [7, 11]);

        let source = CVec::<ReferenceOnly, true>::from_vec(vec![7, 11]);
        check_shared_decode::<co3::vec::Vec<ReferenceOnly>, _>(&source, |value| {
            assert_eq!([value[0].0, value[1].0], [7, 11]);
        });
        assert_eq!(unsafe { source.into_vec() }.unwrap(), [7, 11]);

        let source = CWideBox::<[ReferenceOnly]>::from_box(vec![7, 11].into_boxed_slice());
        check_shared_decode::<Box<[ReferenceOnly]>, _>(&source, |value| {
            assert_eq!([value[0].0, value[1].0], [7, 11]);
        });
        assert_eq!(&*unsafe { source.into_box() }.unwrap(), &[7, 11]);

        check_shared_decode::<Option<ReferenceOnly>, _>(&COption::new(Some(7)), |value| {
            assert_eq!(value.as_ref().unwrap().0, 7);
        });
        check_shared_decode::<Option<ReferenceOnly>, _>(&COption::new(None), |value| {
            assert!(value.is_none());
        });
        check_shared_decode::<co3::option::Option<ReferenceOnly>, _>(
            &COption::new(Some(7)),
            |value| assert!(matches!(value, co3::option::Option::Some(ReferenceOnly(7)))),
        );
        check_shared_decode::<Result<ReferenceOnly, ReferenceOnly>, _>(
            &CResult::new(Ok(7)),
            |value| {
                assert_eq!(value.as_ref().ok().unwrap().0, 7);
            },
        );
        check_shared_decode::<Result<ReferenceOnly, ReferenceOnly>, _>(
            &CResult::new(Err(11)),
            |value| {
                assert_eq!(value.as_ref().err().unwrap().0, 11);
            },
        );
        check_shared_decode::<co3::result::Result<ReferenceOnly, ReferenceOnly>, _>(
            &CResult::new(Ok(7)),
            |value| assert!(matches!(value, co3::result::Result::Ok(ReferenceOnly(7)))),
        );
    }

    #[test]
    fn mixed_aggregates_extract_non_clone_fields() {
        #[derive(RustSpec, co3::ReprC)]
        struct Record(ReferenceOnly, u32);

        check_shared_decode::<(u32, ReferenceOnly), _>(&co3::tuple::CTuple2(5, 7), |value| {
            assert_eq!((value.0, value.1.0), (5, 7))
        });
        check_shared_decode::<co3::tuple::Tuple2<u32, ReferenceOnly>, _>(
            &co3::tuple::Tuple2(5, 7),
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

    unsafe impl co3::sync::TakeDecoded for SharedStore {
        type Value = ReferenceOnly;

        unsafe fn take_decoded(store: *mut Self) -> Option<Self::Value> {
            unsafe { (*store).0.take() }
        }
    }

    unsafe impl<'d> SoftDecodeRef<'d> for ReferenceOnly {
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

        unsafe fn soft_decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut SharedStore,
        ) -> &'d Self {
            let value = *unsafe { source.into_ref().unwrap_unchecked() };
            store.0.insert(Self(value))
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

    unsafe impl<'d> SoftDecodeMut<'d> for ReferenceOnly {
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

        unsafe fn soft_decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut Self::Store,
        ) -> &'d mut Self {
            let source = unsafe { source.into_mut().unwrap_unchecked() };
            let value = ReferenceOnly(*source);
            store.source = Some(core::ptr::NonNull::from(source));
            store.value.insert(value)
        }
    }

    #[test]
    fn downstream_shared_decoder_is_used_without_owned_decoding() {
        assert_not_impl_any!(ReferenceOnly: crate::borrow::BorrowedDecode<'static>, DecodeMove<'static>);
        let source = 7;
        let mut store = Default::default();
        let value =
            unsafe { co3::soft_decode::<&ReferenceOnly>(CSoftRef::from_ref(&source), &mut store) }
                .unwrap();
        assert_eq!(value.0, 7);
        store.sync().unwrap();

        let mut store = Default::default();
        assert!(
            unsafe { co3::soft_decode::<&ReferenceOnly>(CSoftRef::null(), &mut store) }.is_none()
        );
        let mut store = Default::default();
        let zero = 0;
        assert!(
            unsafe { co3::soft_decode::<&ReferenceOnly>(CSoftRef::from_ref(&zero), &mut store) }
                .is_none()
        );
        // Exercise unchecked dispatch with a valid source as well.
        let mut store = Default::default();
        let value = unsafe {
            co3::soft_decode_unchecked::<&ReferenceOnly>(CSoftRef::from_ref(&source), &mut store)
        };
        assert_eq!(value.0, 7);
    }

    #[test]
    fn downstream_mutable_decoder_synchronizes_the_source() {
        for unchecked in [false, true] {
            let mut source = 7;
            let mut store = Default::default();
            let carrier = CSoftRefMut::from_mut(&mut source);
            let value = if unchecked {
                unsafe { co3::soft_decode_unchecked::<&mut ReferenceOnly>(carrier, &mut store) }
            } else {
                unsafe { co3::soft_decode::<&mut ReferenceOnly>(carrier, &mut store) }.unwrap()
            };
            value.0 = 11;
            store.sync().unwrap();
            assert_eq!(source, 11);
        }
        let mut store = Default::default();
        assert!(
            unsafe { co3::soft_decode::<&mut ReferenceOnly>(CSoftRefMut::null(), &mut store) }
                .is_none()
        );
    }

    #[test]
    fn mutable_slice_decode_writes_changes_back() {
        use co3::{tuple::CTuple2, wide::CSoftWideRefMut};

        let mut values = [CTuple2(1_u8, 2_u8)];
        let source = CSoftWideRefMut::from_mut(&mut values[..]);
        let mut store = Default::default();
        let decoded = unsafe { co3::soft_decode::<&mut [(u8, u8)]>(source, &mut store) }.unwrap();
        decoded[0] = (3, 4);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(values[0], CTuple2(3, 4));
    }
}
