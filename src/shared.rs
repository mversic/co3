//! Referent-specific encoding for the unstable shared-reference path.

#[cfg(all(feature = "alloc", feature = "repr-rust"))]
use alloc::{boxed::Box, vec::Vec};
#[cfg(feature = "repr-rust")]
use core::{marker::PhantomData, mem::ManuallyDrop};

use disjoint_impls::disjoint_impls;
#[cfg(feature = "repr-rust")]
use rust_spec::{
    RustSpec,
    mutability::{Exclusive, ExclusiveKind},
    size::{MetaSized, NulTerminated, Sized as RustSpecSized, SliceLike, Zero},
};
#[cfg(all(feature = "alloc", feature = "repr-rust"))]
use rust_spec::{Specified, Unspecified, layout::Robust, mutability::WithInterior};

#[cfg(feature = "repr-rust")]
use crate::{
    CType,
    convert::{EncodeMove, EncodeMoveUnchecked, encode_ref_field, encode_ref_field_unchecked},
    marker::Native,
    niche::Niche,
    option::abi::{Option as AbiOption, OptionSome as AbiOptionSome},
    reference::abi::{RefCarrier, SoftRef},
    result::abi::{Result as AbiResult, ResultErr as AbiResultErr, ResultOk as AbiResultOk},
    sync::{ArrayStore, RefSizedEncodeStore},
    wide::{
        Wide, WideHeader,
        abi::{SoftWideRef as AbiSoftWideRef, WideRefCarrier},
    },
};
use crate::{
    ReprC,
    convert::Store,
    lang::{self, Language},
};
#[cfg(all(feature = "alloc", feature = "repr-rust"))]
use crate::{
    boxed::abi::Box as AbiBox,
    marker::AbiCompatible,
    sync::{RefVecEncodeStore, RefWideEncodeStore},
    vec::abi::Vec as AbiVec,
    wide::abi::WideBox as AbiWideBox,
};

#[cfg(all(feature = "alloc", feature = "repr-rust"))]
trait SharedVecMutability {}

#[cfg(all(feature = "alloc", feature = "repr-rust"))]
impl<I: ExclusiveKind> SharedVecMutability for Exclusive<I> {}
#[cfg(all(feature = "alloc", feature = "repr-rust"))]
impl SharedVecMutability for WithInterior<Robust> {}

disjoint_impls! {
    /// Encodes a shared reference using conversion storage without transferring ownership.
    ///
    /// # Safety
    ///
    /// See [`crate::soft_encode_unchecked`].
    pub unsafe trait SoftEncodeRefUnchecked<'a>: ReprC + 'a
    where
        &'a Self: ReprC<CType: Sized>,
    {
        type SoftStore: Store + Default;
        /// Returns a borrowed reference carrier. Neither the referent nor any
        /// backing allocation is transferred to the caller.
        /// # Safety
        /// See [`crate::soft_encode_unchecked`].
        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm;
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: ReprC + ?Sized + 'a> SoftEncodeRefUnchecked<'a> for Box<R>
    where
        R: RustSpec,
        &'a R: EncodeMoveUnchecked + ReprC<CType = SoftRef<<R as ReprC>::CType>>,
        Self: ReprC<CType = AbiBox<<R as ReprC>::CType>>,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        <R as ReprC>::CType: Sized,
    {
        type SoftStore = RefSizedEncodeStore<Self, <&'a R as EncodeMoveUnchecked>::SoftStore>;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = unsafe { (&**self).soft_encode_unchecked(fields) };
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { AbiBox::from_raw(view.as_ptr().cast_mut()) }
            })
        }
    }
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: 'a + RustSpec + ?Sized> SoftEncodeRefUnchecked<'a> for Box<R>
    where
        R: RustSpec<Size = MetaSized<SliceLike>> + ReprC,
        &'a R: EncodeMoveUnchecked,
        Self: ReprC<CType = AbiWideBox<<R as ReprC>::CType>>,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>>,
        &'a R: ReprC<CType: WideRefCarrier<Pointee = <R as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, <&'a R as EncodeMoveUnchecked>::SoftStore>;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = unsafe { (&**self).soft_encode_unchecked(fields) };
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe {
                    AbiWideBox::from_raw_nullable(
                        WideRefCarrier::data(&view).cast_mut(),
                        WideRefCarrier::metadata(&view),
                    )
                }
            })
        }
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: AbiCompatible, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a> for Vec<R>
    where
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        <R as ReprC>::CType: Sized,
        R: RustSpec<Layout = Specified>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
        Self: ReprC<CType = Native<AbiVec<<R as ReprC>::CType>>> + 'a,
    {
        type SoftStore = RefSizedEncodeStore<Self, ()>;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|_fields| {
                Native(unsafe {
                    AbiVec::from_raw_parts(self.as_ptr().cast_mut().cast(), self.len(), self.capacity())
                })
            })
        }
    }
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: ReprC, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a> for Vec<R>
    where
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        <R as ReprC>::CType: Copy,
        R: RustSpec<Layout = Unspecified>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
        &'a R: EncodeMoveUnchecked + ReprC<CType = SoftRef<<R as ReprC>::CType>>,
        Self: ReprC<CType = Native<AbiVec<<R as ReprC>::CType>>> + 'a,
    {
        type SoftStore = RefSizedEncodeStore<
            Self,
            RefVecEncodeStore<R, Box<[<&'a R as EncodeMoveUnchecked>::SoftStore]>>,
        >;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let store = fields;
                Native(store.encode(|stores| unsafe {
                    encode_vec_unchecked::<_>(self, self.capacity(), stores)
                }))
            })
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: ReprC, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a> for Option<R>
    where
        Self: ReprC<CType = Native<crate::option::abi::Option<<R as ReprC>::CType>>>,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMoveUnchecked,
        <R as ReprC>::CType: Copy,
        &'a R: ReprC<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMoveUnchecked>::SoftStore>>;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                Native({
                    let store = fields;

                    match self {
                        Some(value) => AbiOption::Some(unsafe {
                            encode_ref_field_unchecked::<_>(value, store.insert(Default::default()))
                        }),
                        None => {
                            *store = None;
                            AbiOption::None()
                        }
                    }
                })
            })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: Niche, __Interior: ExclusiveKind, N: rust_spec::layout::LayoutKind>
        SoftEncodeRefUnchecked<'a> for Option<R>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<CType = Native<crate::option::abi::OptionSome<<R as ReprC>::CType>>>,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMoveUnchecked,
        &'a R: ReprC<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMoveUnchecked>::SoftStore>>;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                Native({
                    let store = fields;

                    match self {
                        Some(value) => AbiOptionSome(unsafe {
                            encode_ref_field_unchecked::<_>(value, store.insert(Default::default()))
                        }),
                        None => {
                            *store = None;
                            AbiOptionSome(R::NICHE)
                        }
                    }
                })
            })
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: ReprC, E: ReprC, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a>
        for Result<R, E>
    where
        Self:
            ReprC<CType = Native<crate::result::abi::Result<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        <R as ReprC>::CType: Copy,
        <E as ReprC>::CType: Copy,
        &'a R: EncodeMoveUnchecked,
        &'a E: EncodeMoveUnchecked,
        &'a R: ReprC<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
        &'a E: ReprC<CType: RefCarrier<Pointee = <E as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<
            Self,
            Option<
                Result<
                    <&'a R as EncodeMoveUnchecked>::SoftStore,
                    <&'a E as EncodeMoveUnchecked>::SoftStore,
                >,
            >,
        >;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                Native({
                    let store = fields;

                    match self {
                        Ok(ok) => {
                            let Result::Ok(inner) = store.insert(Ok(Default::default())) else {
                                unreachable!()
                            };
                            AbiResult::Ok(unsafe { encode_ref_field_unchecked::<_>(ok, inner) })
                        }
                        Err(err) => {
                            let Result::Err(inner) = store.insert(Err(Default::default())) else {
                                unreachable!()
                            };
                            AbiResult::Err(unsafe { encode_ref_field_unchecked::<_>(err, inner) })
                        }
                    }
                })
            })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: Niche, E: ReprC, __Interior: ExclusiveKind, N: rust_spec::layout::LayoutKind>
        SoftEncodeRefUnchecked<'a> for Result<R, E>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultOk<<R as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMoveUnchecked,
        <E as ReprC>::CType: Sized,
        &'a R: ReprC<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMoveUnchecked>::SoftStore>>;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                Native({
                    let store = fields;

                    match self {
                        Ok(ok) => AbiResultOk(
                            unsafe {
                                encode_ref_field_unchecked::<_>(ok, store.insert(Default::default()))
                            },
                            PhantomData,
                        ),
                        Err(_) => {
                            *store = None;
                            AbiResultOk(R::NICHE, PhantomData)
                        }
                    }
                })
            })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: ReprC, E: Niche, __Interior: ExclusiveKind, N: rust_spec::layout::LayoutKind>
        SoftEncodeRefUnchecked<'a> for Result<R, E>
    where
        E: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultErr<<R as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a E: EncodeMoveUnchecked,
        <R as ReprC>::CType: Sized,
        &'a E: ReprC<CType: RefCarrier<Pointee = <E as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, Option<<&'a E as EncodeMoveUnchecked>::SoftStore>>;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                Native({
                    let store = fields;

                    match self {
                        Ok(_) => {
                            *store = None;
                            AbiResultErr(E::NICHE, PhantomData)
                        }
                        Err(err) => AbiResultErr(
                            unsafe {
                                encode_ref_field_unchecked::<_>(err, store.insert(Default::default()))
                            },
                            PhantomData,
                        ),
                    }
                })
            })
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: ReprC + ?Sized, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a>
        for ManuallyDrop<R>
    where
        Self: ReprC + RustSpec<Size: crate::Thin, Mutability = Exclusive<__Interior>> + 'a,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'a R: ReprC<CType = SoftRef<<R as ReprC>::CType>>,
        &'a R: EncodeMoveUnchecked,
        <Self as ReprC>::CType: Sized,
    {
        type SoftStore = <&'a R as EncodeMoveUnchecked>::SoftStore;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = unsafe { (&**self).soft_encode_unchecked(store) };
            // ManuallyDrop's C representation transparently wraps the inner CType.
            // SAFETY: ManuallyDrop preserves the inner layout and metadata.
            unsafe { inner.cast() }
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: ReprC + ?Sized, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a>
        for ManuallyDrop<R>
    where
        Self: ReprC + RustSpec<Size = NulTerminated, Mutability = Exclusive<__Interior>> + 'a,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'a R: ReprC<CType = SoftRef<<R as ReprC>::CType>>,
        &'a R: EncodeMoveUnchecked,
        <Self as ReprC>::CType: Sized,
    {
        type SoftStore = <&'a R as EncodeMoveUnchecked>::SoftStore;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = unsafe { (&**self).soft_encode_unchecked(store) };
            // SAFETY: ManuallyDrop preserves the inner layout and metadata.
            unsafe { inner.cast() }
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<
        'a,
        R: ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>
            + ?Sized,
        __Interior: ExclusiveKind,
    > SoftEncodeRefUnchecked<'a> for ManuallyDrop<R>
    where
        Self: ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>
            + RustSpec<Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>
            + 'a,
        &'a Self: ReprC<CType = AbiSoftWideRef<<Self as ReprC>::CType>>,
        &'a R: ReprC,
        &'a R: EncodeMoveUnchecked,
        &'a R: ReprC<CType: WideRefCarrier<Pointee = <R as ReprC>::CType>>,
    {
        type SoftStore = <&'a R as EncodeMoveUnchecked>::SoftStore;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = unsafe { (&**self).soft_encode_unchecked(store) };
            // SAFETY: ManuallyDrop preserves the inner layout and metadata.
            unsafe { AbiSoftWideRef::from_raw_parts(inner.data().cast(), inner.metadata()) }
        }
    }
}

disjoint_impls! {
    /// Checked encoding of the contents of a shared reference.
    ///
    /// # Safety
    ///
    /// The returned representation must satisfy the checked shared-reference encoding
    /// contract, remain valid while `self` and the store are alive, and preserve Rust
    /// aliasing and interior-mutability invariants. It is not an ownership transfer.
    /// If `Store` implements `EmptyStore`, the carrier must not borrow from it.
    /// Callers must ensure the foreign boundary upholds the validity and aliasing guarantees of `L`.
    pub unsafe trait SoftEncodeRef<'a, L: Language = lang::C>:
        SoftEncodeRefUnchecked<'a>
    where
        &'a Self: ReprC<CType: Sized>,
    {
        type Store: Store + Default;

        /// Returns access to the encoded referent without transferring ownership.
        /// The source and store must remain alive for any later access. Neither
        /// the referent nor its backing allocations may be freed or owned-decoded.
        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            &'a Self: ReprC<CType: Sized>,
            'a: 'itm;
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, L: Language, R: ReprC + ?Sized + 'a> SoftEncodeRef<'a, L> for Box<R>
    where
        R: RustSpec,
        &'a R: EncodeMove<L> + ReprC<CType = SoftRef<<R as ReprC>::CType>>,
        Self: ReprC<CType = AbiBox<<R as ReprC>::CType>>,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        <R as ReprC>::CType: Sized,
    {
        type Store = RefSizedEncodeStore<Self, <&'a R as EncodeMove<L>>::Store>;

        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = crate::convert::EncodeMove::<L>::soft_encode(&**self, fields);
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { AbiBox::from_raw(view.as_ptr().cast_mut()) }
            })
        }
    }
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, L: Language, R: RustSpec + 'a + ?Sized> SoftEncodeRef<'a, L> for Box<R>
    where
        R: RustSpec<Size = MetaSized<SliceLike>> + ReprC,
        &'a R: EncodeMove<L>,
        Self: ReprC<CType = AbiWideBox<<R as ReprC>::CType>>,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>>,
        &'a R: ReprC<CType: WideRefCarrier<Pointee = <R as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, <&'a R as EncodeMove<L>>::Store>;

        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = crate::convert::EncodeMove::<L>::soft_encode(&**self, fields);
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe {
                    AbiWideBox::from_raw_nullable(
                        WideRefCarrier::data(&view).cast_mut(),
                        WideRefCarrier::metadata(&view),
                    )
                }
            })
        }
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, L: Language, T: AbiCompatible, __Interior: ExclusiveKind> SoftEncodeRef<'a, L>
        for Vec<T>
    where
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        T: RustSpec<Layout = Specified, Mutability: SharedVecMutability>,
        Self: ReprC<CType = Native<AbiVec<<T as ReprC>::CType>>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        <T as ReprC>::CType: Sized,
    {
        type Store = RefSizedEncodeStore<Self, ()>;

        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|_fields| {
                Native(unsafe {
                    AbiVec::from_raw_parts(self.as_ptr().cast_mut().cast(), self.len(), self.capacity())
                })
            })
        }
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, L: Language, T: ReprC, __Interior: ExclusiveKind> SoftEncodeRef<'a, L> for Vec<T>
    where
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        T: RustSpec<Layout = Unspecified>,
        Self: crate::convert::SoftEncodeRefUnchecked<'a>
            + ReprC<CType = Native<AbiVec<<T as ReprC>::CType>>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a T: EncodeMove<L> + ReprC<CType = SoftRef<<T as ReprC>::CType>>,
        <T as ReprC>::CType: Copy,
    {
        type Store =
            RefSizedEncodeStore<Self, RefVecEncodeStore<T, Box<[<&'a T as EncodeMove<L>>::Store]>>>;

        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                Native({
                    let store = fields;

                    store.encode(|stores| encode_vec::<_, L>(self, self.capacity(), stores))
                })
            })
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, L: Language, R: ReprC, __Interior: ExclusiveKind> SoftEncodeRef<'a, L> for Option<R>
    where
        Self: ReprC<CType = Native<crate::option::abi::Option<<R as ReprC>::CType>>>,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMove<L>,
        <R as ReprC>::CType: Copy,
        &'a R: ReprC<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMove<L>>::Store>>;

        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                Native({
                    let store = fields;

                    match self {
                        Some(value) => AbiOption::Some(encode_ref_field::<_, L>(
                            value,
                            store.insert(Default::default()),
                        )),
                        None => {
                            *store = None;
                            AbiOption::None()
                        }
                    }
                })
            })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, L: Language, R: Niche, __Interior: ExclusiveKind, N: rust_spec::layout::LayoutKind>
        SoftEncodeRef<'a, L> for Option<R>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<CType = Native<crate::option::abi::OptionSome<<R as ReprC>::CType>>>,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMove<L>,
        <R as ReprC>::CType: Copy,
        &'a R: ReprC<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMove<L>>::Store>>;

        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                Native({
                    let store = fields;

                    if let Some(value) = self {
                        let encoded = encode_ref_field::<_, L>(value, store.insert(Default::default()));
                        debug_assert!(!R::is_niche(&encoded), "encoding produced NICHE value");
                        AbiOptionSome(encoded)
                    } else {
                        *store = None;
                        AbiOptionSome(R::NICHE)
                    }
                })
            })
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, L: Language, R: ReprC, E: ReprC, __Interior: ExclusiveKind> SoftEncodeRef<'a, L>
        for Result<R, E>
    where
        Self:
            ReprC<CType = Native<crate::result::abi::Result<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMove<L>,
        &'a E: EncodeMove<L>,
        <R as ReprC>::CType: Copy,
        <E as ReprC>::CType: Copy,
        &'a R: ReprC<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
        &'a E: ReprC<CType: RefCarrier<Pointee = <E as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<
            Self,
            Option<Result<<&'a R as EncodeMove<L>>::Store, <&'a E as EncodeMove<L>>::Store>>,
        >;

        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                Native({
                    let store = fields;

                    match self {
                        Ok(ok) => {
                            let Ok(store) = store.insert(Ok(Default::default())) else {
                                unreachable!()
                            };
                            AbiResult::Ok(encode_ref_field::<_, L>(ok, store))
                        }
                        Err(err) => {
                            let Err(store) = store.insert(Err(Default::default())) else {
                                unreachable!()
                            };
                            AbiResult::Err(encode_ref_field::<_, L>(err, store))
                        }
                    }
                })
            })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<
        'a,
        L: Language,
        R: Niche,
        E: ReprC,
        __Interior: ExclusiveKind,
        N: rust_spec::layout::LayoutKind,
    > SoftEncodeRef<'a, L> for Result<R, E>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultOk<<R as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMove<L>,
        <E as ReprC>::CType: Sized,
        &'a R: ReprC<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMove<L>>::Store>>;

        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                Native({
                    let store = fields;

                    match self {
                        Ok(ok) => AbiResultOk(
                            encode_ref_field::<_, L>(ok, store.insert(Default::default())),
                            PhantomData,
                        ),
                        Err(_) => {
                            *store = None;
                            AbiResultOk(R::NICHE, PhantomData)
                        }
                    }
                })
            })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<
        'a,
        L: Language,
        R: ReprC,
        E: Niche,
        __Interior: ExclusiveKind,
        N: rust_spec::layout::LayoutKind,
    > SoftEncodeRef<'a, L> for Result<R, E>
    where
        E: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultErr<<R as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a E: EncodeMove<L>,
        <R as ReprC>::CType: Sized,
        &'a E: ReprC<CType: RefCarrier<Pointee = <E as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, Option<<&'a E as EncodeMove<L>>::Store>>;

        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                Native({
                    let store = fields;

                    match self {
                        Ok(_) => {
                            *store = None;
                            AbiResultErr(E::NICHE, PhantomData)
                        }
                        Err(err) => AbiResultErr(
                            encode_ref_field::<_, L>(err, store.insert(Default::default())),
                            PhantomData,
                        ),
                    }
                })
            })
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, L: Language, R: RustSpec + 'a + ReprC + ?Sized, __Interior: ExclusiveKind>
        SoftEncodeRef<'a, L> for ManuallyDrop<R>
    where
        Self: ReprC + RustSpec<Size: crate::Thin, Mutability = Exclusive<__Interior>> + 'a,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'a R: EncodeMove<L> + ReprC<CType = SoftRef<<R as ReprC>::CType>>,
        <Self as ReprC>::CType: Sized,
    {
        type Store = <&'a R as EncodeMove<L>>::Store;

        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = EncodeMove::<L>::soft_encode(&**self, store);
            // SAFETY: ManuallyDrop preserves the inner layout and metadata.
            unsafe { inner.cast() }
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, L: Language, R: RustSpec + 'a + ReprC + ?Sized, __Interior: ExclusiveKind>
        SoftEncodeRef<'a, L> for ManuallyDrop<R>
    where
        Self: ReprC + RustSpec<Size = NulTerminated, Mutability = Exclusive<__Interior>> + 'a,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'a R: EncodeMove<L> + ReprC<CType = SoftRef<<R as ReprC>::CType>>,
        <Self as ReprC>::CType: Sized,
    {
        type Store = <&'a R as EncodeMove<L>>::Store;

        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = EncodeMove::<L>::soft_encode(&**self, store);
            // SAFETY: ManuallyDrop preserves the inner layout and metadata.
            unsafe { inner.cast() }
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<
        'a,
        L: Language,
        R: RustSpec
            + 'a
            + ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>
            + ?Sized,
        __Interior: ExclusiveKind,
    > SoftEncodeRef<'a, L> for ManuallyDrop<R>
    where
        Self: ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>
            + RustSpec<Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>
            + 'a,
        &'a Self: ReprC<CType = AbiSoftWideRef<<Self as ReprC>::CType>>,
        &'a R: EncodeMove<L>,
        &'a R: ReprC<CType: WideRefCarrier<Pointee = <R as ReprC>::CType>>,
    {
        type Store = <&'a R as EncodeMove<L>>::Store;

        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = EncodeMove::<L>::soft_encode(&**self, store);
            // SAFETY: ManuallyDrop preserves the inner layout and metadata.
            unsafe { AbiSoftWideRef::from_raw_parts(inner.data().cast(), inner.metadata()) }
        }
    }
}

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<'a, R: ReprC, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a>
    for crate::vec::Vec<R>
where
    &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    Self: ReprC<CType = AbiVec<<R as ReprC>::CType>>,
    <R as ReprC>::CType: Copy,
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
    &'a R: EncodeMoveUnchecked + ReprC<CType = SoftRef<<R as ReprC>::CType>>,
{
    type SoftStore = RefSizedEncodeStore<
        Self,
        RefVecEncodeStore<R, Box<[<&'a R as EncodeMoveUnchecked>::SoftStore]>>,
    >;

    unsafe fn soft_encode_ref_unchecked<'itm>(
        &'a self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| {
            let store = fields;

            store.encode(|stores| unsafe {
                encode_vec_unchecked::<_>(self, self.capacity(), stores)
            })
        })
    }
}

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<'a, L: Language, T: ReprC, __Interior: ExclusiveKind> SoftEncodeRef<'a, L>
    for crate::vec::Vec<T>
where
    &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    Self: crate::convert::SoftEncodeRefUnchecked<'a> + ReprC<CType = AbiVec<<T as ReprC>::CType>>,
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
    &'a T: EncodeMove<L> + ReprC<CType = SoftRef<<T as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
{
    type Store =
        RefSizedEncodeStore<Self, RefVecEncodeStore<T, Box<[<&'a T as EncodeMove<L>>::Store]>>>;

    fn soft_encode_ref<'itm>(
        &'a self,
        store: &'itm mut Self::Store,
    ) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| {
            let store = fields;

            store.encode(|stores| encode_vec::<_, L>(self, self.capacity(), stores))
        })
    }
}

#[cfg(feature = "repr-rust")]
unsafe impl<'a, T: ReprC + 'a, const N: usize> SoftEncodeRefUnchecked<'a> for [T; N]
where
    &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'a T: EncodeMoveUnchecked + ReprC<CType = SoftRef<<T as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
{
    type SoftStore =
        RefSizedEncodeStore<Self, ArrayStore<<&'a T as EncodeMoveUnchecked>::SoftStore, N>>;
    unsafe fn soft_encode_ref_unchecked<'itm>(
        &'a self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| {
            core::array::from_fn(|i| unsafe {
                encode_ref_field_unchecked::<_>(&self[i], &mut fields.0[i])
            })
        })
    }
}

#[cfg(feature = "repr-rust")]
unsafe impl<'a, L: Language, T: ReprC + 'a, const N: usize> SoftEncodeRef<'a, L> for [T; N]
where
    &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'a T: EncodeMove<L> + ReprC<CType = SoftRef<<T as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
{
    type Store = RefSizedEncodeStore<Self, ArrayStore<<&'a T as EncodeMove<L>>::Store, N>>;
    fn soft_encode_ref<'itm>(
        &'a self,
        store: &'itm mut Self::Store,
    ) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| {
            core::array::from_fn(|i| encode_ref_field::<_, L>(&self[i], &mut fields.0[i]))
        })
    }
}

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<'a, T: ReprC + 'a> SoftEncodeRefUnchecked<'a> for [T]
where
    &'a T: EncodeMoveUnchecked + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    &'a Self: ReprC<CType = AbiSoftWideRef<<Self as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
{
    type SoftStore = RefWideEncodeStore<Self, Box<[<&'a T as EncodeMoveUnchecked>::SoftStore]>>;

    unsafe fn soft_encode_ref_unchecked<'itm>(
        &'a self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|stores| {
            map_boxed_slice(self.iter(), stores, |item, store| unsafe {
                encode_ref_field_unchecked::<_>(item, store)
            })
        })
    }
}

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<'a, L: Language, T: ReprC + 'a> SoftEncodeRef<'a, L> for [T]
where
    &'a T: EncodeMove<L> + ReprC<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    &'a Self: ReprC<CType = AbiSoftWideRef<<Self as ReprC>::CType>>,
    <T as ReprC>::CType: Copy,
{
    type Store = RefWideEncodeStore<Self, Box<[<&'a T as EncodeMove<L>>::Store]>>;

    fn soft_encode_ref<'itm>(
        &'a self,
        store: &'itm mut Self::Store,
    ) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|stores| map_boxed_slice(self.iter(), stores, encode_ref_field))
    }
}

#[cfg(feature = "repr-rust")]
unsafe impl<'a> SoftEncodeRefUnchecked<'a> for core::ffi::CStr {
    type SoftStore = ();

    unsafe fn soft_encode_ref_unchecked<'itm>(
        &'a self,
        _: &'itm mut (),
    ) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        unsafe { SoftRef::from_raw(self.as_ptr().cast()) }
    }
}

#[cfg(feature = "repr-rust")]
unsafe impl<'a, L: Language> SoftEncodeRef<'a, L> for core::ffi::CStr {
    type Store = ();

    fn soft_encode_ref<'itm>(&'a self, _: &'itm mut ()) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        unsafe { SoftRef::from_raw(self.as_ptr().cast()) }
    }
}

// Copy only the shared reference, then retain its converted representation and
// backing store for the outer reference. The pointee is never cloned.
#[cfg(feature = "repr-rust")]
unsafe impl<'a, 'b: 'a, R: ?Sized, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a> for &'b R
where
    &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    Self: EncodeMoveUnchecked
        + ReprC
        + RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
    <Self as ReprC>::CType: Sized,
{
    type SoftStore = RefSizedEncodeStore<Self, <Self as EncodeMoveUnchecked>::SoftStore>;

    unsafe fn soft_encode_ref_unchecked<'itm>(
        &'a self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| unsafe { (*self).soft_encode_unchecked(fields) })
    }
}

#[cfg(feature = "repr-rust")]
unsafe impl<'a, 'b: 'a, L: Language, R: ?Sized, __Interior: ExclusiveKind> SoftEncodeRef<'a, L>
    for &'b R
where
    &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    Self: EncodeMove<L> + ReprC + RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
    <Self as ReprC>::CType: Sized,
{
    type Store = RefSizedEncodeStore<Self, <Self as EncodeMove<L>>::Store>;

    fn soft_encode_ref<'itm>(
        &'a self,
        store: &'itm mut Self::Store,
    ) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| crate::convert::EncodeMove::<L>::soft_encode(*self, fields))
    }
}

// A shared borrow of an exclusive reference only permits shared pointee access.
// The mutable carrier is retained solely to match the nested reference ABI.
#[cfg(feature = "repr-rust")]
unsafe impl<'a, 'b: 'a, R: RustSpec + ?Sized, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a>
    for &'b mut R
where
    &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    Self: ReprC + RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
    &'a R: EncodeMoveUnchecked + ReprC,
    <Self as ReprC>::CType: Copy,
    <&'a R as ReprC>::CType: Copy,
{
    type SoftStore = RefSizedEncodeStore<Self, <&'a R as EncodeMoveUnchecked>::SoftStore>;

    unsafe fn soft_encode_ref_unchecked<'itm>(
        &'a self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| {
            let value = unsafe { (&**self).soft_encode_unchecked(fields) };
            const {
                assert!(
                    core::mem::size_of::<<Self as ReprC>::CType>()
                        == core::mem::size_of::<<&R as ReprC>::CType>()
                );
            }
            // Shared and mutable reference carriers have identical pointer/metadata
            // layouts. This changes only the ABI carrier, not access permissions.
            unsafe { core::mem::transmute_copy(&value) }
        })
    }
}

#[cfg(feature = "repr-rust")]
unsafe impl<'a, 'b: 'a, L: Language, R: RustSpec + ?Sized, __Interior: ExclusiveKind>
    SoftEncodeRef<'a, L> for &'b mut R
where
    &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    Self: ReprC + RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
    &'a R: EncodeMove<L> + ReprC,
    <Self as ReprC>::CType: Copy,
    <&'a R as ReprC>::CType: Copy,
{
    type Store = RefSizedEncodeStore<Self, <&'a R as EncodeMove<L>>::Store>;

    fn soft_encode_ref<'itm>(
        &'a self,
        store: &'itm mut Self::Store,
    ) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| {
            let value = crate::convert::EncodeMove::<L>::soft_encode(&**self, fields);
            const {
                assert!(
                    core::mem::size_of::<<Self as ReprC>::CType>()
                        == core::mem::size_of::<<&R as ReprC>::CType>()
                );
            }
            // Shared and mutable reference carriers have identical pointer/metadata
            // layouts. This changes only the ABI carrier, not access permissions.
            unsafe { core::mem::transmute_copy(&value) }
        })
    }
}

#[cfg(all(feature = "alloc", feature = "repr-rust"))]
macro_rules! impl_buffer_shared {
    ($ty:ty, $carrier:expr) => {
        unsafe impl<'a> SoftEncodeRefUnchecked<'a> for $ty {
            type SoftStore = RefSizedEncodeStore<Self, ()>;
            unsafe fn soft_encode_ref_unchecked<'itm>(
                &'a self,
                store: &'itm mut Self::SoftStore,
            ) -> <&'a Self as ReprC>::CType
            where
                'a: 'itm,
            {
                store.encode(|_fields| ($carrier)(self))
            }
        }
        unsafe impl<'a, L: Language> SoftEncodeRef<'a, L> for $ty {
            type Store = RefSizedEncodeStore<Self, ()>;
            fn soft_encode_ref<'itm>(
                &'a self,
                store: &'itm mut Self::Store,
            ) -> <&'a Self as crate::ReprC>::CType
            where
                'a: 'itm,
            {
                store.encode(|_fields| ($carrier)(self))
            }
        }
    };
}
#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
impl_buffer_shared!(
    alloc::string::String,
    |value: &alloc::string::String| unsafe {
        Native(crate::string::abi::String(AbiVec::from_raw_parts(
            value.as_ptr().cast_mut(),
            value.len(),
            value.capacity(),
        )))
    }
);
#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
impl_buffer_shared!(alloc::ffi::CString, |value: &alloc::ffi::CString| unsafe {
    AbiBox::from_raw(value.as_ptr().cast_mut().cast())
});

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
fn map_boxed_slice<'a, I: ExactSizeIterator, U, S: Default>(
    values: I,
    stores: &'a mut Box<[S]>,
    mut convert: impl FnMut(I::Item, &'a mut S) -> U,
) -> Box<[U]> {
    *stores = core::iter::repeat_with(S::default)
        .take(values.len())
        .collect();
    values
        .zip(stores.iter_mut())
        .map(|(value, store)| convert(value, store))
        .collect()
}

#[cfg(all(feature = "alloc", feature = "repr-rust"))]
fn encode_vec<'a, T: ReprC<CType: Copy> + 'a, L: Language>(
    source: &'a [T],
    capacity: usize,
    stores: &mut Box<[<&'a T as EncodeMove<L>>::Store]>,
) -> Vec<T::CType>
where
    &'a T: EncodeMove<L, CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
{
    crate::stored::map_vec(source.iter(), capacity, stores, encode_ref_field)
}

#[cfg(all(feature = "alloc", feature = "repr-rust"))]
unsafe fn encode_vec_unchecked<'a, T: ReprC<CType: Copy> + 'a>(
    source: &'a [T],
    capacity: usize,
    stores: &mut Box<[<&'a T as EncodeMoveUnchecked>::SoftStore]>,
) -> Vec<T::CType>
where
    &'a T: EncodeMoveUnchecked<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
{
    crate::stored::map_vec(source.iter(), capacity, stores, |item, store| unsafe {
        encode_ref_field_unchecked::<_>(item, store)
    })
}

#[cfg(all(test, feature = "alloc", feature = "derive"))]
mod tests {
    use alloc::{boxed::Box, string::String, vec::Vec};
    use core::ffi::CStr;

    use rust_spec::RustSpec;

    use crate::{
        ReprC,
        convert::{
            EncodeMove, EncodeMoveUnchecked, SoftEncodeMut, SoftEncodeMutUnchecked, SoftEncodeRef,
            SoftEncodeRefUnchecked, Store,
        },
        lang,
    };

    macro_rules! assert_not_shared_encoder {
        ($ty:ty: $($encoder:path),+ $(,)?) => {
            const _: fn() = || {
                $(
                    {
                        trait AmbiguousIfImpl<A> { fn check() {} }
                        impl<T: ?Sized> AmbiguousIfImpl<()> for T {}
                        impl<T: ?Sized + $encoder> AmbiguousIfImpl<u8> for T
                        where
                            &'static T: crate::ReprC<CType: crate::CType + crate::CType + Sized>,
                        {}
                        let _ = <$ty as AmbiguousIfImpl<_>>::check;
                    }
                )+
            };
        };
    }

    macro_rules! assert_shared_encoder {
        ($ty:ty: $($encoder:path),+ $(,)?) => {
            const _: fn() = || {
                fn check<T: ?Sized $(+ $encoder)+>()
                where
                    &'static T: crate::ReprC<CType: crate::CType + crate::CType + Sized>,
                {}
                check::<$ty>();
            };
        };
    }

    // No Clone implementation: the stable vector path must not require one.
    #[derive(RustSpec, crate::ReprC)]
    #[repr(transparent)]
    #[repr_c(identity)]
    struct NoClone(u32);

    #[test]
    fn stable_vec_borrows_non_clone_elements() {
        let values = alloc::vec![NoClone(7), NoClone(11)];
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&values, &mut store) };
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), values.as_ptr());
        assert_eq!(header.len(), 2);
        assert_eq!(unsafe { (*header.as_ptr()).0 }, 7);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(values[1].0, 11);

        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode_unchecked(&values, &mut store) };
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), values.as_ptr());
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn nested_shared_vec_refs_borrow_elements() {
        let values = alloc::vec![NoClone(7), NoClone(11)];
        let reference = &values;
        let nested = &reference;
        let mut store = Default::default();
        // Use the free function to prevent method-call autoderef from hiding a
        // missing nested-reference implementation.
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(nested, &mut store) };
        let header = unsafe { &*(*encoded.as_ptr()).as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), values.as_ptr());
        assert_eq!(header.len(), 2);
        assert_eq!(unsafe { (*header.as_ptr()).0 }, 7);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode_unchecked(&nested, &mut store) };
        let header = unsafe { &*(*(*encoded.as_ptr()).as_ptr()).as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), values.as_ptr());
        assert_eq!(header.len(), 2);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(values[1].0, 11);
    }

    #[test]
    fn stable_vec_handles_empty_zst_and_non_robust_elements() {
        for values in [alloc::vec![], alloc::vec![false, true]] {
            let mut store = Default::default();
            let encoded = unsafe { crate::soft_encode::<_, lang::C>(&values, &mut store) };
            let header = unsafe { &*encoded.as_ptr() };
            assert_eq!(header.as_ptr().cast::<bool>().cast_const(), values.as_ptr());
            assert_eq!(header.len(), values.len());
            assert_eq!(store.sync(), Some(()));
        }
        let values = alloc::vec![(); 3];
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&values, &mut store) };
        assert_eq!(unsafe { (*encoded.as_ptr()).len() }, 3);
        assert_eq!(store.sync(), Some(()));
    }

    #[derive(Clone, RustSpec, crate::ReprC)]
    struct Record<T>(T);

    // The layout of this downstream type is projected through its foreign field.
    #[derive(Clone, RustSpec, crate::ReprC)]
    #[repr(transparent)]
    struct Projected((u32,));

    #[test]
    fn derived_shared_refs_support_field_dependent_layouts() {
        let value = Record((13_u32,));
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&value, &mut store) };
        assert_eq!(unsafe { (*encoded.as_ptr()).0.0.0 }, 13);
        assert_eq!(store.sync(), Some(()));

        let value = Projected((17,));
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode_unchecked(&value, &mut store) };
        assert_eq!(unsafe { (*encoded.as_ptr()).0.0.0 }, 17);
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn unstable_vec_converts_elements() {
        let values = alloc::vec![(19_u32,), (23,)];
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&values, &mut store) };
        let header = unsafe { *encoded.as_ptr() };
        assert_ne!(
            header.as_ptr().cast::<(u32,)>().cast_const(),
            values.as_ptr()
        );
        assert_eq!(header.len(), 2);
        assert_eq!(unsafe { (*header.as_ptr()).0.0 }, 19);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(values, [(19,), (23,)]);
    }

    #[test]
    fn checked_vec_of_cells_borrows_element_storage() {
        use core::cell::Cell;
        let values = alloc::vec![Cell::new(29_u32)];
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&values, &mut store) };
        let header = unsafe { *encoded.as_ptr() };
        assert_eq!(
            header.as_ptr().cast::<Cell<u32>>().cast_const(),
            values.as_ptr()
        );
        unsafe { header.as_ptr().cast::<u32>().write(31) };
        assert_eq!(values[0].get(), 31);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode_unchecked(&values, &mut store) };
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(
            header.as_ptr().cast::<Cell<u32>>().cast_const(),
            values.as_ptr()
        );
        assert_eq!(store.sync(), Some(()));
    }

    #[derive(RustSpec)]
    struct RefOnly(u32);

    crate::borrow::__impl_borrowed_identity!([] RefOnly);

    impl crate::ReprC for RefOnly {
        type CType = u32;
    }

    #[derive(Default)]
    struct CheckedStore(bool);
    impl Store for CheckedStore {
        fn sync(self) -> Option<()> {
            self.0.then_some(())
        }
    }

    // This downstream type needs neither Clone nor owned encoding. Its checked
    // shared conversion has state, whereas its unchecked conversion has none.
    unsafe impl<'a> crate::convert::SoftEncodeRefUnchecked<'a> for RefOnly {
        type SoftStore = crate::sync::RefSizedEncodeStore<Self, ()>;
        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|_fields| self.0)
        }
    }
    unsafe impl<'a> crate::convert::SoftEncodeRef<'a, lang::C> for RefOnly {
        type Store = crate::sync::RefSizedEncodeStore<Self, CheckedStore>;
        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                fields.0 = true;
                self.0
            })
        }
    }

    #[test]
    fn downstream_shared_only_type_keeps_independent_stores() {
        let value = RefOnly(37);
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&value, &mut store) };
        assert_eq!(unsafe { *encoded.as_ptr() }, 37);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode_unchecked(&value, &mut store) };
        assert_eq!(unsafe { *encoded.as_ptr() }, 37);
        assert_eq!(store.sync(), Some(()));
    }

    #[derive(RustSpec, crate::ReprC)]
    struct Nested {
        values: Vec<NoClone>,
        text: String,
        boxed: Box<(u32,)>,
    }

    #[derive(RustSpec, crate::ReprC)]
    struct Transparent<T>(T);

    #[derive(RustSpec, crate::ReprC)]
    #[repr(u8)]
    enum Choice {
        #[repr_c(allow_zero_tag)]
        Empty,
        Value(RefOnly),
    }

    #[test]
    fn derived_struct_and_transparent_wrapper_delegate_without_clone() {
        let value = Nested {
            values: alloc::vec![NoClone(41)],
            text: String::from("borrowed"),
            boxed: Box::new((43,)),
        };
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&value, &mut store) };
        let fields = unsafe { &*encoded.as_ptr() };
        assert_eq!(fields.values.as_ptr().cast_const(), value.values.as_ptr());
        assert_eq!(fields.text.as_ptr().cast_const(), value.text.as_ptr());
        assert_eq!(fields.text.capacity(), value.text.capacity());
        assert_eq!(unsafe { (*fields.boxed.as_ptr()).0.0 }, 43);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(value.values[0].0, 41);

        let wrapped = Transparent(value.values);
        let mut store = Default::default();
        let header = unsafe { crate::soft_encode::<_, lang::C>(&wrapped, &mut store) };
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(header.0.as_ptr().cast_const(), wrapped.0.as_ptr());
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn tuples_arrays_options_results_and_manually_drop_delegate() {
        let value = (alloc::vec![NoClone(47)],);
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&value, &mut store) };
        assert_eq!(
            unsafe { (*encoded.as_ptr()).0.0.as_ptr().cast_const() },
            value.0.as_ptr()
        );
        assert_eq!(store.sync(), Some(()));

        let array = [value.0];
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode_unchecked(&array, &mut store) };
        let encoded = unsafe { &*encoded.as_ptr() };
        assert_eq!(encoded[0].as_ptr().cast_const(), array[0].as_ptr());
        assert_eq!(store.sync(), Some(()));

        let elements = alloc::vec![NoClone(53)];
        let pointer = elements.as_ptr();
        let value = Some(elements);
        let mut store = Default::default();
        let header = unsafe { crate::soft_encode::<_, lang::C>(&value, &mut store) };
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(header.0.0.as_ptr().cast_const(), pointer);
        assert_eq!(store.sync(), Some(()));

        let Some(value) = value else { unreachable!() };
        let value = Ok::<_, ()>(value);
        let mut store = Default::default();
        let header = unsafe { crate::soft_encode::<_, lang::C>(&value, &mut store) };
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(header.0.0.as_ptr().cast_const(), pointer);
        assert_eq!(store.sync(), Some(()));

        let Ok(value) = value else { unreachable!() };
        let value = core::mem::ManuallyDrop::new(value);
        let mut store = Default::default();
        let header = unsafe { crate::soft_encode::<_, lang::C>(&value, &mut store) };
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), value.as_ptr());
        assert_eq!(store.sync(), Some(()));
        drop(core::mem::ManuallyDrop::into_inner(value));
    }

    #[test]
    fn unstable_vec_delegates_to_non_clone_shared_only_elements() {
        let values = alloc::vec![(RefOnly(59),), (RefOnly(61),)];
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&values, &mut store) };
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(unsafe { (*header.as_ptr()).0.0 }, 59);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode_unchecked(&values, &mut store) };
        assert_eq!(unsafe { (*(*encoded.as_ptr()).as_ptr()).0.0 }, 59);
        assert_eq!(store.sync(), Some(()));
        let boxed = values.into_boxed_slice();
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&boxed, &mut store) };
        assert_eq!(unsafe { (*(*encoded.as_ptr()).as_ptr()).0.0 }, 59);
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn derived_enum_syncs_only_its_active_shared_store() {
        let mut store = Default::default();
        let _encoded =
            unsafe { crate::soft_encode::<_, lang::C>(&Choice::Value(RefOnly(67)), &mut store) };
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let _encoded =
            unsafe { crate::soft_encode::<_, lang::C>(&Choice::Value(RefOnly(71)), &mut store) };
        let _encoded = unsafe { crate::soft_encode::<_, lang::C>(&Choice::Empty, &mut store) };
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn buffers_and_boxes_borrow_original_allocations() {
        let value = Box::new(NoClone(73));
        let header = unsafe { crate::soft_encode::<_, lang::C>(&value, &mut ()) };
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), &*value as *const NoClone);
        assert_eq!(().sync(), Some(()));

        let value = alloc::vec![NoClone(79)].into_boxed_slice();
        let mut store = Default::default();
        let header = unsafe { crate::soft_encode::<_, lang::C>(&value, &mut store) };
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), value.as_ptr());
        assert_eq!(store.sync(), Some(()));

        let value = alloc::ffi::CString::new("borrowed").unwrap();
        let mut store = Default::default();
        let header = unsafe { crate::soft_encode::<_, lang::C>(&value, &mut store) };
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(
            header.as_ptr().cast::<core::ffi::c_char>().cast_const(),
            value.as_ptr()
        );
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn tagged_wrappers_delegate_and_clear_inactive_stores() {
        let mut store = Default::default();
        let encoded = unsafe {
            crate::soft_encode::<_, lang::C>(&crate::option::Option::Some(RefOnly(83)), &mut store)
        };
        let encoded = unsafe { &*encoded.as_ptr() };
        assert_eq!(encoded.into_option(), Some(Some(83)));
        let encoded = unsafe {
            crate::soft_encode::<_, lang::C>(&crate::option::Option::<RefOnly>::None, &mut store)
        };
        let encoded = unsafe { &*encoded.as_ptr() };
        assert_eq!(encoded.into_option(), Some(None));
        let value = *encoded;
        store.encode(|fields| {
            assert!(fields.is_none());
            value
        });
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded =
            unsafe { crate::soft_encode::<_, lang::C>(&Ok::<_, u32>(RefOnly(89)), &mut store) };
        let encoded = unsafe { &*encoded.as_ptr() };
        assert_eq!(encoded.into_result(), Some(Ok(89)));
        let encoded =
            unsafe { crate::soft_encode::<_, lang::C>(&Err::<RefOnly, _>(97_u32), &mut store) };
        let encoded = unsafe { &*encoded.as_ptr() };
        assert_eq!(encoded.into_result(), Some(Err(97)));
        let value = *encoded;
        store.encode(|fields| {
            assert!(matches!(fields, Some(Err(()))));
            value
        });
        assert_eq!(store.sync(), Some(()));

        let some = Ok::<_, ()>(core::num::NonZeroU8::new(1).unwrap());
        let _ = unsafe { crate::soft_encode::<_, lang::C>(&some, &mut ()) };
        let encoded = unsafe {
            crate::soft_encode::<_, lang::C>(&Err::<core::num::NonZeroU8, _>(()), &mut ())
        };
        assert_eq!(unsafe { *encoded.as_ptr() }.0.0.0, 0);
        assert_eq!(().sync(), Some(()));
    }

    #[test]
    fn boxed_borrowed_fields_do_not_require_static_lifetimes() {
        let value = 101_u32;
        let boxed = Box::new((&value,));
        let mut store = Default::default();
        let header = unsafe { crate::soft_encode::<_, lang::C>(&boxed, &mut store) };
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(
            unsafe { (*header.as_ptr()).0.0.as_ptr() },
            &value as *const _
        );
        assert_eq!(store.sync(), Some(()));
    }

    #[derive(RustSpec, crate::ReprC)]
    #[rust_spec(non_robust)]
    #[repr_c(is_valid = |value| *value != 0)]
    struct Validated(u32);

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "encoding failed custom is_valid")]
    fn derived_shared_encoding_preserves_custom_validation() {
        unsafe { crate::soft_encode::<_, lang::C>(&Validated(0), &mut Default::default()) };
    }

    #[test]
    fn cells_cannot_be_snapshotted_by_shared_encoders() {
        use core::cell::{Cell, UnsafeCell};

        use static_assertions::{assert_impl_all, assert_not_impl_any};

        use crate::convert::{SoftEncodeRef, SoftEncodeRefUnchecked};

        assert_not_shared_encoder!(Cell<u32>: SoftEncodeRefUnchecked<'static>, SoftEncodeRef<'static>);
        assert_not_shared_encoder!(UnsafeCell<u32>: SoftEncodeRefUnchecked<'static>, SoftEncodeRef<'static>);
        assert_not_impl_any!(&Vec<Cell<bool>>: EncodeMove);
        assert_not_shared_encoder!(Box<Cell<bool>>: SoftEncodeRef<'static>);
        assert_not_impl_any!(&Box<[Cell<bool>]>: EncodeMove);
        assert_not_impl_any!(&Vec<UnsafeCell<bool>>: EncodeMove);
        assert_not_impl_any!(&Record<Cell<u32>>: EncodeMove, EncodeMoveUnchecked);
        assert_impl_all!(&Vec<Cell<u32>>: EncodeMove, EncodeMoveUnchecked);
        assert_impl_all!(&Box<Cell<u32>>: EncodeMove, EncodeMoveUnchecked);
        assert_impl_all!(&Box<[Cell<u32>]>: EncodeMove, EncodeMoveUnchecked);
        assert_impl_all!(&Vec<UnsafeCell<u32>>: EncodeMove, EncodeMoveUnchecked);
    }

    #[test]
    fn checked_boxes_of_cells_preserve_storage() {
        use core::cell::Cell;
        let value = Box::new(Cell::new(7_u32));
        let mut store = ();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&value, &mut store) };
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(header.as_ptr().cast::<Cell<u32>>().cast_const(), &*value);
        assert_eq!(store.sync(), Some(()));

        let values = alloc::vec![Cell::new(11_u32)].into_boxed_slice();
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&values, &mut store) };
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(
            header.as_ptr().cast::<Cell<u32>>().cast_const(),
            values.as_ptr()
        );
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn aggregates_encode_stable_fields_without_encode_ref_impls() {
        use crate::convert::{SoftEncodeRef, SoftEncodeRefUnchecked};
        assert_not_shared_encoder!(u32: SoftEncodeRefUnchecked<'static>, SoftEncodeRef<'static>);

        assert_not_shared_encoder!(Record<NoClone>: SoftEncodeRefUnchecked<'static>, SoftEncodeRef<'static>);
        assert_not_shared_encoder!((NoClone,): SoftEncodeRefUnchecked<'static>, SoftEncodeRef<'static>);

        let value = Record(109_u32);
        let mut store = Default::default();
        let encoded: <&Record<u32> as crate::ReprC>::CType =
            unsafe { crate::soft_encode::<_, lang::C>(&value, &mut store) };
        assert_eq!(unsafe { (*encoded.as_ptr()).0 }, 109);
        assert_eq!(store.sync(), Some(()));

        let tuple = (113_u32,);
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode_unchecked(&tuple, &mut store) };
        assert_eq!(unsafe { (*encoded.as_ptr()).0.0 }, 113);
        assert_eq!(store.sync(), Some(()));
    }

    #[derive(RustSpec, crate::ReprC)]
    #[repr(transparent)]
    #[repr_c(identity)]
    struct InteriorIdentity(core::cell::UnsafeCell<u32>);

    #[test]
    fn stable_identity_reference_preserves_interior_storage() {
        let value = InteriorIdentity(core::cell::UnsafeCell::new(127));
        let mut store = ();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&value, &mut store) };
        assert_eq!(encoded.as_ptr(), &value as *const InteriorIdentity);
        unsafe { *(*encoded.as_ptr()).0.get() = 131 };
        assert_eq!(unsafe { *value.0.get() }, 131);
    }

    #[test]
    fn shared_borrow_of_mutable_reference_keeps_vec_shared() {
        let mut values = alloc::vec![NoClone(7), NoClone(11)];
        let pointer = values.as_ptr();
        let reference = &mut values;
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&reference, &mut store) };
        let header = unsafe { &*(*encoded.as_ptr()).as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), pointer);
        assert_eq!(header.len(), 2);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(reference[1].0, 11);

        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode_unchecked(&reference, &mut store) };
        let header = unsafe { &*(*encoded.as_ptr()).as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), pointer);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(reference[0].0, 7);
    }

    #[test]
    fn shared_borrow_of_mutable_slice_reference_preserves_metadata() {
        assert_not_shared_encoder!(&mut Vec<core::cell::Cell<bool>>:
            crate::convert::SoftEncodeRef<'static>);
        let mut values = [NoClone(3), NoClone(5)];
        let pointer = values.as_ptr();
        let reference = &mut values[..];
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&reference, &mut store) };
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), pointer);
        assert_eq!(header.metadata(), 2);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(reference[1].0, 5);
    }

    // Neither the element nor its owned slice supports cloning.
    #[derive(RustSpec, crate::ReprC, Debug, PartialEq)]
    struct Element(Vec<u32>);

    #[test]
    fn shared_unstable_slice_encodes_without_clone() {
        static_assertions::assert_not_impl_any!([Element]: SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
        let values = [Element(alloc::vec![3, 5]), Element(alloc::vec![7])];
        let original = values[0].0.as_ptr();
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(values.as_slice(), &mut store) };
        assert_eq!(encoded.metadata(), 2);
        let field = unsafe { &(*encoded.as_ptr()).0 };
        assert_eq!(field.as_ptr().cast_const(), original);
        assert_eq!(unsafe { *field.as_ptr().add(1) }, 5);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode_unchecked(values.as_slice(), &mut store) };
        assert_eq!(encoded.metadata(), 2);
        assert_eq!(
            unsafe { (*encoded.as_ptr()).0.0.as_ptr().cast_const() },
            original
        );
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn empty_unstable_slices_encode_and_sync() {
        let values: [Element; 0] = [];
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(values.as_slice(), &mut store) };
        assert_eq!(encoded.metadata(), 0);
        assert!(!encoded.as_ptr().is_null());
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn nul_terminated_reference_uses_explicit_encoder_without_storage() {
        let value = c"borrowed";
        let encoded: <&core::ffi::CStr as ReprC>::CType =
            unsafe { crate::soft_encode::<_, lang::C>(value, &mut Default::default()) };
        assert_eq!(encoded.as_ptr().cast::<core::ffi::c_char>(), value.as_ptr());
        let encoded = unsafe { crate::soft_encode_unchecked(value, &mut Default::default()) };
        assert_eq!(encoded.as_ptr().cast::<core::ffi::c_char>(), value.as_ptr());
        let wrapped: &core::mem::ManuallyDrop<core::ffi::CStr> = unsafe {
            &*(value as *const core::ffi::CStr as *const core::mem::ManuallyDrop<core::ffi::CStr>)
        };
        assert_eq!(
            unsafe { crate::soft_encode::<_, lang::C>(wrapped, &mut Default::default()) }
                .as_ptr()
                .cast(),
            encoded.as_ptr()
        );
    }

    // A downstream unsized type can implement reference encoding without ToOwned.
    #[derive(RustSpec)]
    #[repr(transparent)]
    struct SliceView([Element]);
    impl ReprC for SliceView {
        type CType = <[Element] as ReprC>::CType;
    }
    unsafe impl<'a> SoftEncodeRefUnchecked<'a> for SliceView {
        type SoftStore = <[Element] as SoftEncodeRefUnchecked<'a>>::SoftStore;
        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = unsafe { crate::soft_encode_unchecked(&self.0, store) };
            unsafe {
                crate::wide::abi::SoftWideRef::<Self::CType>::from_raw_parts(
                    inner.as_ptr(),
                    inner.metadata(),
                )
            }
        }
    }
    unsafe impl<'a> SoftEncodeRef<'a, lang::C> for SliceView {
        type Store = <[Element] as SoftEncodeRef<'a>>::Store;
        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = unsafe { crate::soft_encode::<_, lang::C>(&self.0, store) };
            unsafe {
                crate::wide::abi::SoftWideRef::<Self::CType>::from_raw_parts(
                    inner.as_ptr(),
                    inner.metadata(),
                )
            }
        }
    }

    #[test]
    fn downstream_unsized_type_delegates_through_reference_traits() {
        let mut values = [Element(alloc::vec![23])];
        let value = unsafe { &mut *(values.as_mut_slice() as *mut [Element] as *mut SliceView) };
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&*value, &mut store) };
        assert_eq!(encoded.metadata(), 1);
        assert_eq!(store.sync(), Some(()));

        let wrapped: &core::mem::ManuallyDrop<SliceView> =
            unsafe { &*(value as *const SliceView as *const core::mem::ManuallyDrop<SliceView>) };
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(wrapped, &mut store) };
        assert_eq!(encoded.metadata(), 1);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode_unchecked(wrapped, &mut store) };
        assert_eq!(encoded.metadata(), 1);
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn boxed_custom_wide_type_delegates_to_pointee_encoder() {
        let values: Box<[Element]> = Box::new([Element(alloc::vec![23])]);
        let boxed: Box<SliceView> =
            unsafe { Box::from_raw(Box::into_raw(values) as *mut SliceView) };
        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&boxed, &mut store) };
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(header.metadata(), 1);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = unsafe { crate::soft_encode_unchecked(&boxed, &mut store) };
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(header.metadata(), 1);
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn stable_slice_paths_remain_store_free() {
        let mut values = [31_u32, 37];
        let encoded = unsafe { crate::encode::<_, lang::C>(values.as_slice()) };
        assert_eq!(encoded.as_ptr(), values.as_ptr());
        let original = values.as_mut_ptr();
        let encoded = unsafe { crate::encode::<_, lang::C>(values.as_mut_slice()) };
        assert_eq!(encoded.as_ptr(), original);
    }

    #[repr(C)]
    #[derive(RustSpec, ReprC)]
    struct CStrTail {
        tag: u32,
        tail: CStr,
    }

    #[repr(transparent)]
    #[derive(RustSpec, ReprC)]
    struct WrappedCStr(CStr);

    #[test]
    fn transparent_cstr_has_nul_terminated_buffer() {
        static_assertions::assert_impl_all!(WrappedCStr: crate::ffi::NulTerminatedBuf);
        assert_shared_encoder!(WrappedCStr: crate::convert::SoftEncodeRef<'static>, crate::convert::SoftEncodeRefUnchecked<'static>);
        static_assertions::assert_not_impl_any!(WrappedCStr:
            crate::convert::SoftEncodeMut<'static>, crate::convert::SoftEncodeMutUnchecked<'static>);

        let value = c"hello";
        let wrapper = unsafe { &*(value as *const CStr as *const WrappedCStr) };
        let mut store = ();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(wrapper, &mut store) };
        assert_eq!(unsafe { CStr::from_ptr(encoded.as_ptr().cast()) }, value);
        assert_eq!(store.sync(), Some(()));
        let mut store = ();
        let encoded = unsafe { crate::soft_encode_unchecked(wrapper, &mut store) };
        assert_eq!(unsafe { CStr::from_ptr(encoded.as_ptr().cast()) }, value);
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn nontransparent_nul_tail_has_no_reference_encoder() {
        assert_not_shared_encoder!(CStrTail: crate::convert::SoftEncodeRef<'static>, crate::convert::SoftEncodeRefUnchecked<'static>);
        static_assertions::assert_not_impl_any!(CStrTail: crate::convert::SoftEncodeMut<'static>, crate::convert::SoftEncodeMutUnchecked<'static>);
    }

    #[repr(C)]
    #[derive(RustSpec, ReprC)]
    struct Packet {
        tag: u32,
        tail: [u8],
    }

    #[repr(transparent)]
    #[derive(RustSpec, ReprC)]
    struct WrappedSlice<T: ?Sized>(T);

    #[test]
    fn transparent_wide_wrapper_delegates_reference_encoding() {
        use crate::convert::{SoftEncodeMut, SoftEncodeMutUnchecked, SoftEncodeRefUnchecked};
        assert_shared_encoder!(WrappedSlice<u32>: SoftEncodeRef<'static>, SoftEncodeRefUnchecked<'static>,
            SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
        assert_shared_encoder!(WrappedSlice<core::ffi::CStr>: SoftEncodeRef<'static>, SoftEncodeRefUnchecked<'static>);
        static_assertions::assert_not_impl_any!(WrappedSlice<core::ffi::CStr>:
            SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
        assert_shared_encoder!(WrappedSlice<[u32]>: SoftEncodeRef<'static>, SoftEncodeRefUnchecked<'static>,
            SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
        assert_shared_encoder!(WrappedSlice<Packet>: SoftEncodeRef<'static>, SoftEncodeRefUnchecked<'static>,
            SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
        let mut values = [1_u32, 2];
        let wrapper = unsafe { &mut *(&mut values[..] as *mut [u32] as *mut WrappedSlice<[u32]>) };
        let mut store = ();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&*wrapper, &mut store) };
        assert_eq!(&unsafe { encoded.into_ref().unwrap() }.0, &[1, 2]);
        assert_eq!(store.sync(), Some(()));
        let mut store = ();
        let encoded = unsafe { crate::soft_encode::<_, lang::C>(&mut *wrapper, &mut store) };
        unsafe { encoded.as_ptr().cast::<u32>().write(3) };
        assert_eq!(store.sync(), Some(()));
        assert_eq!(values, [3, 2]);
    }

    #[repr(C)]
    #[derive(RustSpec, ReprC)]
    struct GenericPacket<T: ?Sized> {
        tag: u32,
        tail: T,
    }

    #[test]
    fn derived_wide_aggregate_reference_encoders_are_unsupported() {
        use crate::convert::{SoftEncodeMut, SoftEncodeMutUnchecked, SoftEncodeRefUnchecked};
        assert_not_shared_encoder!(Packet: SoftEncodeRef<'static>, SoftEncodeRefUnchecked<'static>);
        static_assertions::assert_not_impl_any!(Packet: SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
        assert_shared_encoder!(GenericPacket<u8>: SoftEncodeRef<'static>, SoftEncodeRefUnchecked<'static>);
        assert_not_shared_encoder!(GenericPacket<[u8]>: SoftEncodeRef<'static>, SoftEncodeRefUnchecked<'static>);
        static_assertions::assert_not_impl_any!(GenericPacket<[u8]>: SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
        assert_not_shared_encoder!(GenericPacket<str>: SoftEncodeRef<'static>, SoftEncodeRefUnchecked<'static>);
        static_assertions::assert_not_impl_any!(GenericPacket<str>: SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
    }
}
