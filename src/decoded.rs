//! Referent-specific decoding and mutable writeback for unstable layouts.
#[cfg(feature = "alloc")]
use crate::sync::{RefDstDecodeStore, RefMutSliceDecodeStore};
use crate::{
    CType, ReprC,
    borrow::{Borrow, BorrowCast, borrow_cast},
    ffi::NulTerminatedBuf,
    stored::{DecodeOwned, EmptyStore, EncodeMove, Store},
    sync::{RefMutSizedDecodeStore, RefSizedDecodeStore},
    transmute::InteriorTrap,
};
#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
use disjoint_impls::disjoint_impls;
#[cfg(feature = "alloc")]
use rust_spec::size::{MetaSized, SliceLike};
use rust_spec::{
    RustSpec, Stable, Unstable,
    layout::Robust,
    mutability::{Exclusive, ExclusiveKind},
    size::{NulTerminated, Sized as RustSpecSized, SizedKind},
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

    unsafe impl<'d, R: NulTerminatedBuf + ?Sized, __Interior: ExclusiveKind> DecodeRef<'d> for R
    where
        R: 'd,
        &'d Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated, Mutability = Exclusive<__Interior>>
            + ReprC<CType = <<R as NulTerminatedBuf>::Header as ReprC>::CType>,
        <R as ReprC>::CType: CType + Sized,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable> + ReprC<CType: Sized> + CType,
        <R as RustSpec>::Mutability: InteriorTrap<Trap = Robust>,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn decode_ref<'itm: 'd>(source: <&'d Self as ReprC>::CType, store: &'itm mut ()) -> Option<&'d Self> {
            if source.as_ptr().is_null() {
                return None;
            }

            Some(unsafe { Self::decode_ref_unchecked(source, store) })
        }

        #[inline(always)]
        unsafe fn decode_ref_unchecked<'itm: 'd>(source: <&'d Self as ReprC>::CType, (): &mut ()) -> &'d Self {
            let ptr = source.as_ptr().cast();
            unsafe { R::from_raw(ptr) }
        }
    }
    unsafe impl<
        'd,
        R: ReprC<CType: BorrowCast<AsConst: Copy> + Copy> + Borrow,
        S: SizedKind,
        __Interior: ExclusiveKind,
    > DecodeRef<'d> for R
    where
        R: 'd,
        &'d Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>, Mutability = Exclusive<__Interior>>,
        <R as Borrow>::Borrowed: DecodeOwned<'d> + ReprC<CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
        <R as RustSpec>::Mutability: InteriorTrap<Trap = Robust>,
    {
        type Store = RefSizedDecodeStore<R, <R::Borrowed as DecodeOwned<'d>>::Store>;

        #[inline(always)]
        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut <Self as DecodeRef<'d>>::Store,
        ) -> Option<&'d Self> {
            let ptr = source.as_ptr();
            unsafe { decode_ref_sized(ptr, store) }
        }

        #[inline(always)]
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut <Self as DecodeRef<'d>>::Store,
        ) -> &'d Self {
            let ptr = source.as_ptr();
            let source = borrow_cast(unsafe { ptr.read() });
            store
                .decode(|fields| {
                    let value = unsafe { DecodeOwned::soft_decode_unchecked(source, fields) };
                    Some(Borrow::from_borrow(value))
                })
                .expect("unchecked decoding produces a value")
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<
        'd,
        R: DecodeOwned<'d, CType: BorrowCast<AsConst: Copy> + Copy> + Borrow + Clone,
        __Interior: ExclusiveKind,
    > DecodeRef<'d> for [R]
    where
        R: 'd,
        &'d Self: RustSpec<Layout = Unstable>,
        [R]: RustSpec<Layout = Unstable, Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>,
        <R as Borrow>::Borrowed: DecodeOwned<'d> + ReprC<CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefDstDecodeStore<[R], Box<[<R::Borrowed as DecodeOwned<'d>>::Store]>>;

        #[inline(always)]
        unsafe fn decode_ref<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut <Self as DecodeRef<'d>>::Store,
        ) -> Option<&'d Self> {
            store
                .decode(|fields| {
                    let source = unsafe { source.into_ref()? };
                    let values = source.iter().map(|value| unsafe { core::ptr::read(value) });
                    unsafe { decode_ref_slice_elements(values, fields) }
                })
                .map(Vec::as_slice)
        }

        #[inline(always)]
        unsafe fn decode_ref_unchecked<'itm: 'd>(
            source: <&'d Self as ReprC>::CType,
            store: &'itm mut <Self as DecodeRef<'d>>::Store,
        ) -> &'d Self {
            store
                .decode(|fields| {
                    let source = unsafe { source.into_ref().unwrap_unchecked() };
                    Some(unsafe {
                        decode_ref_slice_elements_unchecked(
                            source.iter().map(|value| core::ptr::read(value)),
                            fields,
                        )
                    })
                })
                .expect("unchecked decoding produces a value")
                .as_slice()
        }
    }
}

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

    unsafe impl<'d, R: ReprC<CType: BorrowCast<AsConst: Copy> + Copy> + Borrow, S: SizedKind>
        DecodeMut<'d> for R
    where
        R: 'd,
        &'d mut Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>> + EncodeMove<Store: EmptyStore>,
        <R as Borrow>::Borrowed: DecodeOwned<'d> + ReprC<CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefMutSizedDecodeStore<R, <R::Borrowed as DecodeOwned<'d>>::Store>;

        #[inline(always)]
        unsafe fn decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut <Self as DecodeMut<'d>>::Store,
        ) -> Option<&'d mut Self> {
            let source = source.as_ptr();
            unsafe {
                store.decode(source, |fields| {
                    let source = borrow_cast(source.read());
                    let value = DecodeOwned::soft_decode(source, fields)?;
                    Some(Borrow::from_borrow(value))
                })
            }
        }

        #[inline(always)]
        unsafe fn decode_mut_unchecked<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut <Self as DecodeMut<'d>>::Store,
        ) -> &'d mut Self {
            let source = source.as_ptr();
            unsafe {
                store.decode(source, |fields| {
                    let source = borrow_cast(source.read());
                    let value = DecodeOwned::soft_decode_unchecked(source, fields);
                    Some(Borrow::from_borrow(value))
                })
            }
            .expect("unchecked decoding produces a value")
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<
        'd,
        R: DecodeOwned<'d, CType: BorrowCast<AsConst: Copy> + Copy>
            + Borrow
            + EncodeMove<Store: EmptyStore>,
    > DecodeMut<'d> for [R]
    where
        R: 'd,
        &'d mut Self: RustSpec<Layout = Unstable>,
        [R]: RustSpec<Layout = Unstable, Size = MetaSized<SliceLike>>,
        <R as Borrow>::Borrowed: DecodeOwned<'d> + ReprC<CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefMutSliceDecodeStore<R, Box<[<R::Borrowed as DecodeOwned<'d>>::Store]>>;

        #[inline(always)]
        unsafe fn decode_mut<'itm: 'd>(
            source: <&'d mut Self as ReprC>::CType,
            store: &'itm mut <Self as DecodeMut<'d>>::Store,
        ) -> Option<&'d mut Self> {
            unsafe {
                store.decode(source, |values, fields| {
                    decode_ref_slice_elements(values.iter().copied(), fields)
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
                    Some(decode_ref_slice_elements_unchecked(
                        values.iter().copied(),
                        fields,
                    ))
                })
            }
            .expect("unchecked decoding produces a value")
        }
    }
}

#[inline(always)]
unsafe fn decode_ref_sized<'d, 'itm: 'd, R>(
    source: *const R::CType,
    store: &'itm mut RefSizedDecodeStore<R, <R::Borrowed as DecodeOwned<'d>>::Store>,
) -> Option<&'d R>
where
    R: ReprC<CType: BorrowCast<AsConst: Copy> + Copy> + Borrow,
    R::Borrowed: DecodeOwned<'d> + ReprC<CType = <R::CType as BorrowCast>::AsConst>,
{
    if source.is_null() {
        return None;
    }

    let source = borrow_cast(unsafe { source.read() });
    store.decode(|fields| {
        let value = unsafe { DecodeOwned::soft_decode(source, fields)? };
        Some(R::from_borrow(value))
    })
}

#[cfg(feature = "alloc")]
#[inline(always)]
unsafe fn decode_ref_slice_elements<'d, 'itm: 'd, R, I>(
    source: I,
    store: &'itm mut Box<[<R::Borrowed as DecodeOwned<'d>>::Store]>,
) -> Option<Vec<R>>
where
    R: ReprC<CType: BorrowCast<AsConst: Copy> + Copy> + Borrow,
    R::Borrowed: DecodeOwned<'d> + ReprC<CType = <R::CType as BorrowCast>::AsConst>,
    I: ExactSizeIterator<Item = R::CType>,
{
    *store = core::iter::repeat_with(Default::default)
        .take(source.len())
        .collect();

    let mut value = Vec::with_capacity(source.len());
    for (elem, elem_store) in source.zip(store.iter_mut()) {
        let source = borrow_cast(elem);
        let decoded = unsafe { DecodeOwned::soft_decode(source, elem_store)? };
        value.push(R::from_borrow(decoded));
    }

    Some(value)
}

#[cfg(feature = "alloc")]
#[inline(always)]
unsafe fn decode_ref_slice_elements_unchecked<'d, 'itm: 'd, R, I>(
    source: I,
    store: &'itm mut Box<[<R::Borrowed as DecodeOwned<'d>>::Store]>,
) -> Vec<R>
where
    R: ReprC<CType: BorrowCast<AsConst: Copy> + Copy> + Borrow,
    R::Borrowed: DecodeOwned<'d> + ReprC<CType = <R::CType as BorrowCast>::AsConst>,
    I: ExactSizeIterator<Item = R::CType>,
{
    *store = core::iter::repeat_with(Default::default)
        .take(source.len())
        .collect();

    source
        .zip(store.iter_mut())
        .map(|(elem, elem_store)| {
            let source = borrow_cast(elem);
            let decoded = unsafe { DecodeOwned::soft_decode_unchecked(source, elem_store) };
            R::from_borrow(decoded)
        })
        .collect()
}
