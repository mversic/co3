//! Referent-specific encoding for the unstable shared-reference path.
#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
use core::mem::ManuallyDrop;

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
    niche::Niche,
    option::COption,
    reference::CRef,
    result::CResult,
    slice::CWideRef,
    stored::{EncodeMove, EncodeMoveUnchecked, Store},
    sync::RefSizedEncodeStore,
    transmute::AbiCompatible,
    wide::{PointeeData, Wide, WideHeader},
};
#[cfg(feature = "alloc")]
use crate::{
    boxed::{CBox, CWideBox},
    reference::CRefMut,
    slice::CWideRefMut,
    sync::{RefVecEncodeStore, RefWideEncodeStore},
    vec::CVec,
};

disjoint_impls! {
    /// Encodes the contents of a shared reference without transferring ownership.
    ///
    /// # Safety
    ///
    /// Check [`craEncodeUnchecked`]
    pub unsafe trait EncodeRefUnchecked<'a>: ReprC + 'a
    where
        &'a Self: ReprC<CType: Sized>,
    {
        type SoftStore: Store + Default;
        /// Returns a borrowed reference carrier. Neither the referent nor any
        /// backing allocation is transferred to the caller.
        ///
        /// # Safety
        ///
        /// See [`crate::soft_encode_unchecked`].
        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm;
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: RustSpec + 'a, C: CType, S: SizedKind, __Interior: ExclusiveKind>
        EncodeRefUnchecked<'a> for Box<R>
    where
        R: RustSpec<Size = RustSpecSized<S>, Mutability = Exclusive<__Interior>>,
        &'a R: EncodeMoveUnchecked + ReprC<CType = CRef<C>>,
        Self: ReprC<CType = CBox<C>>,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, <&'a R as EncodeMoveUnchecked>::SoftStore>;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = unsafe { (&**self).soft_encode_unchecked(fields) };
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { CBox::from_raw(view.as_ptr().cast_mut()) }
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: RustSpec + 'a, C: CType, S: SizedKind, __Trap> EncodeRefUnchecked<'a> for Box<R>
    where
        R: RustSpec<Size = RustSpecSized<S>, Mutability = WithInterior<__Trap>>,
        &'a R: EncodeMoveUnchecked + ReprC<CType = CRefMut<C, false>>,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
        Self: ReprC<CType = CBox<C>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, <&'a R as EncodeMoveUnchecked>::SoftStore>;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = unsafe { (&**self).soft_encode_unchecked(fields) };
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { CBox::from_raw(view.as_ptr()) }
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<
        'a,
        R: RustSpec + 'a + ?Sized,
        P: Wide<Header: WideHeader<Data: CType>, Metadata: CType + Sized> + ?Sized,
        __Interior: ExclusiveKind,
    > EncodeRefUnchecked<'a> for Box<R>
    where
        R: RustSpec<Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>,
        &'a R: EncodeMoveUnchecked + ReprC<CType = CWideRef<P>>,
        Self: ReprC<CType = CWideBox<P>>,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, <&'a R as EncodeMoveUnchecked>::SoftStore>;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = unsafe { (&**self).soft_encode_unchecked(fields) };
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { CWideBox::from_raw_nullable(view.as_ptr().cast_mut(), view.metadata()) }
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<
        'a,
        R: RustSpec + 'a + ?Sized,
        P: Wide<Header: WideHeader<Data: CType>, Metadata: CType + Sized> + ?Sized,
        __Trap,
    > EncodeRefUnchecked<'a> for Box<R>
    where
        R: RustSpec<Size = MetaSized<SliceLike>, Mutability = WithInterior<__Trap>>,
        &'a R: EncodeMoveUnchecked + ReprC<CType = CWideRefMut<P, false>>,
        Self: ReprC<CType = CWideBox<P>>,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, <&'a R as EncodeMoveUnchecked>::SoftStore>;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = unsafe { (&**self).soft_encode_unchecked(fields) };
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { CWideBox::from_raw_nullable(view.as_ptr(), view.metadata()) }
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: RustSpec + 'a + ?Sized, C: CType, __Interior: ExclusiveKind>
        EncodeRefUnchecked<'a> for Box<R>
    where
        R: RustSpec<Size = NulTerminated, Mutability = Exclusive<__Interior>>,
        &'a R: EncodeMoveUnchecked + ReprC<CType = CRef<C>>,
        Self: ReprC<CType = CBox<C>>,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, <&'a R as EncodeMoveUnchecked>::SoftStore>;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = unsafe { (&**self).soft_encode_unchecked(fields) };
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { CBox::from_raw(view.as_ptr().cast_mut()) }
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: RustSpec + 'a + ?Sized, C: CType, __Trap> EncodeRefUnchecked<'a> for Box<R>
    where
        R: RustSpec<Size = NulTerminated, Mutability = WithInterior<__Trap>>,
        &'a R: EncodeMoveUnchecked + ReprC<CType = CRefMut<C, false>>,
        Self: ReprC<CType = CBox<C>>,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, <&'a R as EncodeMoveUnchecked>::SoftStore>;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = unsafe { (&**self).soft_encode_unchecked(fields) };
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { CBox::from_raw(view.as_ptr()) }
            })
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: AbiCompatible<CType: CType + Sized>, __Interior: ExclusiveKind>
        EncodeRefUnchecked<'a> for crate::vec::Vec<T>
    where
        T: RustSpec<Layout = Stable>,
        Self: ReprC<CType = CVec<<T as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
    {
        type SoftStore = RefSizedEncodeStore<Self, ()>;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|_fields| unsafe {
                CVec::from_raw_parts(self.as_ptr().cast_mut().cast(), self.len(), self.capacity())
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized>, __Interior: ExclusiveKind>
        EncodeRefUnchecked<'a> for crate::vec::Vec<T>
    where
        T: RustSpec<Layout = Unstable>,
        Self: ReprC<CType = CVec<<T as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a T: EncodeMoveUnchecked + ReprC<CType = CRef<<T as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<
            Self,
            RefVecEncodeStore<<T as ReprC>::CType, Box<[<&'a T as EncodeMoveUnchecked>::SoftStore]>>,
        >;

        unsafe fn encode_ref_unchecked<'itm>(
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
    unsafe impl<'a, T: AbiCompatible<CType: CType + Sized>, __Interior: ExclusiveKind>
        EncodeRefUnchecked<'a> for Vec<T>
    where
        T: RustSpec<Layout = Stable>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
        Self: ReprC<CType = CVec<<T as ReprC>::CType>> + 'a,
    {
        type SoftStore = RefSizedEncodeStore<Self, ()>;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|_fields| unsafe {
                CVec::from_raw_parts(self.as_ptr().cast_mut().cast(), self.len(), self.capacity())
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized>, __Interior: ExclusiveKind>
        EncodeRefUnchecked<'a> for Vec<T>
    where
        T: RustSpec<Layout = Unstable>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
        &'a T: EncodeMoveUnchecked + ReprC<CType = CRef<<T as ReprC>::CType>>,
        Self: ReprC<CType = CVec<<T as ReprC>::CType>> + 'a,
    {
        type SoftStore = RefSizedEncodeStore<
            Self,
            RefVecEncodeStore<<T as ReprC>::CType, Box<[<&'a T as EncodeMoveUnchecked>::SoftStore]>>,
        >;

        unsafe fn encode_ref_unchecked<'itm>(
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

    unsafe impl<'a, R: ReprC<CType: CType + Copy>, __Interior: ExclusiveKind> EncodeRefUnchecked<'a>
        for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMoveUnchecked + ReprC<CType = CRef<<R as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMoveUnchecked>::SoftStore>>;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let store = fields;

                match self {
                    Some(value) => COption::Some(unsafe {
                        crate::stored::encode_ref_field_unchecked(
                            value,
                            store.insert(Default::default()),
                        )
                    }),
                    None => {
                        *store = None;
                        COption::None()
                    }
                }
            })
        }
    }
    unsafe impl<'a, R: Niche, N: LayoutKind, __Interior: ExclusiveKind> EncodeRefUnchecked<'a>
        for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMoveUnchecked + ReprC<CType = CRef<<R as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMoveUnchecked>::SoftStore>>;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let store = fields;

                match self {
                    Some(value) => unsafe {
                        crate::stored::encode_ref_field_unchecked(
                            value,
                            store.insert(Default::default()),
                        )
                    },
                    None => {
                        *store = None;
                        R::NICHE
                    }
                }
            })
        }
    }

    unsafe impl<
        'a,
        R: ReprC<CType: CType + Copy>,
        E: ReprC<CType: CType + Copy>,
        K: SizedKind,
        __Interior: ExclusiveKind,
    > EncodeRefUnchecked<'a> for Result<R, E>
    where
        Self: ReprC<CType = CResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<K>>,
        E: RustSpec<Size = RustSpecSized<K>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMoveUnchecked + ReprC<CType = CRef<<R as ReprC>::CType>>,
        &'a E: EncodeMoveUnchecked + ReprC<CType = CRef<<E as ReprC>::CType>>,
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

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let store = fields;

                match self {
                    Ok(ok) => {
                        let Result::Ok(inner) = store.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        CResult::Ok(unsafe { crate::stored::encode_ref_field_unchecked(ok, inner) })
                    }
                    Err(err) => {
                        let Result::Err(inner) = store.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        CResult::Err(unsafe { crate::stored::encode_ref_field_unchecked(err, inner) })
                    }
                }
            })
        }
    }

    unsafe impl<'a, R: Niche, E: ReprC, N: LayoutKind, __Interior: ExclusiveKind> EncodeRefUnchecked<'a>
        for Result<R, E>
    where
        Self: ReprC<CType = <R as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMoveUnchecked + ReprC<CType = CRef<<R as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMoveUnchecked>::SoftStore>>;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let store = fields;

                match self {
                    Ok(ok) => unsafe {
                        crate::stored::encode_ref_field_unchecked(ok, store.insert(Default::default()))
                    },
                    Err(_) => {
                        *store = None;
                        R::NICHE
                    }
                }
            })
        }
    }

    unsafe impl<'a, R: ReprC, E: Niche, N: LayoutKind, __Interior: ExclusiveKind> EncodeRefUnchecked<'a>
        for Result<R, E>
    where
        Self: ReprC<CType = <E as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a E: EncodeMoveUnchecked + ReprC<CType = CRef<<E as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<Self, Option<<&'a E as EncodeMoveUnchecked>::SoftStore>>;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let store = fields;

                match self {
                    Ok(_) => {
                        *store = None;
                        E::NICHE
                    }
                    Err(err) => unsafe {
                        crate::stored::encode_ref_field_unchecked(err, store.insert(Default::default()))
                    },
                }
            })
        }
    }

    unsafe impl<'a, R: ReprC<CType: Sized> + ?Sized, __Interior: ExclusiveKind>
        EncodeRefUnchecked<'a> for ManuallyDrop<R>
    where
        Self: ReprC<CType: CType + Sized>
            + RustSpec<Size: crate::Thin, Mutability = Exclusive<__Interior>>
            + 'a,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
        &'a R: ReprC<CType = CRef<<R as ReprC>::CType>>,
        &'a R: EncodeMoveUnchecked,
    {
        type SoftStore = <&'a R as EncodeMoveUnchecked>::SoftStore;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = unsafe { (&**self).soft_encode_unchecked(store) };
            // ManuallyDrop's C representation is the inner CType or its transparent wrapper.
            unsafe { CRef::from_raw(inner.as_ptr().cast()) }
        }
    }
    unsafe impl<'a, R: ReprC<CType: Sized> + ?Sized, __Interior: ExclusiveKind>
        EncodeRefUnchecked<'a> for ManuallyDrop<R>
    where
        Self: ReprC<CType: CType + Sized>
            + RustSpec<Size = NulTerminated, Mutability = Exclusive<__Interior>>
            + 'a,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
        &'a R: ReprC<CType = CRef<<R as ReprC>::CType>>,
        &'a R: EncodeMoveUnchecked,
    {
        type SoftStore = <&'a R as EncodeMoveUnchecked>::SoftStore;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = unsafe { (&**self).soft_encode_unchecked(store) };
            unsafe { CRef::from_raw(inner.as_ptr().cast()) }
        }
    }
    unsafe impl<
        'a,
        R: ReprC<CType: Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>
            + ?Sized,
        __Interior: ExclusiveKind,
    > EncodeRefUnchecked<'a> for ManuallyDrop<R>
    where
        Self: ReprC<CType: Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>
            + RustSpec<Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>
            + 'a,
        &'a Self: ReprC<CType = CWideRef<<Self as ReprC>::CType>>,
        &'a R: ReprC<CType = CWideRef<<R as ReprC>::CType>>,
        &'a R: EncodeMoveUnchecked,
    {
        type SoftStore = <&'a R as EncodeMoveUnchecked>::SoftStore;

        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = unsafe { (&**self).soft_encode_unchecked(store) };
            unsafe {
                CWideRef::from_raw_parts(
                    inner.as_ptr().cast::<PointeeData<Self::CType>>(),
                    inner.metadata(),
                )
            }
        }
    }

    unsafe impl<'a, T: AbiCompatible<CType: CType + Sized> + 'a, const N: usize> EncodeRefUnchecked<'a>
        for [T; N]
    where
        T: RustSpec<Layout = rust_spec::Stable>,
        &'a Self: ReprC<CType = CRef<[<T as ReprC>::CType; N]>>,
        &'a T: EncodeMoveUnchecked + ReprC<CType = CRef<<T as ReprC>::CType>>,
    {
        type SoftStore = ();
        unsafe fn encode_ref_unchecked<'itm>(&'a self, _: &'itm mut ()) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            // SAFETY: Array layout preserves the ABI-compatible element layout.
            // Only the pointer is converted; no element is read or copied.
            unsafe { CRef::from_raw(core::ptr::from_ref(self).cast()) }
        }
    }
    unsafe impl<'a, T: ReprC<CType: Copy> + 'a, const N: usize> EncodeRefUnchecked<'a> for [T; N]
    where
        T: RustSpec<Layout = rust_spec::Unstable>,
        &'a Self: ReprC<CType = CRef<[<T as ReprC>::CType; N]>>,
        &'a T: EncodeMoveUnchecked + ReprC<CType = CRef<<T as ReprC>::CType>>,
    {
        type SoftStore = RefSizedEncodeStore<
            Self,
            crate::sync::ArrayStore<<&'a T as EncodeMoveUnchecked>::SoftStore, N>,
        >;
        unsafe fn encode_ref_unchecked<'itm>(
            &'a self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                core::array::from_fn(|i| unsafe {
                    crate::stored::encode_ref_field_unchecked(&self[i], &mut fields.0[i])
                })
            })
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
    pub unsafe trait EncodeRef<'a>: EncodeRefUnchecked<'a>
    where
        &'a Self: ReprC<CType: Sized>,
    {
        type Store: Store + Default;

        /// Returns access to the encoded referent without transferring ownership.
        /// The source and store must remain alive for any later access. Neither
        /// the referent nor its backing allocations may be freed or owned-decoded.
        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm;
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: RustSpec + 'a, C: CType, S: SizedKind, __Interior: ExclusiveKind> EncodeRef<'a>
        for Box<R>
    where
        R: RustSpec<Size = RustSpecSized<S>, Mutability = Exclusive<__Interior>>,
        &'a R: EncodeMove + ReprC<CType = CRef<C>>,
        Self: ReprC<CType = CBox<C>>,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, <&'a R as EncodeMove>::Store>;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = (&**self).soft_encode(fields);
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { CBox::from_raw(view.as_ptr().cast_mut()) }
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: RustSpec + 'a, C: CType, S: SizedKind, __Trap> EncodeRef<'a> for Box<R>
    where
        R: RustSpec<Size = RustSpecSized<S>, Mutability = WithInterior<__Trap>>,
        &'a R: EncodeMove + ReprC<CType = CRefMut<C, false>>,
        Self: ReprC<CType = CBox<C>>,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, <&'a R as EncodeMove>::Store>;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = (&**self).soft_encode(fields);
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { CBox::from_raw(view.as_ptr()) }
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<
        'a,
        R: RustSpec + 'a + ?Sized,
        P: Wide<Header: WideHeader<Data: CType>, Metadata: CType + Sized> + ?Sized,
        __Interior: ExclusiveKind,
    > EncodeRef<'a> for Box<R>
    where
        R: RustSpec<Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>,
        &'a R: EncodeMove + ReprC<CType = CWideRef<P>>,
        Self: ReprC<CType = CWideBox<P>>,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, <&'a R as EncodeMove>::Store>;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = (&**self).soft_encode(fields);
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { CWideBox::from_raw_nullable(view.as_ptr().cast_mut(), view.metadata()) }
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<
        'a,
        R: RustSpec + 'a + ?Sized,
        P: Wide<Header: WideHeader<Data: CType>, Metadata: CType + Sized> + ?Sized,
        __Trap,
    > EncodeRef<'a> for Box<R>
    where
        R: RustSpec<Size = MetaSized<SliceLike>, Mutability = WithInterior<__Trap>>,
        &'a R: EncodeMove + ReprC<CType = CWideRefMut<P, false>>,
        Self: ReprC<CType = CWideBox<P>>,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, <&'a R as EncodeMove>::Store>;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = (&**self).soft_encode(fields);
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { CWideBox::from_raw_nullable(view.as_ptr(), view.metadata()) }
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: RustSpec + 'a + ?Sized, C: CType, __Interior: ExclusiveKind> EncodeRef<'a>
        for Box<R>
    where
        R: RustSpec<Size = NulTerminated, Mutability = Exclusive<__Interior>>,
        &'a R: EncodeMove + ReprC<CType = CRef<C>>,
        Self: ReprC<CType = CBox<C>>,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, <&'a R as EncodeMove>::Store>;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = (&**self).soft_encode(fields);
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { CBox::from_raw(view.as_ptr().cast_mut()) }
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: RustSpec + 'a + ?Sized, C: CType, __Trap> EncodeRef<'a> for Box<R>
    where
        R: RustSpec<Size = NulTerminated, Mutability = WithInterior<__Trap>>,
        &'a R: EncodeMove + ReprC<CType = CRefMut<C, false>>,
        Self: ReprC<CType = CBox<C>>,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, <&'a R as EncodeMove>::Store>;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let view = (&**self).soft_encode(fields);
                // Only the header is borrowed; the pointee remains owned by self or its store.
                unsafe { CBox::from_raw(view.as_ptr()) }
            })
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: AbiCompatible<CType: CType + Sized>, __Interior: ExclusiveKind>
        EncodeRef<'a> for crate::vec::Vec<T>
    where
        T: RustSpec<Layout = Stable, Mutability = Exclusive<__Interior>>,
        Self: ReprC<CType = CVec<<T as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
    {
        type Store = RefSizedEncodeStore<Self, ()>;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|_fields| unsafe {
                CVec::from_raw_parts(self.as_ptr().cast_mut().cast(), self.len(), self.capacity())
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: AbiCompatible<CType: CType + Sized>, __Interior: ExclusiveKind>
        EncodeRef<'a> for crate::vec::Vec<T>
    where
        T: RustSpec<Layout = Stable, Mutability = WithInterior<Robust>>,
        Self: ReprC<CType = CVec<<T as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
    {
        type Store = RefSizedEncodeStore<Self, ()>;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|_fields| unsafe {
                CVec::from_raw_parts(self.as_ptr().cast_mut().cast(), self.len(), self.capacity())
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized>, __Interior: ExclusiveKind> EncodeRef<'a>
        for crate::vec::Vec<T>
    where
        T: RustSpec<Layout = Unstable>,
        Self: EncodeRefUnchecked<'a> + ReprC<CType = CVec<<T as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a T: EncodeMove + ReprC<CType = CRef<<T as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<
            Self,
            RefVecEncodeStore<<T as ReprC>::CType, Box<[<&'a T as EncodeMove>::Store]>>,
        >;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let store = fields;

                store.encode(|stores| encode_vec(self, self.capacity(), stores))
            })
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: AbiCompatible<CType: CType + Sized>, __Interior: ExclusiveKind>
        EncodeRef<'a> for Vec<T>
    where
        T: RustSpec<Layout = Stable, Mutability = Exclusive<__Interior>>,
        Self: ReprC<CType = CVec<<T as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
    {
        type Store = RefSizedEncodeStore<Self, ()>;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|_fields| unsafe {
                CVec::from_raw_parts(self.as_ptr().cast_mut().cast(), self.len(), self.capacity())
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: AbiCompatible<CType: CType + Sized>, __Interior: ExclusiveKind>
        EncodeRef<'a> for Vec<T>
    where
        T: RustSpec<Layout = Stable, Mutability = WithInterior<Robust>>,
        Self: ReprC<CType = CVec<<T as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
    {
        type Store = RefSizedEncodeStore<Self, ()>;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|_fields| unsafe {
                CVec::from_raw_parts(self.as_ptr().cast_mut().cast(), self.len(), self.capacity())
            })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized>, __Interior: ExclusiveKind> EncodeRef<'a>
        for Vec<T>
    where
        T: RustSpec<Layout = Unstable>,
        Self: EncodeRefUnchecked<'a> + ReprC<CType = CVec<<T as ReprC>::CType>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a T: EncodeMove + ReprC<CType = CRef<<T as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<
            Self,
            RefVecEncodeStore<<T as ReprC>::CType, Box<[<&'a T as EncodeMove>::Store]>>,
        >;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let store = fields;

                store.encode(|stores| encode_vec(self, self.capacity(), stores))
            })
        }
    }

    unsafe impl<'a, R: ReprC<CType: CType + Copy>, __Interior: ExclusiveKind> EncodeRef<'a>
        for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMove + ReprC<CType = CRef<<R as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMove>::Store>>;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let store = fields;

                match self {
                    Some(value) => COption::Some(crate::stored::encode_ref_field(
                        value,
                        store.insert(Default::default()),
                    )),
                    None => {
                        *store = None;
                        COption::None()
                    }
                }
            })
        }
    }
    // NOTE: PartialEq is only required for decoding, Here it is only used by `debug_assert!`
    unsafe impl<'a, R: Niche<CType: Copy + PartialEq>, N: LayoutKind, __Interior: ExclusiveKind>
        EncodeRef<'a> for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMove + ReprC<CType = CRef<<R as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMove>::Store>>;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let store = fields;

                if let Some(value) = self {
                    let encoded =
                        crate::stored::encode_ref_field(value, store.insert(Default::default()));
                    debug_assert!(encoded != R::NICHE, "encoding produced NICHE value");
                    encoded
                } else {
                    *store = None;
                    R::NICHE
                }
            })
        }
    }

    unsafe impl<
        'a,
        R: ReprC<CType: CType + Copy>,
        E: ReprC<CType: CType + Copy>,
        K: SizedKind,
        __Interior: ExclusiveKind,
    > EncodeRef<'a> for Result<R, E>
    where
        Self: ReprC<CType = CResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<K>>,
        E: RustSpec<Size = RustSpecSized<K>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMove + ReprC<CType = CRef<<R as ReprC>::CType>>,
        &'a E: EncodeMove + ReprC<CType = CRef<<E as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<
            Self,
            Option<Result<<&'a R as EncodeMove>::Store, <&'a E as EncodeMove>::Store>>,
        >;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let store = fields;

                encode_shared_result(self, store)
            })
        }
    }
    unsafe impl<'a, R: Niche, E: ReprC, N: LayoutKind, __Interior: ExclusiveKind> EncodeRef<'a>
        for Result<R, E>
    where
        Self: ReprC<CType = <R as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a R: EncodeMove + ReprC<CType = CRef<<R as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, Option<<&'a R as EncodeMove>::Store>>;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let store = fields;

                match self {
                    Ok(ok) => crate::stored::encode_ref_field(ok, store.insert(Default::default())),
                    Err(_) => {
                        *store = None;
                        R::NICHE
                    }
                }
            })
        }
    }
    unsafe impl<'a, R: ReprC, E: Niche, N: LayoutKind, __Interior: ExclusiveKind> EncodeRef<'a>
        for Result<R, E>
    where
        Self: ReprC<CType = <E as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        Self: RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin> + 'a,
        &'a E: EncodeMove + ReprC<CType = CRef<<E as ReprC>::CType>>,
    {
        type Store = RefSizedEncodeStore<Self, Option<<&'a E as EncodeMove>::Store>>;

        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                let store = fields;

                match self {
                    Ok(_) => {
                        *store = None;
                        E::NICHE
                    }
                    Err(err) => crate::stored::encode_ref_field(err, store.insert(Default::default())),
                }
            })
        }
    }

    unsafe impl<'a, R: EncodeRef<'a> + RustSpec + ReprC<CType: Sized> + ?Sized, __Interior: ExclusiveKind>
        EncodeRef<'a> for ManuallyDrop<R>
    where
        Self: ReprC<CType: CType + Sized>
            + RustSpec<Size: crate::Thin, Mutability = Exclusive<__Interior>>
            + 'a,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
        &'a R: EncodeMoveUnchecked + ReprC<CType = CRef<<R as ReprC>::CType>>,
    {
        type Store = R::Store;

        fn encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = R::encode_ref(&**self, store);
            unsafe { CRef::from_raw(inner.as_ptr().cast()) }
        }
    }
    unsafe impl<'a, R: EncodeRef<'a> + RustSpec + ReprC<CType: Sized> + ?Sized, __Interior: ExclusiveKind>
        EncodeRef<'a> for ManuallyDrop<R>
    where
        Self: ReprC<CType: CType + Sized>
            + RustSpec<Size = NulTerminated, Mutability = Exclusive<__Interior>>
            + 'a,
        &'a Self: ReprC<CType = CRef<<Self as ReprC>::CType>>,
        &'a R: EncodeMoveUnchecked + ReprC<CType = CRef<<R as ReprC>::CType>>,
    {
        type Store = R::Store;

        fn encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = R::encode_ref(&**self, store);
            unsafe { CRef::from_raw(inner.as_ptr().cast()) }
        }
    }
    unsafe impl<
        'a,
        R: EncodeRef<'a> + RustSpec + ReprC<CType: Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>
            + ?Sized,
        __Interior: ExclusiveKind,
    > EncodeRef<'a> for ManuallyDrop<R>
    where
        Self: ReprC<CType: Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>
            + RustSpec<Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>
            + 'a,
        &'a Self: ReprC<CType = CWideRef<<Self as ReprC>::CType>>,
        &'a R: EncodeMoveUnchecked + ReprC<CType = CWideRef<<R as ReprC>::CType>>,
    {
        type Store = R::Store;

        fn encode_ref<'itm>(
            &'a self,
            store: &'itm mut Self::Store,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = R::encode_ref(&**self, store);
            unsafe {
                CWideRef::from_raw_parts(
                    inner.as_ptr().cast::<PointeeData<Self::CType>>(),
                    inner.metadata(),
                )
            }
        }
    }

    unsafe impl<'a, T: AbiCompatible<CType: CType + Sized> + 'a, const N: usize> EncodeRef<'a>
        for [T; N]
    where
        T: RustSpec<Layout = rust_spec::Stable>,
        &'a Self: ReprC<CType = CRef<[<T as ReprC>::CType; N]>>,
        &'a T: EncodeMove + ReprC<CType = CRef<<T as ReprC>::CType>>,
    {
        type Store = ();
        fn encode_ref<'itm>(&'a self, _: &'itm mut ()) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            // SAFETY: Array layout preserves the ABI-compatible element layout.
            // Only the pointer is converted; no element is read or copied.
            unsafe { CRef::from_raw(core::ptr::from_ref(self).cast()) }
        }
    }
    unsafe impl<'a, T: ReprC<CType: Copy> + 'a, const N: usize> EncodeRef<'a> for [T; N]
    where
        T: RustSpec<Layout = rust_spec::Unstable>,
        &'a Self: ReprC<CType = CRef<[<T as ReprC>::CType; N]>>,
        &'a T: EncodeMove + ReprC<CType = CRef<<T as ReprC>::CType>>,
    {
        type Store =
            RefSizedEncodeStore<Self, crate::sync::ArrayStore<<&'a T as EncodeMove>::Store, N>>;
        fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(|fields| {
                core::array::from_fn(|i| crate::stored::encode_ref_field(&self[i], &mut fields.0[i]))
            })
        }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized> + 'a> EncodeRefUnchecked<'a> for [T]
where
    &'a T: EncodeMoveUnchecked + ReprC<CType = CRef<<T as ReprC>::CType>>,
    &'a Self: ReprC<CType = CWideRef<[<T as ReprC>::CType]>>,
{
    type SoftStore = RefWideEncodeStore<[T], Box<[<&'a T as EncodeMoveUnchecked>::SoftStore]>>;

    unsafe fn encode_ref_unchecked<'itm>(
        &'a self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|stores| unsafe { encode_slice_unchecked(self, stores) })
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized> + 'a> EncodeRef<'a> for [T]
where
    &'a T: EncodeMove + ReprC<CType = CRef<<T as ReprC>::CType>>,
    &'a Self: ReprC<CType = CWideRef<[<T as ReprC>::CType]>>,
{
    type Store = RefWideEncodeStore<[T], Box<[<&'a T as EncodeMove>::Store]>>;

    fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|stores| encode_slice(self, stores))
    }
}

unsafe impl<'a> EncodeRefUnchecked<'a> for core::ffi::CStr {
    type SoftStore = ();

    unsafe fn encode_ref_unchecked<'itm>(&'a self, _: &'itm mut ()) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        unsafe { CRef::from_raw(self.as_ptr().cast()) }
    }
}

unsafe impl<'a> EncodeRef<'a> for core::ffi::CStr {
    type Store = ();

    fn encode_ref<'itm>(&'a self, _: &'itm mut ()) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        unsafe { CRef::from_raw(self.as_ptr().cast()) }
    }
}

fn encode_shared_result<'a, R: ReprC<CType: Copy> + 'a, E: ReprC<CType: Copy> + 'a>(
    value: &'a Result<R, E>,
    store: &mut Option<Result<<&'a R as EncodeMove>::Store, <&'a E as EncodeMove>::Store>>,
) -> CResult<R::CType, E::CType>
where
    &'a R: EncodeMove<CType = CRef<R::CType>>,
    &'a E: EncodeMove<CType = CRef<E::CType>>,
{
    match value {
        Ok(ok) => {
            let Ok(store) = store.insert(Ok(Default::default())) else {
                unreachable!()
            };
            CResult::Ok(crate::stored::encode_ref_field(ok, store))
        }
        Err(err) => {
            let Err(store) = store.insert(Err(Default::default())) else {
                unreachable!()
            };
            CResult::Err(crate::stored::encode_ref_field(err, store))
        }
    }
}

// Copy only the shared reference, then retain its converted representation and
// backing store for the outer reference. The pointee is never cloned.
unsafe impl<'a, 'b: 'a, R: ?Sized, __Interior: ExclusiveKind> EncodeRefUnchecked<'a> for &'b R
where
    Self: EncodeMoveUnchecked
        + ReprC<CType: Sized>
        + RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
{
    type SoftStore = RefSizedEncodeStore<Self, <Self as EncodeMoveUnchecked>::SoftStore>;

    unsafe fn encode_ref_unchecked<'itm>(
        &'a self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| unsafe { (*self).soft_encode_unchecked(fields) })
    }
}

unsafe impl<'a, 'b: 'a, R: ?Sized, __Interior: ExclusiveKind> EncodeRef<'a> for &'b R
where
    Self: EncodeMove
        + ReprC<CType: Sized>
        + RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
{
    type Store = RefSizedEncodeStore<Self, <Self as EncodeMove>::Store>;

    fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| (*self).soft_encode(fields))
    }
}

// A shared borrow of an exclusive reference only permits shared pointee access.
// The mutable carrier is retained solely to match the nested reference ABI.
unsafe impl<'a, 'b: 'a, R: RustSpec + ?Sized, __Interior: ExclusiveKind> EncodeRefUnchecked<'a>
    for &'b mut R
where
    Self: ReprC<CType: Copy + Sized>
        + RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
    &'a R: EncodeMoveUnchecked + ReprC<CType: Copy + Sized>,
{
    type SoftStore = RefSizedEncodeStore<Self, <&'a R as EncodeMoveUnchecked>::SoftStore>;

    unsafe fn encode_ref_unchecked<'itm>(
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

unsafe impl<'a, 'b: 'a, R: RustSpec + ?Sized, __Interior: ExclusiveKind> EncodeRef<'a> for &'b mut R
where
    Self: ReprC<CType: Copy + Sized>
        + RustSpec<Mutability = Exclusive<__Interior>, Size: crate::Thin>,
    &'a R: EncodeMove + ReprC<CType: Copy + Sized>,
{
    type Store = RefSizedEncodeStore<Self, <&'a R as EncodeMove>::Store>;

    fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| {
            let value = (&**self).soft_encode(fields);
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
    ($ty:ty, $view:ident) => {
        unsafe impl<'a> EncodeRefUnchecked<'a> for $ty {
            type SoftStore = RefSizedEncodeStore<Self, ()>;
            unsafe fn encode_ref_unchecked<'itm>(
                &'a self,
                store: &'itm mut Self::SoftStore,
            ) -> <&'a Self as ReprC>::CType
            where
                'a: 'itm,
            {
                store.encode(|_fields| {
                    let view = unsafe { self.$view().soft_encode_unchecked(&mut ()) };
                    // The pointee remains borrowed; only its raw pointer carrier changes.
                    unsafe { core::mem::transmute_copy(&view) }
                })
            }
        }
        unsafe impl<'a> EncodeRef<'a> for $ty {
            type Store = RefSizedEncodeStore<Self, ()>;
            fn encode_ref<'itm>(
                &'a self,
                store: &'itm mut Self::Store,
            ) -> <&'a Self as ReprC>::CType
            where
                'a: 'itm,
            {
                store.encode(|_fields| {
                    let view = self.$view().soft_encode(&mut ());
                    unsafe { core::mem::transmute_copy(&view) }
                })
            }
        }
    };
}
#[cfg(feature = "alloc")]
impl_buffer_shared!(alloc::string::String, as_str);
#[cfg(feature = "alloc")]
impl_buffer_shared!(alloc::ffi::CString, as_c_str);

#[cfg(feature = "alloc")]
fn encode_slice<'a, T: ReprC<CType: CType + Copy> + 'a, S: Store + Default>(
    source: &'a [T],
    stores: &mut Box<[S]>,
) -> Box<[T::CType]>
where
    &'a T: EncodeMove<CType = CRef<T::CType>, Store = S>,
{
    *stores = core::iter::repeat_with(Default::default)
        .take(source.len())
        .collect();
    source
        .iter()
        .zip(stores.iter_mut())
        .map(|(item, store)| crate::stored::encode_ref_field(item, store))
        .collect()
}

#[cfg(feature = "alloc")]
unsafe fn encode_slice_unchecked<'a, T: ReprC<CType: CType + Copy> + 'a, S: Store + Default>(
    source: &'a [T],
    stores: &mut Box<[S]>,
) -> Box<[T::CType]>
where
    &'a T: EncodeMoveUnchecked<CType = CRef<T::CType>, SoftStore = S>,
{
    *stores = core::iter::repeat_with(Default::default)
        .take(source.len())
        .collect();
    source
        .iter()
        .zip(stores.iter_mut())
        .map(|(item, store)| unsafe { crate::stored::encode_ref_field_unchecked(item, store) })
        .collect()
}

#[cfg(feature = "alloc")]
fn encode_vec<'a, T: ReprC<CType: CType + Copy> + 'a, S: Store + Default>(
    source: &'a [T],
    capacity: usize,
    stores: &mut Box<[S]>,
) -> Vec<T::CType>
where
    &'a T: EncodeMove<CType = CRef<T::CType>, Store = S>,
{
    *stores = core::iter::repeat_with(Default::default)
        .take(source.len())
        .collect();
    let mut encoded = Vec::with_capacity(capacity);
    encoded.extend(
        source
            .iter()
            .zip(stores.iter_mut())
            .map(|(item, store)| crate::stored::encode_ref_field(item, store)),
    );
    encoded
}

#[cfg(feature = "alloc")]
unsafe fn encode_vec_unchecked<'a, T: ReprC<CType: CType + Copy> + 'a, S: Store + Default>(
    source: &'a [T],
    capacity: usize,
    stores: &mut Box<[S]>,
) -> Vec<T::CType>
where
    &'a T: EncodeMoveUnchecked<CType = CRef<T::CType>, SoftStore = S>,
{
    *stores = core::iter::repeat_with(Default::default)
        .take(source.len())
        .collect();
    let mut encoded = Vec::with_capacity(capacity);
    encoded.extend(
        source
            .iter()
            .zip(stores.iter_mut())
            .map(|(item, store)| unsafe { crate::stored::encode_ref_field_unchecked(item, store) }),
    );
    encoded
}
