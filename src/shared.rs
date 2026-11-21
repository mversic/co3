//! Referent-specific encoding for the unstable shared-reference path.
use crate::marker::Native;
#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
use core::{marker::PhantomData, mem::ManuallyDrop};

use disjoint_impls::disjoint_impls;
use rust_spec::{
    RustSpec,
    layout::LayoutKind,
    mutability::{Exclusive, ExclusiveKind},
    niche::{WithNiche, WithoutNiche},
    size::{MetaSized, NulTerminated, Sized as RustSpecSized, SizedKind, SliceLike, Zero},
};
#[cfg(feature = "alloc")]
use rust_spec::{Stable, Unstable, layout::Robust, mutability::WithInterior};

use crate::{
    CType, ReprC,
    convert::{
        EncodeMove, EncodeMoveUnchecked, Store, encode_ref_field, encode_ref_field_unchecked,
    },
    niche::Niche,
    option::abi::{Option as AbiOption, OptionSome as AbiOptionSome},
    reference::abi::SoftRef,
    result::abi::{Result as AbiResult, ResultErr as AbiResultErr, ResultOk as AbiResultOk},
    sync::{ArrayStore, RefSizedEncodeStore},
    wide::{Wide, WideHeader, WideRefCarrier, abi::SoftWideRef as AbiSoftWideRef},
};
#[cfg(feature = "alloc")]
use crate::{
    boxed::abi::Box as AbiBox,
    marker::AbiCompatible,
    sync::{RefVecEncodeStore, RefWideEncodeStore},
    vec::abi::Vec as AbiVec,
    wide::abi::WideBox as AbiWideBox,
};

#[cfg(feature = "alloc")]
trait SharedVecMutability {}

#[cfg(feature = "alloc")]
impl<I: ExclusiveKind> SharedVecMutability for Exclusive<I> {}
#[cfg(feature = "alloc")]
impl SharedVecMutability for WithInterior<Robust> {}

disjoint_impls! {
    /// Encodes a shared reference using conversion storage without transferring ownership.
    ///
    /// # Safety
    ///
    /// See [`crate::soft_encode_unchecked`].
    pub unsafe trait SoftEncodeRefUnchecked<'a>: ReprC + 'a
    where
        &'a Self: ReprC<CType: CType + Sized>,
    {
        type SoftStore: Store + Default;
        /// Returns a borrowed reference carrier. Neither the referent nor any
        /// backing allocation is transferred to the caller.
        ///
        /// # Safety
        ///
        /// See [`crate::soft_encode_unchecked`].
        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm;
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: ReprC<CType: CType + Sized> + ?Sized + 'a>
        SoftEncodeRefUnchecked<'a> for Box<R>
    where
        R: RustSpec,
        &'a R: EncodeMoveUnchecked + ReprC<CType = SoftRef<<R as ReprC>::CType>>,
        Self: ReprC<CType = AbiBox<<R as ReprC>::CType>>,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
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
    unsafe impl<'a, R: 'a + RustSpec + ?Sized> SoftEncodeRefUnchecked<'a> for Box<R>
    where
        R: RustSpec<Size = MetaSized<SliceLike>>
            + ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType>>>,
        &'a R: EncodeMoveUnchecked + ReprC<CType: crate::CType + WideRefCarrier<R>>,
        Self: ReprC<CType = AbiWideBox<<R as ReprC>::CType>>,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
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
    unsafe impl<'a, R: AbiCompatible, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a> for Vec<R>
    where
        &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
        <R as ReprC>::CType: crate::CType + Sized,
        R: RustSpec<Layout = Stable>,
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
            store.encode(|_fields| Native(unsafe {
                AbiVec::from_raw_parts(self.as_ptr().cast_mut().cast(), self.len(), self.capacity())
            }))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: ReprC, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a> for Vec<R>
    where
        &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
        <R as ReprC>::CType: crate::CType + Copy,
        R: RustSpec<Layout = Unstable>,
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
                Native(store.encode(|stores| unsafe { encode_vec_unchecked(self, self.capacity(), stores) }))
            })
        }
    }

    unsafe impl<'a, R: ReprC, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a> for Option<R>
    where
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        R: RustSpec<Niche = WithoutNiche>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMoveUnchecked + ReprC<CType: crate::CType + crate::reference::RefCarrier<R>>,
        <R as ReprC>::CType: crate::CType + Copy,
    {
        type SoftStore = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMoveUnchecked>::SoftStore>>;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {Native({
                let store = fields;

                match self {
                    Some(value) => AbiOption::Some(unsafe {
                        encode_ref_field_unchecked(value, store.insert(Default::default()))
                    }),
                    None => {
                        *store = None;
                        AbiOption::None()
                    }
                }
            })})
        }
    }
    unsafe impl<'a, R: Niche, N: LayoutKind, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a>
        for Option<R>
    where
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        R: RustSpec<Niche = WithNiche<N>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMoveUnchecked + ReprC<CType: crate::CType + crate::reference::RefCarrier<R>>,
    Self: ReprC<CType = Native<AbiOptionSome<<R as ReprC>::CType>>>,
{
        type SoftStore = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMoveUnchecked>::SoftStore>>;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {Native({
                let store = fields;

                match self {
                    Some(value) => AbiOptionSome(unsafe {
                        encode_ref_field_unchecked(
                            value,
                            store.insert(Default::default()),
                        )
                    }),
                    None => {
                        *store = None;
                        AbiOptionSome(R::NICHE)
                    }
                }
            })})
        }
    }

    unsafe impl<'a, R: ReprC, E: ReprC, K: SizedKind, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a>
        for Result<R, E>
    where
        &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        <R as ReprC>::CType: crate::CType + Copy,
        <E as ReprC>::CType: crate::CType + Copy,
        Self: ReprC<CType = Native<AbiResult<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        R: RustSpec<Size = RustSpecSized<K>>,
        E: RustSpec<Size = RustSpecSized<K>>,
        &'a R: EncodeMoveUnchecked + ReprC<CType: crate::CType + crate::reference::RefCarrier<R>>,
        &'a E: EncodeMoveUnchecked + ReprC<CType: crate::CType + crate::reference::RefCarrier<E>>,
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
            store.encode(|fields| {Native({
                let store = fields;

                match self {
                    Ok(ok) => {
                        let Result::Ok(inner) = store.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        AbiResult::Ok(unsafe { encode_ref_field_unchecked(ok, inner) })
                    }
                    Err(err) => {
                        let Result::Err(inner) = store.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        AbiResult::Err(unsafe { encode_ref_field_unchecked(err, inner) })
                    }
                }
            })})
        }
    }
    unsafe impl<'a, R: Niche, E: ReprC<CType: CType + Sized>, N: LayoutKind, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a>
        for Result<R, E>
    where
        &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
        Self: ReprC<CType = Native<AbiResultOk<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMoveUnchecked + ReprC<CType: crate::CType + crate::reference::RefCarrier<R>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMoveUnchecked>::SoftStore>>;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {Native({
                let store = fields;

                match self {
                    Ok(ok) => AbiResultOk(unsafe {
                        encode_ref_field_unchecked(ok, store.insert(Default::default()))
                    }, PhantomData),
                    Err(_) => {
                        *store = None;
                        AbiResultOk(R::NICHE, PhantomData)
                    }
                }
            })})
        }
    }
    unsafe impl<'a, R: ReprC<CType: CType + Sized>, E: Niche, N: LayoutKind, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a>
        for Result<R, E>
    where
        &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
        Self: ReprC<CType = Native<AbiResultErr<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a E: EncodeMoveUnchecked + ReprC<CType: crate::CType + crate::reference::RefCarrier<E>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, Option<<&'a E as EncodeMoveUnchecked>::SoftStore>>;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {Native({
                let store = fields;

                match self {
                    Ok(_) => {
                        *store = None;
                        AbiResultErr(E::NICHE, PhantomData)
                    }
                    Err(err) => AbiResultErr(unsafe {
                        encode_ref_field_unchecked(err, store.insert(Default::default()))
                    }, PhantomData),
                }
            })})
        }
    }

    unsafe impl<'a, R: ReprC<CType: CType + Sized> + ?Sized, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a>
        for ManuallyDrop<R>
    where
        Self: ReprC<CType: CType + Sized>
            + RustSpec<Size: crate::Thin, Mutability = Exclusive<__Interior>>
            + 'a,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'a R: ReprC<CType = SoftRef<<R as ReprC>::CType>>,
        &'a R: EncodeMoveUnchecked,
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
    unsafe impl<'a, R: ReprC<CType: CType + Sized> + ?Sized, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a>
        for ManuallyDrop<R>
    where
        Self: ReprC<CType: CType + Sized>
            + RustSpec<Size = NulTerminated, Mutability = Exclusive<__Interior>>
            + 'a,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'a R: ReprC<CType = SoftRef<<R as ReprC>::CType>>,
        &'a R: EncodeMoveUnchecked,
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
    unsafe impl<
        'a,
        R: ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>> + ?Sized,
        __Interior: ExclusiveKind,
    > SoftEncodeRefUnchecked<'a> for ManuallyDrop<R>
    where
        Self: ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>
            + RustSpec<Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>
            + 'a,
        &'a Self: ReprC<CType = AbiSoftWideRef<<Self as ReprC>::CType>>,
        &'a R: ReprC<CType: crate::CType + WideRefCarrier<R, Pointer = *const crate::wide::PointeeData<<R as ReprC>::CType>>>,
        &'a R: EncodeMoveUnchecked,
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
            unsafe { AbiSoftWideRef::from_raw_parts(inner.as_ptr().cast(), inner.metadata()) }
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
    pub unsafe trait SoftEncodeRef<'a>: SoftEncodeRefUnchecked<'a>
    where &'a Self: ReprC<CType: CType + Sized>,
    {
        type Store: Store + Default;

        /// Returns access to the encoded referent without transferring ownership.
        /// The source and store must remain alive for any later access. Neither
        /// the referent nor its backing allocations may be freed or owned-decoded.
        fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType
        where
            &'a Self: ReprC<CType: CType + Sized>,
            'a: 'itm;
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: ReprC<CType: CType + Sized> + ?Sized + 'a>
        SoftEncodeRef<'a> for Box<R>
    where
        R: RustSpec,
        &'a R: EncodeMove + ReprC<CType = SoftRef<<R as ReprC>::CType>>,
        Self: ReprC<CType = AbiBox<<R as ReprC>::CType>>,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, <&'a R as EncodeMove>::Store>;

        fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = crate::convert::EncodeMove::soft_encode(&**self, fields);
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { AbiBox::from_raw(view.as_ptr().cast_mut()) }
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: RustSpec + 'a + ?Sized> SoftEncodeRef<'a> for Box<R>
    where
        R: RustSpec<Size = MetaSized<SliceLike>>
            + ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType>>>,
        &'a R: EncodeMove + ReprC<CType: crate::CType + WideRefCarrier<R>>,
        Self: ReprC<CType = AbiWideBox<<R as ReprC>::CType>>,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, <&'a R as EncodeMove>::Store>;

        fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = crate::convert::EncodeMove::soft_encode(&**self, fields);
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
    unsafe impl<
        'a,
        T: AbiCompatible<CType: CType + Sized>,
        __Interior: ExclusiveKind,
    > SoftEncodeRef<'a> for Vec<T>
    where
        &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
        T: RustSpec<Layout = Stable, Mutability: SharedVecMutability>,
        Self: ReprC<CType = Native<AbiVec<<T as ReprC>::CType>>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
    {
        type Store = RefSizedEncodeStore<Self, ()>;

        fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|_fields| Native(unsafe {
                AbiVec::from_raw_parts(self.as_ptr().cast_mut().cast(), self.len(), self.capacity())
            }))
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized>, __Interior: ExclusiveKind> SoftEncodeRef<'a>
        for Vec<T>
    where
        &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
        T: RustSpec<Layout = Unstable>,
        Self: crate::convert::SoftEncodeRefUnchecked<'a> + ReprC<CType = Native<AbiVec<<T as ReprC>::CType>>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a T: EncodeMove + ReprC<CType = SoftRef<<T as ReprC>::CType>>,
    {
        type Store =
            RefSizedEncodeStore<Self, RefVecEncodeStore<T, Box<[<&'a T as EncodeMove>::Store]>>>;

        fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {Native({
                let store = fields;

                store.encode(|stores| encode_vec(self, self.capacity(), stores))
            })})
        }
    }

    unsafe impl<'a, R: ReprC<CType: CType + Copy>, __Interior: ExclusiveKind> SoftEncodeRef<'a>
        for Option<R>
    where
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        R: RustSpec<Niche = WithoutNiche>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMove + ReprC<CType: crate::CType + crate::reference::RefCarrier<R>>,
    {
        type Store = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMove>::Store>>;

        fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {Native({
                let store = fields;

                match self {
                    Some(value) => {
                        AbiOption::Some(encode_ref_field(value, store.insert(Default::default())))
                    }
                    None => {
                        *store = None;
                        AbiOption::None()
                    }
                }
            })})
        }
    }
    unsafe impl<'a, R: Niche<CType: crate::CType + Copy>, N: LayoutKind, __Interior: ExclusiveKind> SoftEncodeRef<'a>
        for Option<R>
    where
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        R: RustSpec<Niche = WithNiche<N>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMove + ReprC<CType: crate::CType + crate::reference::RefCarrier<R>>,
    Self: ReprC<CType = Native<AbiOptionSome<<R as ReprC>::CType>>>,
{
        type Store = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMove>::Store>>;

        fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {Native({
                let store = fields;

                if let Some(value) = self {
                    let encoded = encode_ref_field(value, store.insert(Default::default()));
                    debug_assert!(!R::is_niche(&encoded), "encoding produced NICHE value");
                    AbiOptionSome(encoded)
                } else {
                    *store = None;
                    AbiOptionSome(R::NICHE)
                }
            })})
        }
    }

    unsafe impl<
        'a,
        R: ReprC<CType: CType + Copy>,
        E: ReprC<CType: CType + Copy>,
        K: SizedKind,
        __Interior: ExclusiveKind,
    > SoftEncodeRef<'a> for Result<R, E>
    where
        &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
        Self: ReprC<CType = Native<AbiResult<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        R: RustSpec<Size = RustSpecSized<K>>,
        E: RustSpec<Size = RustSpecSized<K>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMove + ReprC<CType: crate::CType + crate::reference::RefCarrier<R>>,
        &'a E: EncodeMove + ReprC<CType: crate::CType + crate::reference::RefCarrier<E>>,
    {
        type Store = RefSizedEncodeStore<
            Self,
            Option<Result<<&'a R as EncodeMove>::Store, <&'a E as EncodeMove>::Store>>,
        >;

        fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {Native({
                let store = fields;

                match self {
                    Ok(ok) => {
                        let Ok(store) = store.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        AbiResult::Ok(encode_ref_field(ok, store))
                    }
                    Err(err) => {
                        let Err(store) = store.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        AbiResult::Err(encode_ref_field(err, store))
                    }
                }
            })})
        }
    }
    unsafe impl<'a, R: Niche, E: ReprC<CType: CType + Sized>, N: LayoutKind, __Interior: ExclusiveKind> SoftEncodeRef<'a>
        for Result<R, E>
    where
        &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
        Self: ReprC<CType = Native<AbiResultOk<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMove + ReprC<CType: crate::CType + crate::reference::RefCarrier<R>>,
    {
        type Store = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMove>::Store>>;

        fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {Native({
                let store = fields;

                match self {
                    Ok(ok) => AbiResultOk(encode_ref_field(ok, store.insert(Default::default())), PhantomData),
                    Err(_) => {
                        *store = None;
                        AbiResultOk(R::NICHE, PhantomData)
                    }
                }
            })})
        }
    }
    unsafe impl<'a, R: ReprC<CType: CType + Sized>, E: Niche, N: LayoutKind, __Interior: ExclusiveKind> SoftEncodeRef<'a>
        for Result<R, E>
    where
        &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
        Self: ReprC<CType = Native<AbiResultErr<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a E: EncodeMove + ReprC<CType: crate::CType + crate::reference::RefCarrier<E>>,
    {
        type Store = RefSizedEncodeStore<Self, Option<<&'a E as EncodeMove>::Store>>;

        fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {Native({
                let store = fields;

                match self {
                    Ok(_) => {
                        *store = None;
                        AbiResultErr(E::NICHE, PhantomData)
                    }
                    Err(err) => AbiResultErr(encode_ref_field(err, store.insert(Default::default())), PhantomData),
                }
            })})
        }
    }

    unsafe impl<
        'a,
        R: RustSpec + 'a + ReprC<CType: CType + Sized> + ?Sized,
        __Interior: ExclusiveKind,
    > SoftEncodeRef<'a> for ManuallyDrop<R>
    where
        Self: ReprC<CType: CType + Sized>
            + RustSpec<Size: crate::Thin, Mutability = Exclusive<__Interior>>
            + 'a,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'a R: EncodeMove + ReprC<CType = SoftRef<<R as ReprC>::CType>>,
    {
        type Store = <&'a R as EncodeMove>::Store;

        fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = EncodeMove::soft_encode(&**self, store);
            // SAFETY: ManuallyDrop preserves the inner layout and metadata.
            unsafe { inner.cast() }
        }
    }
    unsafe impl<
        'a,
        R: RustSpec + 'a + ReprC<CType: CType + Sized> + ?Sized,
        __Interior: ExclusiveKind,
    > SoftEncodeRef<'a> for ManuallyDrop<R>
    where
        Self: ReprC<CType: CType + Sized>
            + RustSpec<Size = NulTerminated, Mutability = Exclusive<__Interior>>
            + 'a,
        &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
        &'a R: EncodeMove + ReprC<CType = SoftRef<<R as ReprC>::CType>>,
    {
        type Store = <&'a R as EncodeMove>::Store;

        fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = EncodeMove::soft_encode(&**self, store);
            // SAFETY: ManuallyDrop preserves the inner layout and metadata.
            unsafe { inner.cast() }
        }
    }
    unsafe impl<
        'a,
        R: RustSpec + 'a
            + ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>
            + ?Sized,
        __Interior: ExclusiveKind,
    > SoftEncodeRef<'a> for ManuallyDrop<R>
    where
        Self: ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>
            + RustSpec<Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>
            + 'a,
        &'a Self: ReprC<CType = AbiSoftWideRef<<Self as ReprC>::CType>>,
        &'a R: EncodeMove + ReprC<CType: crate::CType + WideRefCarrier<R, Pointer = *const crate::wide::PointeeData<<R as ReprC>::CType>>>,
    {
        type Store = <&'a R as EncodeMove>::Store;

        fn soft_encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as crate::ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = EncodeMove::soft_encode(&**self, store);
            // SAFETY: ManuallyDrop preserves the inner layout and metadata.
            unsafe { AbiSoftWideRef::from_raw_parts(inner.as_ptr().cast(), inner.metadata()) }
        }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'a, R: ReprC, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a>
    for crate::vec::Vec<R>
where
    &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
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

            store.encode(|stores| unsafe { encode_vec_unchecked(self, self.capacity(), stores) })
        })
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized>, __Interior: ExclusiveKind> SoftEncodeRef<'a>
    for crate::vec::Vec<T>
where
    &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
    Self: crate::convert::SoftEncodeRefUnchecked<'a> + ReprC<CType = AbiVec<<T as ReprC>::CType>>,
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
    &'a T: EncodeMove + ReprC<CType = SoftRef<<T as ReprC>::CType>>,
{
    type Store =
        RefSizedEncodeStore<Self, RefVecEncodeStore<T, Box<[<&'a T as EncodeMove>::Store]>>>;

    fn soft_encode_ref<'itm>(
        &'a self,
        store: &'itm mut Self::Store,
    ) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| {
            let store = fields;

            store.encode(|stores| encode_vec(self, self.capacity(), stores))
        })
    }
}

unsafe impl<'a, T: ReprC<CType: Copy> + 'a, const N: usize> SoftEncodeRefUnchecked<'a> for [T; N]
where
    &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'a T: EncodeMoveUnchecked + ReprC<CType = SoftRef<<T as ReprC>::CType>>,
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
                encode_ref_field_unchecked(&self[i], &mut fields.0[i])
            })
        })
    }
}

unsafe impl<'a, T: ReprC<CType: Copy> + 'a, const N: usize> SoftEncodeRef<'a> for [T; N]
where
    &'a Self: ReprC<CType = SoftRef<<Self as ReprC>::CType>>,
    &'a T: EncodeMove + ReprC<CType = SoftRef<<T as ReprC>::CType>>,
{
    type Store = RefSizedEncodeStore<Self, ArrayStore<<&'a T as EncodeMove>::Store, N>>;
    fn soft_encode_ref<'itm>(
        &'a self,
        store: &'itm mut Self::Store,
    ) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        store
            .encode(|fields| core::array::from_fn(|i| encode_ref_field(&self[i], &mut fields.0[i])))
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized> + 'a> SoftEncodeRefUnchecked<'a> for [T]
where
    &'a T: EncodeMoveUnchecked + ReprC<CType: crate::reference::RefCarrier<T>>,
    &'a Self: ReprC<CType = AbiSoftWideRef<<Self as ReprC>::CType>>,
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
                encode_ref_field_unchecked(item, store)
            })
        })
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized> + 'a> SoftEncodeRef<'a> for [T]
where
    &'a T: EncodeMove + ReprC<CType: crate::reference::RefCarrier<T>>,
    &'a Self: ReprC<CType = AbiSoftWideRef<<Self as ReprC>::CType>>,
{
    type Store = RefWideEncodeStore<Self, Box<[<&'a T as EncodeMove>::Store]>>;

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

unsafe impl<'a> SoftEncodeRef<'a> for core::ffi::CStr {
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
unsafe impl<'a, 'b: 'a, R: ?Sized, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a> for &'b R
where
    &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
    Self: EncodeMoveUnchecked
        + ReprC<CType: CType + Sized>
        + RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
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

unsafe impl<'a, 'b: 'a, R: ?Sized, __Interior: ExclusiveKind> SoftEncodeRef<'a> for &'b R
where
    &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
    Self: EncodeMove
        + ReprC<CType: CType + Sized>
        + RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
{
    type Store = RefSizedEncodeStore<Self, <Self as EncodeMove>::Store>;

    fn soft_encode_ref<'itm>(
        &'a self,
        store: &'itm mut Self::Store,
    ) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| crate::convert::EncodeMove::soft_encode(*self, fields))
    }
}

// A shared borrow of an exclusive reference only permits shared pointee access.
// The mutable carrier is retained solely to match the nested reference ABI.
unsafe impl<'a, 'b: 'a, R: RustSpec + ?Sized, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a>
    for &'b mut R
where
    &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
    Self: ReprC<CType: Copy + Sized>
        + RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
    &'a R: EncodeMoveUnchecked + ReprC<CType: Copy + Sized>,
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

unsafe impl<'a, 'b: 'a, R: RustSpec + ?Sized, __Interior: ExclusiveKind> SoftEncodeRef<'a>
    for &'b mut R
where
    &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
    Self: ReprC<CType: Copy + Sized>
        + RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
    &'a R: EncodeMove + ReprC<CType: Copy + Sized>,
{
    type Store = RefSizedEncodeStore<Self, <&'a R as EncodeMove>::Store>;

    fn soft_encode_ref<'itm>(
        &'a self,
        store: &'itm mut Self::Store,
    ) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| {
            let value = crate::convert::EncodeMove::soft_encode(&**self, fields);
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

#[cfg(feature = "alloc")]
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
        unsafe impl<'a> SoftEncodeRef<'a> for $ty {
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
impl_buffer_shared!(alloc::ffi::CString, |value: &alloc::ffi::CString| unsafe {
    AbiBox::from_raw(value.as_ptr().cast_mut().cast())
});

#[cfg(feature = "alloc")]
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

#[cfg(feature = "alloc")]
fn encode_vec<'a, T: ReprC<CType: CType + Copy> + 'a>(
    source: &'a [T],
    capacity: usize,
    stores: &mut Box<[<&'a T as EncodeMove>::Store]>,
) -> Vec<T::CType>
where
    &'a T: EncodeMove<CType: crate::reference::RefCarrier<T>, Store: Store + Default>,
{
    crate::stored::map_vec(source.iter(), capacity, stores, encode_ref_field)
}

#[cfg(feature = "alloc")]
unsafe fn encode_vec_unchecked<'a, T: ReprC<CType: CType + Copy> + 'a>(
    source: &'a [T],
    capacity: usize,
    stores: &mut Box<[<&'a T as EncodeMoveUnchecked>::SoftStore]>,
) -> Vec<T::CType>
where
    &'a T: EncodeMoveUnchecked<CType: crate::reference::RefCarrier<T>, SoftStore: Store + Default>,
{
    crate::stored::map_vec(source.iter(), capacity, stores, |item, store| unsafe {
        encode_ref_field_unchecked(item, store)
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
                            &'static T: co3::ReprC<CType: crate::CType + co3::CType + Sized>,
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
                    &'static T: co3::ReprC<CType: crate::CType + co3::CType + Sized>,
                {}
                check::<$ty>();
            };
        };
    }

    // No Clone implementation: the stable vector path must not require one.
    #[derive(RustSpec, co3::ReprC)]
    #[repr(transparent)]
    #[repr_c(identity)]
    struct NoClone(u32);

    #[test]
    fn stable_vec_borrows_non_clone_elements() {
        let values = alloc::vec![NoClone(7), NoClone(11)];
        let mut store = Default::default();
        let encoded = co3::soft_encode(&values, &mut store);
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), values.as_ptr());
        assert_eq!(header.len(), 2);
        assert_eq!(unsafe { (*header.as_ptr()).0 }, 7);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(values[1].0, 11);

        let mut store = Default::default();
        let encoded = unsafe { co3::soft_encode_unchecked(&values, &mut store) };
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
        let encoded = co3::soft_encode(nested, &mut store);
        let header = unsafe { &*(*encoded.as_ptr()).as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), values.as_ptr());
        assert_eq!(header.len(), 2);
        assert_eq!(unsafe { (*header.as_ptr()).0 }, 7);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = unsafe { co3::soft_encode_unchecked(&nested, &mut store) };
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
            let encoded = co3::soft_encode(&values, &mut store);
            let header = unsafe { &*encoded.as_ptr() };
            assert_eq!(header.as_ptr().cast::<bool>().cast_const(), values.as_ptr());
            assert_eq!(header.len(), values.len());
            assert_eq!(store.sync(), Some(()));
        }
        let values = alloc::vec![(); 3];
        let mut store = Default::default();
        let encoded = co3::soft_encode(&values, &mut store);
        assert_eq!(unsafe { (*encoded.as_ptr()).len() }, 3);
        assert_eq!(store.sync(), Some(()));
    }

    #[derive(Clone, RustSpec, co3::ReprC)]
    struct Record<T>(T);

    // The layout of this downstream type is projected through its foreign field.
    #[derive(Clone, RustSpec, co3::ReprC)]
    #[repr(transparent)]
    struct Projected((u32,));

    #[test]
    fn derived_shared_refs_support_field_dependent_layouts() {
        let value = Record((13_u32,));
        let mut store = Default::default();
        let encoded = co3::soft_encode(&value, &mut store);
        assert_eq!(unsafe { (*encoded.as_ptr()).0.0.0 }, 13);
        assert_eq!(store.sync(), Some(()));

        let value = Projected((17,));
        let mut store = Default::default();
        let encoded = unsafe { co3::soft_encode_unchecked(&value, &mut store) };
        assert_eq!(unsafe { (*encoded.as_ptr()).0.0.0 }, 17);
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn unstable_vec_converts_elements() {
        let values = alloc::vec![(19_u32,), (23,)];
        let mut store = Default::default();
        let encoded = co3::soft_encode(&values, &mut store);
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
        let encoded = co3::soft_encode(&values, &mut store);
        let header = unsafe { *encoded.as_ptr() };
        assert_eq!(
            header.as_ptr().cast::<Cell<u32>>().cast_const(),
            values.as_ptr()
        );
        unsafe { header.as_ptr().cast::<u32>().write(31) };
        assert_eq!(values[0].get(), 31);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = unsafe { co3::soft_encode_unchecked(&values, &mut store) };
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

    impl co3::ReprC for RefOnly {
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
    unsafe impl<'a> co3::convert::SoftEncodeRefUnchecked<'a> for RefOnly {
        type SoftStore = co3::sync::RefSizedEncodeStore<Self, ()>;
        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as co3::ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|_fields| self.0)
        }
    }
    unsafe impl<'a> co3::convert::SoftEncodeRef<'a> for RefOnly {
        type Store = co3::sync::RefSizedEncodeStore<Self, CheckedStore>;
        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as co3::ReprC>::CType
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
        let encoded = co3::soft_encode(&value, &mut store);
        assert_eq!(unsafe { *encoded.as_ptr() }, 37);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = unsafe { co3::soft_encode_unchecked(&value, &mut store) };
        assert_eq!(unsafe { *encoded.as_ptr() }, 37);
        assert_eq!(store.sync(), Some(()));
    }

    #[derive(RustSpec, co3::ReprC)]
    struct Nested {
        values: Vec<NoClone>,
        text: String,
        boxed: Box<(u32,)>,
    }

    #[derive(RustSpec, co3::ReprC)]
    struct Transparent<T>(T);

    #[derive(RustSpec, co3::ReprC)]
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
        let encoded = co3::soft_encode(&value, &mut store);
        let fields = unsafe { &*encoded.as_ptr() };
        assert_eq!(fields.values.as_ptr().cast_const(), value.values.as_ptr());
        assert_eq!(fields.text.as_ptr().cast_const(), value.text.as_ptr());
        assert_eq!(fields.text.capacity(), value.text.capacity());
        assert_eq!(unsafe { (*fields.boxed.as_ptr()).0.0 }, 43);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(value.values[0].0, 41);

        let wrapped = Transparent(value.values);
        let mut store = Default::default();
        let header = co3::soft_encode(&wrapped, &mut store);
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(header.0.as_ptr().cast_const(), wrapped.0.as_ptr());
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn tuples_arrays_options_results_and_manually_drop_delegate() {
        let value = (alloc::vec![NoClone(47)],);
        let mut store = Default::default();
        let encoded = co3::soft_encode(&value, &mut store);
        assert_eq!(
            unsafe { (*encoded.as_ptr()).0.0.as_ptr().cast_const() },
            value.0.as_ptr()
        );
        assert_eq!(store.sync(), Some(()));

        let array = [value.0];
        let mut store = Default::default();
        let encoded = unsafe { co3::soft_encode_unchecked(&array, &mut store) };
        let encoded = unsafe { &*encoded.as_ptr() };
        assert_eq!(encoded[0].as_ptr().cast_const(), array[0].as_ptr());
        assert_eq!(store.sync(), Some(()));

        let elements = alloc::vec![NoClone(53)];
        let pointer = elements.as_ptr();
        let value = Some(elements);
        let mut store = Default::default();
        let header = co3::soft_encode(&value, &mut store);
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(header.0.0.as_ptr().cast_const(), pointer);
        assert_eq!(store.sync(), Some(()));

        let Some(value) = value else { unreachable!() };
        let value = Ok::<_, ()>(value);
        let mut store = Default::default();
        let header = co3::soft_encode(&value, &mut store);
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(header.0.0.as_ptr().cast_const(), pointer);
        assert_eq!(store.sync(), Some(()));

        let Ok(value) = value else { unreachable!() };
        let value = core::mem::ManuallyDrop::new(value);
        let mut store = Default::default();
        let header = co3::soft_encode(&value, &mut store);
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), value.as_ptr());
        assert_eq!(store.sync(), Some(()));
        drop(core::mem::ManuallyDrop::into_inner(value));
    }

    #[test]
    fn unstable_vec_delegates_to_non_clone_shared_only_elements() {
        let values = alloc::vec![(RefOnly(59),), (RefOnly(61),)];
        let mut store = Default::default();
        let encoded = co3::soft_encode(&values, &mut store);
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(unsafe { (*header.as_ptr()).0.0 }, 59);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = unsafe { co3::soft_encode_unchecked(&values, &mut store) };
        assert_eq!(unsafe { (*(*encoded.as_ptr()).as_ptr()).0.0 }, 59);
        assert_eq!(store.sync(), Some(()));
        let boxed = values.into_boxed_slice();
        let mut store = Default::default();
        let encoded = co3::soft_encode(&boxed, &mut store);
        assert_eq!(unsafe { (*(*encoded.as_ptr()).as_ptr()).0.0 }, 59);
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn derived_enum_syncs_only_its_active_shared_store() {
        let mut store = Default::default();
        let _encoded = co3::soft_encode(&Choice::Value(RefOnly(67)), &mut store);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let _encoded = co3::soft_encode(&Choice::Value(RefOnly(71)), &mut store);
        let _encoded = co3::soft_encode(&Choice::Empty, &mut store);
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn buffers_and_boxes_borrow_original_allocations() {
        let value = Box::new(NoClone(73));
        let header = co3::soft_encode(&value, &mut ());
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), &*value as *const NoClone);
        assert_eq!(().sync(), Some(()));

        let value = alloc::vec![NoClone(79)].into_boxed_slice();
        let mut store = Default::default();
        let header = co3::soft_encode(&value, &mut store);
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), value.as_ptr());
        assert_eq!(store.sync(), Some(()));

        let value = alloc::ffi::CString::new("borrowed").unwrap();
        let mut store = Default::default();
        let header = co3::soft_encode(&value, &mut store);
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
        let encoded = co3::soft_encode(&co3::option::Option::Some(RefOnly(83)), &mut store);
        let encoded = unsafe { &*encoded.as_ptr() };
        assert_eq!(encoded.into_option(), Some(Some(83)));
        let encoded = co3::soft_encode(&co3::option::Option::<RefOnly>::None, &mut store);
        let encoded = unsafe { &*encoded.as_ptr() };
        assert_eq!(encoded.into_option(), Some(None));
        let value = *encoded;
        store.encode(|fields| {
            assert!(fields.is_none());
            value
        });
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = co3::soft_encode(&Ok::<_, u32>(RefOnly(89)), &mut store);
        let encoded = unsafe { &*encoded.as_ptr() };
        assert_eq!(encoded.into_result(), Some(Ok(89)));
        let encoded = co3::soft_encode(&Err::<RefOnly, _>(97_u32), &mut store);
        let encoded = unsafe { &*encoded.as_ptr() };
        assert_eq!(encoded.into_result(), Some(Err(97)));
        let value = *encoded;
        store.encode(|fields| {
            assert!(matches!(fields, Some(Err(()))));
            value
        });
        assert_eq!(store.sync(), Some(()));

        let _ = co3::soft_encode(&Ok::<_, ()>(core::num::NonZeroU8::new(1).unwrap()), &mut ());
        let encoded = co3::soft_encode(&Err::<core::num::NonZeroU8, _>(()), &mut ());
        assert_eq!(unsafe { *encoded.as_ptr() }.0.0.0, 0);
        assert_eq!(().sync(), Some(()));
    }

    #[test]
    fn boxed_borrowed_fields_do_not_require_static_lifetimes() {
        let value = 101_u32;
        let boxed = Box::new((&value,));
        let mut store = Default::default();
        let header = co3::soft_encode(&boxed, &mut store);
        let header = unsafe { &*header.as_ptr() };
        assert_eq!(
            unsafe { (*header.as_ptr()).0.0.as_ptr() },
            &value as *const _
        );
        assert_eq!(store.sync(), Some(()));
    }

    #[derive(RustSpec, co3::ReprC)]
    #[rust_spec(non_robust)]
    #[repr_c(is_valid = |value| *value != 0)]
    struct Validated(u32);

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "encoding failed custom is_valid")]
    fn derived_shared_encoding_preserves_custom_validation() {
        co3::soft_encode(&Validated(0), &mut Default::default());
    }

    #[test]
    fn cells_cannot_be_snapshotted_by_shared_encoders() {
        use core::cell::{Cell, UnsafeCell};

        use co3::convert::{SoftEncodeRef, SoftEncodeRefUnchecked};
        use static_assertions::{assert_impl_all, assert_not_impl_any};

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
        let encoded = co3::soft_encode(&value, &mut store);
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(header.as_ptr().cast::<Cell<u32>>().cast_const(), &*value);
        assert_eq!(store.sync(), Some(()));

        let values = alloc::vec![Cell::new(11_u32)].into_boxed_slice();
        let mut store = Default::default();
        let encoded = co3::soft_encode(&values, &mut store);
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(
            header.as_ptr().cast::<Cell<u32>>().cast_const(),
            values.as_ptr()
        );
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn aggregates_encode_stable_fields_without_encode_ref_impls() {
        use co3::convert::{SoftEncodeRef, SoftEncodeRefUnchecked};
        assert_not_shared_encoder!(u32: SoftEncodeRefUnchecked<'static>, SoftEncodeRef<'static>);

        assert_not_shared_encoder!(Record<NoClone>: SoftEncodeRefUnchecked<'static>, SoftEncodeRef<'static>);
        assert_not_shared_encoder!((NoClone,): SoftEncodeRefUnchecked<'static>, SoftEncodeRef<'static>);

        let value = Record(109_u32);
        let mut store = Default::default();
        let encoded: <&Record<u32> as co3::ReprC>::CType = co3::soft_encode(&value, &mut store);
        assert_eq!(unsafe { (*encoded.as_ptr()).0 }, 109);
        assert_eq!(store.sync(), Some(()));

        let tuple = (113_u32,);
        let mut store = Default::default();
        let encoded = unsafe { co3::soft_encode_unchecked(&tuple, &mut store) };
        assert_eq!(unsafe { (*encoded.as_ptr()).0.0 }, 113);
        assert_eq!(store.sync(), Some(()));
    }

    #[derive(RustSpec, co3::ReprC)]
    #[repr(transparent)]
    #[repr_c(identity)]
    struct InteriorIdentity(core::cell::UnsafeCell<u32>);

    #[test]
    fn stable_identity_reference_preserves_interior_storage() {
        let value = InteriorIdentity(core::cell::UnsafeCell::new(127));
        let mut store = ();
        let encoded = co3::soft_encode(&value, &mut store);
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
        let encoded = co3::soft_encode(&reference, &mut store);
        let header = unsafe { &*(*encoded.as_ptr()).as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), pointer);
        assert_eq!(header.len(), 2);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(reference[1].0, 11);

        let mut store = Default::default();
        let encoded = unsafe { co3::soft_encode_unchecked(&reference, &mut store) };
        let header = unsafe { &*(*encoded.as_ptr()).as_ptr() };
        assert_eq!(header.as_ptr().cast_const(), pointer);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(reference[0].0, 7);
    }

    #[test]
    fn shared_borrow_of_mutable_slice_reference_preserves_metadata() {
        assert_not_shared_encoder!(&mut Vec<core::cell::Cell<bool>>:
            co3::convert::SoftEncodeRef<'static>);
        let mut values = [NoClone(3), NoClone(5)];
        let pointer = values.as_ptr();
        let reference = &mut values[..];
        let mut store = Default::default();
        let encoded = co3::soft_encode(&reference, &mut store);
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(header.as_ptr(), pointer);
        assert_eq!(header.metadata(), 2);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(reference[1].0, 5);
    }

    // Neither the element nor its owned slice supports cloning.
    #[derive(RustSpec, co3::ReprC, Debug, PartialEq)]
    struct Element(Vec<u32>);

    #[test]
    fn shared_unstable_slice_encodes_without_clone() {
        static_assertions::assert_not_impl_any!([Element]: SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
        let values = [Element(alloc::vec![3, 5]), Element(alloc::vec![7])];
        let original = values[0].0.as_ptr();
        let mut store = Default::default();
        let encoded = co3::soft_encode(values.as_slice(), &mut store);
        assert_eq!(encoded.metadata(), 2);
        let field = unsafe { &(*encoded.as_ptr()).0 };
        assert_eq!(field.as_ptr().cast_const(), original);
        assert_eq!(unsafe { *field.as_ptr().add(1) }, 5);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = unsafe { co3::soft_encode_unchecked(values.as_slice(), &mut store) };
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
        let encoded = co3::soft_encode(values.as_slice(), &mut store);
        assert_eq!(encoded.metadata(), 0);
        assert!(!encoded.as_ptr().is_null());
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn nul_terminated_reference_uses_explicit_encoder_without_storage() {
        let value = c"borrowed";
        let encoded: <&core::ffi::CStr as ReprC>::CType =
            co3::soft_encode(value, &mut Default::default());
        assert_eq!(encoded.as_ptr().cast::<core::ffi::c_char>(), value.as_ptr());
        let encoded = unsafe { co3::soft_encode_unchecked(value, &mut Default::default()) };
        assert_eq!(encoded.as_ptr().cast::<core::ffi::c_char>(), value.as_ptr());
        let wrapped: &core::mem::ManuallyDrop<core::ffi::CStr> = unsafe {
            &*(value as *const core::ffi::CStr as *const core::mem::ManuallyDrop<core::ffi::CStr>)
        };
        assert_eq!(
            co3::soft_encode(wrapped, &mut Default::default())
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
            let inner = unsafe { co3::soft_encode_unchecked(&self.0, store) };
            unsafe {
                co3::wide::abi::SoftWideRef::<Self::CType>::from_raw_parts(
                    inner.as_ptr(),
                    inner.metadata(),
                )
            }
        }
    }
    unsafe impl<'a> SoftEncodeRef<'a> for SliceView {
        type Store = <[Element] as SoftEncodeRef<'a>>::Store;
        fn soft_encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as co3::ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = co3::soft_encode(&self.0, store);
            unsafe {
                co3::wide::abi::SoftWideRef::<Self::CType>::from_raw_parts(
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
        let encoded = co3::soft_encode(&*value, &mut store);
        assert_eq!(encoded.metadata(), 1);
        assert_eq!(store.sync(), Some(()));

        let wrapped: &core::mem::ManuallyDrop<SliceView> =
            unsafe { &*(value as *const SliceView as *const core::mem::ManuallyDrop<SliceView>) };
        let mut store = Default::default();
        let encoded = co3::soft_encode(wrapped, &mut store);
        assert_eq!(encoded.metadata(), 1);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = unsafe { co3::soft_encode_unchecked(wrapped, &mut store) };
        assert_eq!(encoded.metadata(), 1);
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn boxed_custom_wide_type_delegates_to_pointee_encoder() {
        let values: Box<[Element]> = Box::new([Element(alloc::vec![23])]);
        let boxed: Box<SliceView> =
            unsafe { Box::from_raw(Box::into_raw(values) as *mut SliceView) };
        let mut store = Default::default();
        let encoded = co3::soft_encode(&boxed, &mut store);
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(header.metadata(), 1);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let encoded = unsafe { co3::soft_encode_unchecked(&boxed, &mut store) };
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(header.metadata(), 1);
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn stable_slice_paths_remain_store_free() {
        let mut values = [31_u32, 37];
        let encoded = co3::encode(values.as_slice());
        assert_eq!(encoded.as_ptr(), values.as_ptr());
        let original = values.as_mut_ptr();
        let encoded = co3::encode(values.as_mut_slice());
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
        static_assertions::assert_impl_all!(WrappedCStr: co3::ffi::NulTerminatedBuf);
        assert_shared_encoder!(WrappedCStr: co3::convert::SoftEncodeRef<'static>, co3::convert::SoftEncodeRefUnchecked<'static>);
        static_assertions::assert_not_impl_any!(WrappedCStr:
            co3::convert::SoftEncodeMut<'static>, co3::convert::SoftEncodeMutUnchecked<'static>);

        let value = c"hello";
        let wrapper = unsafe { &*(value as *const CStr as *const WrappedCStr) };
        let mut store = ();
        let encoded = co3::soft_encode(wrapper, &mut store);
        assert_eq!(unsafe { CStr::from_ptr(encoded.as_ptr().cast()) }, value);
        assert_eq!(store.sync(), Some(()));
        let mut store = ();
        let encoded = unsafe { co3::soft_encode_unchecked(wrapper, &mut store) };
        assert_eq!(unsafe { CStr::from_ptr(encoded.as_ptr().cast()) }, value);
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn nontransparent_nul_tail_has_no_reference_encoder() {
        assert_not_shared_encoder!(CStrTail: co3::convert::SoftEncodeRef<'static>, co3::convert::SoftEncodeRefUnchecked<'static>);
        static_assertions::assert_not_impl_any!(CStrTail: co3::convert::SoftEncodeMut<'static>, co3::convert::SoftEncodeMutUnchecked<'static>);
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
        use co3::convert::{SoftEncodeMut, SoftEncodeMutUnchecked, SoftEncodeRefUnchecked};
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
        let encoded = co3::soft_encode(&*wrapper, &mut store);
        assert_eq!(&unsafe { encoded.into_ref().unwrap() }.0, &[1, 2]);
        assert_eq!(store.sync(), Some(()));
        let mut store = ();
        let encoded = co3::soft_encode(&mut *wrapper, &mut store);
        unsafe { encoded.as_ptr().cast_mut().cast::<u32>().write(3) };
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
        use co3::convert::{SoftEncodeMut, SoftEncodeMutUnchecked, SoftEncodeRefUnchecked};
        assert_not_shared_encoder!(Packet: SoftEncodeRef<'static>, SoftEncodeRefUnchecked<'static>);
        static_assertions::assert_not_impl_any!(Packet: SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
        assert_shared_encoder!(GenericPacket<u8>: SoftEncodeRef<'static>, SoftEncodeRefUnchecked<'static>);
        assert_not_shared_encoder!(GenericPacket<[u8]>: SoftEncodeRef<'static>, SoftEncodeRefUnchecked<'static>);
        static_assertions::assert_not_impl_any!(GenericPacket<[u8]>: SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
        assert_not_shared_encoder!(GenericPacket<str>: SoftEncodeRef<'static>, SoftEncodeRefUnchecked<'static>);
        static_assertions::assert_not_impl_any!(GenericPacket<str>: SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
    }
}
