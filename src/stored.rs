#[cfg(feature = "alloc")]
use alloc_crate::{borrow::ToOwned as StdToOwned, boxed::Box, vec::Vec};

use disjoint_impls::disjoint_impls;

#[cfg(feature = "alloc")]
use crate::transmute::transmute_from_target_boxed_dst;
use crate::{
    ExternC, ReprC, assert_arr_has_non_zero_len,
    borrow::{Borrow, BorrowCast, ToOwned, borrow_cast, borrow_cast_mut},
    boxed::{CBox, CBoxedSlice},
    ir::{ReprFamily, Robust, Transmuted},
    niche::{Niche, NicheFamily, WithCustomNiche, WithoutNiche},
    option::COption,
    out_ptr::Zst,
    result::CResult,
    size::{MetaSized, PointeeSized, SizeFamily, SliceLike, Thin, Wide},
    slice::{CSlice, CSliceMut},
    transmute::{
        CheckedTransmute, transmute_from_target, transmute_from_target_dst_mut,
        transmute_from_target_ref_dst, transmute_into_target, transmute_into_target_boxed_dst,
    },
    transmute_into_target_dst_mut, transmute_into_target_ref_dst,
};

// TODO: Could the store just be synced on drop?
// FIXME: Encode types can never error during sync
pub trait Store: Sized {
    fn sync(self) -> Option<()>;
}

disjoint_impls! {
    /// Facilitates conversion from a Rust type into a corresponding C-compatible representation.
    pub trait SoftEncodeOwned: ExternC<CType: Copy> + Sized {
        /// Auxiliary storage used during conversion. If storage is not used, set the type to `()`.
        ///
        /// Use cases include:
        /// - Keeping the result of the conversion of references of [`Stored`] types
        /// - Storing mutable references that need to be updated in [`Store::sync`]
        ///
        /// Conceptually, serves a role similar to the "context" captured by a closure.
        type Store: Store + Default;

        /// Convert from [`Self`] into [`Self::CType`].
        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm;
    }

    impl<R: ReprC + Copy> SoftEncodeOwned for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            self
        }
    }
    impl<R> SoftEncodeOwned for R
    where
        R: ReprFamily<Kind = Transmuted> + CheckedTransmute<Target: SoftEncodeOwned>,
    {
        type Store = <R::Target as SoftEncodeOwned>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            transmute_into_target(self).encode(store)
        }
    }

    impl<R: ReprFamily<Kind = Robust> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized>
        SoftEncodeOwned for &R
    where
        Self: ReprFamily<Kind = Self>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: ReprC,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            CSlice::from_raw_parts(self.as_ptr(), self.metadata())
        }
    }
    impl<'a, R: ReprFamily<Kind = Transmuted> + CheckedTransmute + ?Sized> SoftEncodeOwned for &'a R
    where
        Self: ReprFamily<Kind = Self>,
        <R as CheckedTransmute>::Target: Wide,
        &'a <R as CheckedTransmute>::Target: SoftEncodeOwned,
        R: Wide<Metadata = <<R as CheckedTransmute>::Target as Wide>::Metadata>,
    {
        type Store = <&'a R::Target as SoftEncodeOwned>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            transmute_into_target_ref_dst(self).encode(store)
        }
    }
    impl<R: ReprFamily<Kind = R> + SizeFamily<Kind = crate::size::Sized<K>>, K> SoftEncodeOwned for &R
    where
        Self: ReprFamily<Kind = Self>,
        R: Clone + SoftEncodeOwned,
    {
        type Store = RefSizedEncodeStore<R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let owned = self.clone();
            let ctype = owned.encode(&mut store.store);
            store.ctype.insert(ctype)
        }
    }
    // TODO: We should prevent `Sized` opaque types here because they can't be decoded, likewise for mutable
    // This can be achieved if `ExternTypeLike` is only used on extern types. Opaque types should be `Sized`
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = R> + SizeFamily<Kind: PointeeSized> + ?Sized> SoftEncodeOwned for &R
    where
        Self: ReprFamily<Kind = Self> + ExternC<CType = <<<R as StdToOwned>::Owned as ExternC>::CType as BorrowCast>::AsConst>,
        R: StdToOwned<Owned: ExternC<CType: BorrowCast<AsConst: Copy>> + SoftEncodeOwned>,
    {
        type Store = RefDstEncodeStore<R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let owned = StdToOwned::to_owned(self);
            let ctype = owned.encode(&mut store.store);
            borrow_cast(*store.ctype.insert(ctype))
        }
    }

    impl<R: ReprFamily<Kind = Robust> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized>
        SoftEncodeOwned for &mut R
    where
        Self: ReprFamily<Kind = Self>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: ReprC,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            CSliceMut::from_raw_parts_mut(self.as_mut_ptr(), self.metadata())
        }
    }
    impl<'a, R: ReprFamily<Kind = Transmuted> + CheckedTransmute + ?Sized> SoftEncodeOwned for &'a mut R
    where
        Self: ReprFamily<Kind = Self>,
        <R as CheckedTransmute>::Target: Wide,
        &'a mut <R as CheckedTransmute>::Target: SoftEncodeOwned,
        R: Wide<Metadata = <<R as CheckedTransmute>::Target as Wide>::Metadata>,
    {
        type Store = <&'a mut R::Target as SoftEncodeOwned>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            transmute_into_target_dst_mut(self).encode(store)
        }
    }
    impl<'a, R: ReprFamily<Kind = R> + SizeFamily<Kind = crate::size::Sized<K>>, K> SoftEncodeOwned
        for &'a mut R
    where
        Self: ReprFamily<Kind = Self>,
        R: Clone + SoftEncodeOwned + SoftDecodeOwned<'a>,
    {
        type Store = RefMutSizedEncodeStore<'a, R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let original = store.original.insert(self);
            let owned = original.clone();
            let ctype = owned.encode(&mut store.store);
            store.ctype.insert(ctype)
        }
    }
    // TODO: We should prevent Sized opaque types here because they can't be decoded
    #[cfg(feature = "alloc")]
    impl<'a, R: ReprFamily<Kind = R> + SizeFamily<Kind: PointeeSized> + ?Sized> SoftEncodeOwned for &'a mut R
    where
        Self: ReprFamily<Kind = Self> + ExternC<CType = <<<R as StdToOwned>::Owned as ExternC>::CType as BorrowCast>::AsMut>,
        R: StdToOwned<Owned: ExternC<CType: BorrowCast<AsMut: Copy>> + SoftEncodeOwned + SoftDecodeOwned<'a>>,
    {
        type Store = RefMutDstEncodeStore<'a, R>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let original = store.original.insert(self);
            let owned = StdToOwned::to_owned(&**original);
            let ctype = owned.encode(&mut store.store);
            borrow_cast_mut(*store.ctype.insert(ctype))
        }
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Robust> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized>
        SoftEncodeOwned for Box<R>
    where
        Self: ReprFamily<Kind = Self>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: ReprC,
    {
        type Store = ();

        fn encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let len = self.metadata();

            let data = R::into_non_null(self);
            CBoxedSlice::from_raw_parts(data, len)
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted> + CheckedTransmute + ?Sized>
        SoftEncodeOwned for Box<R>
    where
        Self: ReprFamily<Kind = Self>,
        <R as CheckedTransmute>::Target: Wide,
        Box<<R as CheckedTransmute>::Target>: SoftEncodeOwned,
        R: Wide<Metadata = <<R as CheckedTransmute>::Target as Wide>::Metadata>,
    {
        type Store = <Box<R::Target> as SoftEncodeOwned>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            transmute_into_target_boxed_dst(self).encode(store)
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = R> + SizeFamily<Kind = crate::size::Sized<K>>, K> SoftEncodeOwned
        for Box<R>
    where
        Self: ReprFamily<Kind = Self>,
        R: SoftEncodeOwned<CType: Copy>,
    {
        type Store = R::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            CBox::from_box(Some(Box::new((*self).encode(store))))
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = R> + SizeFamily<Kind: PointeeSized> + ?Sized> SoftEncodeOwned for Box<R>
    where
        Self: ReprFamily<Kind = Self> + ExternC<CType = <<R as StdToOwned>::Owned as ExternC>::CType>,
        R: StdToOwned<Owned: SoftEncodeOwned>,
        Self: Into<<R as StdToOwned>::Owned>,
    {
        type Store = <R::Owned as SoftEncodeOwned>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.into().encode(store)
        }
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Robust> + ReprC> SoftEncodeOwned for Vec<R>
    where
        Self: ReprFamily<Kind = Self>,
    {
        type Store = <Box<[R]> as SoftEncodeOwned>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            SoftEncodeOwned::encode(self.into_boxed_slice(), store)
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted>> SoftEncodeOwned for Vec<R>
    where
        Self: ReprFamily<Kind = Self> + ExternC<CType = <Box<[R]> as ExternC>::CType>,
        Box<[R]>: SoftEncodeOwned,
    {
        type Store = <Box<[R]> as SoftEncodeOwned>::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            SoftEncodeOwned::encode(self.into_boxed_slice(), store)
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = R> + SoftEncodeOwned> SoftEncodeOwned for Vec<R>
    where
        Self: ReprFamily<Kind = Self>,
    {
        type Store = Box<[R::Store]>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            *store = (0..self.len()).map(|_| Default::default()).collect();

            let ctypes = self
                .into_iter()
                .zip(store)
                .map(|(item, store)| item.encode(store))
                .collect::<Box<[_]>>();

            CBoxedSlice::from_boxed_slice(Some(ctypes))
        }
    }

    impl<R: SoftEncodeOwned, const N: usize> SoftEncodeOwned for [R; N]
    where
        Self: ReprFamily<Kind = Self>,
    {
        type Store = ArrayStore<R::Store, N>;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            assert_arr_has_non_zero_len::<N>();

            let store = &mut store.0;

            let mut items = self.into_iter();
            let mut stores = store.iter_mut();

            core::array::from_fn(|_| {
                let item = items.next().unwrap();
                let store = stores.next().unwrap();

                item.encode(store)
            })
        }
    }

    impl<R: NicheFamily<Kind = WithoutNiche> + SoftEncodeOwned> SoftEncodeOwned for Option<R>
    where
        Self: ReprFamily<Kind = Self>,
        <R as ExternC>::CType: Copy,
    {
        type Store = R::Store;

        fn encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.map(|v| v.encode(store)).into()
        }
    }
    impl<R: NicheFamily<Kind = WithCustomNiche> + SoftEncodeOwned + Niche> SoftEncodeOwned for Option<R>
    where
        Self: ReprFamily<Kind = Self>,
    {
        type Store = R::Store;

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            if let Some(value) = self {
                return value.encode(store);
            }

            R::NICHE_VALUE
        }
    }

    impl<
        R: NicheFamily<Kind = WithoutNiche> + SoftEncodeOwned,
        E: NicheFamily<Kind = WithoutNiche> + SoftEncodeOwned,
    > SoftEncodeOwned for Result<R, E>
    where
        Self: ReprFamily<Kind = Self>,
        <R as ExternC>::CType: Copy,
        <E as ExternC>::CType: Copy,
    {
        // TODO: a union would save space, this issue is even more pronounced when deriving user-defined enums
        // Check other places, for instance Decoding
        type Store = (R::Store, E::Store);

        fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Ok(ok) => CResult::Ok(ok.encode(&mut store.0)),
                Err(err) => CResult::Err(err.encode(&mut store.1)),
            }
        }
    }
    // TODO: Implement for niche optimized Results
}

disjoint_impls! {
    pub trait SoftDecodeOwned<'d>: ExternC<CType: Sized> + Sized {
        type Store: Store + Default;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self>;
    }

    impl<'d, R: ReprC + Sized> SoftDecodeOwned<'d> for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            Some(source)
        }
    }
    impl<'d, R: CheckedTransmute<Target: SoftDecodeOwned<'d>>> SoftDecodeOwned<'d> for R
    where
        Self: ReprFamily<Kind = Transmuted>,
    {
        type Store = <R::Target as SoftDecodeOwned<'d>>::Store;

        #[inline(always)]
        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { SoftDecodeOwned::decode(source, store) }.and_then(transmute_from_target)
        }
    }

    impl<'d, R: ReprFamily<Kind = Robust> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized>
        SoftDecodeOwned<'d> for &'d R
    where
        Self: ReprFamily<Kind = Self>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: ReprC,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source == CSlice::none() {
                return None;
            }

            Some(unsafe { R::from_raw_parts(source.as_ptr(), source.len()) })
        }
    }
    impl<'d, R: ReprFamily<Kind = Transmuted> + CheckedTransmute + ?Sized> SoftDecodeOwned<'d> for &'d R
    where
        Self: ReprFamily<Kind = Self>,
        <R as CheckedTransmute>::Target: Wide,
        &'d <R as CheckedTransmute>::Target: SoftDecodeOwned<'d>,
        R: Wide<Metadata = <<R as CheckedTransmute>::Target as Wide>::Metadata>,
    {
        type Store = <&'d R::Target as SoftDecodeOwned<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            transmute_from_target_ref_dst(unsafe {
                <&R::Target as SoftDecodeOwned>::decode(source, store)?
            })
        }
    }
    impl<'d, R: ReprFamily<Kind = R> + SizeFamily<Kind: Thin> + ToOwned<'d>> SoftDecodeOwned<'d>
        for &'d R
    where
        Self: ReprFamily<Kind = Self>,
        R: ExternC<CType: BorrowCast<AsConst: Sized>> + Borrow<Borrowed<'d>: SoftDecodeOwned<'d>>,
        <R as Borrow>::Borrowed<'d>: ExternC<CType = <<R as ExternC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefSizedDecodeStore<R, <R::Borrowed<'d> as SoftDecodeOwned<'d>>::Store>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let source = borrow_cast(unsafe { source.read() });

            let value = unsafe {
                <R::Borrowed<'d> as SoftDecodeOwned>::decode(source, &mut store.store)?
            };

            Some(store.value.insert(ToOwned::to_owned(value)))
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = R> + SizeFamily<Kind = MetaSized<SliceLike>> + StdToOwned + ?Sized>
        SoftDecodeOwned<'d> for &'d R
    where
        Self: ReprFamily<Kind = Self> + ExternC<CType: Copy>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: SoftDecodeOwned<'d>,
    {
        type Store = RefDstDecodeStore<R, Box<[<R::Data as SoftDecodeOwned<'d>>::Store]>>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unimplemented!()
            //let source = unsafe { source.into_rust()? };

            //*store.store = core::iter::repeat_with(Default::default)
            //    .take(source.len())
            //    .collect();

            //let owned = source
            //    .iter()
            //    .cloned()
            //    .zip(store.store.iter_mut())
            //    .map(|(item, store)| unsafe { R::Data::decode(item.view(), store) })
            //    .collect::<Option<Vec<_>>>()?;

            //Some(core::borrow::Borrow::borrow(store.value.insert(owned)))
        }
    }

    impl<'d, R: ReprFamily<Kind = Robust> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized>
        SoftDecodeOwned<'d> for &'d mut R
    where
        Self: ReprFamily<Kind = Self>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: ReprC,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(mut source: Self::CType, (): &mut ()) -> Option<Self> {
            if source == CSliceMut::none() {
                return None;
            }

            Some(unsafe { R::from_raw_parts_mut(source.as_mut_ptr(), source.len()) })
        }
    }
    impl<'d, R: ReprFamily<Kind = Transmuted> + CheckedTransmute + ?Sized> SoftDecodeOwned<'d>
        for &'d mut R
    where
        Self: ReprFamily<Kind = Self>,
        <R as CheckedTransmute>::Target: Wide,
        &'d mut <R as CheckedTransmute>::Target: SoftDecodeOwned<'d>,
        R: Wide<Metadata = <<R as CheckedTransmute>::Target as Wide>::Metadata>,
    {
        type Store = <&'d mut R::Target as SoftDecodeOwned<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            transmute_from_target_dst_mut(unsafe {
                <&mut R::Target as SoftDecodeOwned>::decode(source, store)?
            })
        }
    }
    impl<'d, R: ReprFamily<Kind = R> + SizeFamily<Kind: Thin> + ToOwned<'d>> SoftDecodeOwned<'d>
        for &'d mut R
    where
        Self: ReprFamily<Kind = Self>,
        R: ExternC<CType: BorrowCast<AsConst: Sized>> + Borrow<Borrowed<'d>: SoftDecodeOwned<'d>>,
        <R as Borrow>::Borrowed<'d>: ExternC<CType = <<R as ExternC>::CType as BorrowCast>::AsConst>,
    {
        type Store =
            RefMutSizedDecodeStore<R, <R::Borrowed<'d> as SoftDecodeOwned<'d>>::Store>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let source = borrow_cast(unsafe { source.read() });

            let value = unsafe {
                <R::Borrowed<'d> as SoftDecodeOwned>::decode(source, &mut store.store)?
            };
            Some(store.value.insert(ToOwned::to_owned(value)))
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = R> + SizeFamily<Kind = MetaSized<SliceLike>> + StdToOwned + ?Sized>
        SoftDecodeOwned<'d> for &'d mut R
    where
        Self: ReprFamily<Kind = Self> + ExternC<CType: Copy>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: SoftDecodeOwned<'d>,
    {
        type Store = RefMutDstDecodeStore<R, Box<[<R::Data as SoftDecodeOwned<'d>>::Store]>>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unimplemented!()
            //let value = unsafe { R::Owned::decode(source, &mut store.store)? };

            //Some(core::borrow::BorrowMut::borrow_mut(
            //    store.value.insert(value),
            //))
        }
    }

    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = Robust> + SizeFamily<Kind = MetaSized<SliceLike>> + StdToOwned<Owned: Into<Self>> + ?Sized>
        SoftDecodeOwned<'d> for Box<R>
    where
        Self: ReprFamily<Kind = Self>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: ReprC,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let source = borrow_cast(source);

            if source == CSlice::none() {
                return None;
            }

            Some(
                unsafe { R::from_raw_parts(source.as_ptr(), source.len()) }
                    .to_owned()
                    .into(),
            )
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = Transmuted> + CheckedTransmute + ?Sized> SoftDecodeOwned<'d>
        for Box<R>
    where
        Self: ReprFamily<Kind = Self>,
        <R as CheckedTransmute>::Target: Wide,
        Box<<R as CheckedTransmute>::Target>: SoftDecodeOwned<'d>,
        R: Wide<Metadata = <<R as CheckedTransmute>::Target as Wide>::Metadata>,
    {
        type Store = <Box<R::Target> as SoftDecodeOwned<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { SoftDecodeOwned::decode(source, store) }.and_then(transmute_from_target_boxed_dst)
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = R> + SizeFamily<Kind = crate::size::Sized<K>>, K> SoftDecodeOwned<'d>
        for Box<R>
    where
        Self: ReprFamily<Kind = Self>,
        R: SoftDecodeOwned<'d, CType: Copy>,
    {
        type Store = R::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            let source = unsafe { source.read() };
            let value = unsafe { R::decode(source, store)? };
            Some(Box::new(value))
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = R> + SizeFamily<Kind: PointeeSized> + StdToOwned + ?Sized>
        SoftDecodeOwned<'d> for Box<R>
    where
        Self: ReprFamily<Kind = Self> + ExternC<CType = <<R as StdToOwned>::Owned as ExternC>::CType>,
        <R as StdToOwned>::Owned: SoftDecodeOwned<'d> + Into<Self>,
    {
        type Store = <R::Owned as SoftDecodeOwned<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { R::Owned::decode(source, store) }.map(Into::into)
        }
    }

    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = Robust> + ReprC + Copy> SoftDecodeOwned<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Self>,
    {
        type Store = ();

        unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            unsafe { <Box<[R]> as DecodeOwned>::decode(source) }.map(Into::into)
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = Transmuted>> SoftDecodeOwned<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Self>,
        Box<[R]>: SoftDecodeOwned<'d>,
    {
        type Store = <Box<[R]> as SoftDecodeOwned<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { Box::decode(source, store) }.map(Into::into)
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = R> + SoftDecodeOwned<'d>> SoftDecodeOwned<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Self>,
        <R as ExternC>::CType: Copy,
    {
        type Store = Box<[R::Store]>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            let source = unsafe { source.into_rust()? };

            *store = core::iter::repeat_with(Default::default)
                .take(source.len())
                .collect();

            source
                .into_vec()
                .into_iter()
                .zip(store)
                .map(|(item, store)| unsafe { R::decode(item, store) })
                .collect()
        }
    }

    impl<'d, R: SoftDecodeOwned<'d, CType: Copy>, const N: usize> SoftDecodeOwned<'d> for [R; N]
    where
        Self: ReprFamily<Kind = Self>,
    {
        type Store = ArrayStore<R::Store, N>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            decode_stored_array(source, store, |item, store| unsafe {
                R::decode(item, store)
            })
        }
    }

    impl<'d, R: NicheFamily<Kind = WithoutNiche> + SoftDecodeOwned<'d>> SoftDecodeOwned<'d>
        for Option<R>
    where
        Self: ReprFamily<Kind = Self>,
        <R as ExternC>::CType: Copy,
    {
        type Store = R::Store;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            decode_stored_option_without_niche(source, store, |source, store| unsafe {
                SoftDecodeOwned::decode(source, store)
            })
        }
    }
    impl<'d, R: NicheFamily<Kind = WithCustomNiche> + SoftDecodeOwned<'d> + Niche<CType: PartialEq>>
        SoftDecodeOwned<'d> for Option<R>
    where
        Self: ReprFamily<Kind = Self> + ExternC<CType = <R as ExternC>::CType>,
        <R as ExternC>::CType: Sized,
    {
        type Store = <R as SoftDecodeOwned<'d>>::Store;

        unsafe fn decode<'itm: 'd>(source: R::CType, store: &'itm mut Self::Store) -> Option<Self> {
            decode_stored_option_with_custom_niche(
                source,
                store,
                R::NICHE_VALUE,
                |source, store| unsafe { R::decode(source, store) },
            )
        }
    }

    impl<'d, R: NicheFamily<Kind = WithoutNiche>, E: NicheFamily<Kind = WithoutNiche>>
        SoftDecodeOwned<'d> for Result<R, E>
    where
        Self: ReprFamily<Kind = Self>,
        R: SoftDecodeOwned<'d, CType: Copy>,
        E: SoftDecodeOwned<'d, CType: Copy>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            decode_stored_result(
                source,
                store,
                |ok, store| unsafe { R::decode(ok, store) },
                |err, store| unsafe { E::decode(err, store) },
            )
        }
    }
    // TODO: Implement for niche optimized Results
}

pub trait DecodeOwned<'d>: SoftDecodeOwned<'d> {
    unsafe fn decode(source: Self::CType) -> Option<Self>;
}

// TODO: Verify this impl for correctness and Decode as well
impl<'d, R> DecodeOwned<'d> for R
where
    R: SoftDecodeOwned<'d>,
    <R as SoftDecodeOwned<'d>>::Store: Zst + Default + 'd,
{
    unsafe fn decode(source: Self::CType) -> Option<Self> {
        let mut store = R::Store::default();
        // SAFETY: `DecodeFromRef` is only blanket-implemented for zero-sized stores, so extending
        // the borrow of the local store does not extend the lifetime of any backing data.
        let store = unsafe { core::mem::transmute::<&mut R::Store, &'d mut R::Store>(&mut store) };
        unsafe { Self::decode(source, store) }
    }
}

pub(super) fn decode_stored_array<'d, R, C, F, const N: usize>(
    source: [C; N],
    store: &'d mut ArrayStore<R::Store, N>,
    mut decoder: F,
) -> Option<[R; N]>
where
    R: SoftDecodeOwned<'d>,
    F: FnMut(C, &'d mut R::Store) -> Option<R>,
{
    assert_arr_has_non_zero_len::<N>();

    let mut stores = store.0.iter_mut();
    let decoded = source.map(|item| decoder(item, stores.next().unwrap()));

    if decoded.iter().any(Option::is_none) {
        return None;
    }

    Some(decoded.map(|item| unsafe { item.unwrap_unchecked() }))
}

pub(super) fn decode_stored_option_without_niche<'d, R, C: Copy, F>(
    source: COption<C>,
    store: &'d mut R::Store,
    decoder: F,
) -> Option<Option<R>>
where
    R: SoftDecodeOwned<'d>,
    F: FnOnce(C, &'d mut R::Store) -> Option<R>,
{
    match source.try_into().ok()? {
        Some(source) => decoder(source, store).map(Some),
        None => Some(None),
    }
}

pub(super) fn decode_stored_option_with_custom_niche<'d, R, C: PartialEq, F>(
    source: C,
    store: &'d mut R::Store,
    niche: C,
    decoder: F,
) -> Option<Option<R>>
where
    R: SoftDecodeOwned<'d>,
    F: FnOnce(C, &'d mut R::Store) -> Option<R>,
{
    if source == niche {
        return Some(None);
    }

    decoder(source, store).map(Some)
}

pub(super) fn decode_stored_result<'d, R, E, COk: Copy, CErr: Copy, FOk, FErr>(
    source: crate::result::CResult<COk, CErr>,
    store: &'d mut Option<Result<R::Store, E::Store>>,
    ok_decoder: FOk,
    err_decoder: FErr,
) -> Option<Result<R, E>>
where
    R: SoftDecodeOwned<'d>,
    E: SoftDecodeOwned<'d>,
    FOk: FnOnce(COk, &'d mut R::Store) -> Option<R>,
    FErr: FnOnce(CErr, &'d mut E::Store) -> Option<E>,
{
    let value = match TryInto::<Result<COk, CErr>>::try_into(source).ok()? {
        Ok(ok) => {
            let ok_store = store.insert(Ok(Default::default()));
            let ok_store = unsafe { ok_store.as_mut().unwrap_unchecked() };
            Ok(ok_decoder(ok, ok_store)?)
        }
        Err(err) => {
            let err_store = store.insert(Err(Default::default()));
            let err_store = unsafe { err_store.as_mut().unwrap_err_unchecked() };
            Err(err_decoder(err, err_store)?)
        }
    };

    Some(value)
}

impl Store for () {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

#[cfg(feature = "alloc")]
impl<D: Store> Store for Box<[D]> {
    fn sync(self) -> Option<()> {
        for store in self {
            store.sync()?;
        }

        Some(())
    }
}

pub struct RefSizedEncodeStore<R: SoftEncodeOwned> {
    pub(crate) ctype: Option<R::CType>,
    pub(crate) store: R::Store,
}

#[cfg(feature = "alloc")]
pub struct RefDstEncodeStore<R: StdToOwned<Owned: SoftEncodeOwned> + ?Sized> {
    pub(crate) ctype: Option<<R::Owned as ExternC>::CType>,
    pub(crate) store: <R::Owned as SoftEncodeOwned>::Store,
}

pub struct RefMutSizedEncodeStore<'d, R: SoftEncodeOwned> {
    pub(crate) ctype: Option<R::CType>,
    pub(crate) store: R::Store,
    pub(crate) original: Option<&'d mut R>,
}

#[cfg(feature = "alloc")]
pub struct RefMutDstEncodeStore<'d, R: StdToOwned<Owned: SoftEncodeOwned> + ?Sized> {
    pub(crate) ctype: Option<<R::Owned as ExternC>::CType>,
    pub(crate) store: <R::Owned as SoftEncodeOwned>::Store,
    pub(crate) original: Option<&'d mut R>,
}

pub struct RefSizedDecodeStore<R, S> {
    pub(crate) value: Option<R>,
    pub(crate) store: S,
}

#[cfg(feature = "alloc")]
pub struct RefDstDecodeStore<R: StdToOwned + ?Sized, S> {
    pub(crate) value: Option<R::Owned>,
    pub(crate) store: S,
}

pub struct RefMutSizedDecodeStore<R, S> {
    pub(crate) value: Option<R>,
    pub(crate) store: S,
}

#[cfg(feature = "alloc")]
pub struct RefMutDstDecodeStore<R: StdToOwned + ?Sized, S> {
    pub(crate) value: Option<R::Owned>,
    pub(crate) store: S,
}
/// This struct exists only because [arrays don't yet implement Default](https://github.com/rust-lang/rust/issues/61415)
pub struct ArrayStore<D, const N: usize>(pub(crate) [D; N]);

// TODO: derive Default if macro is improved
impl<R: SoftEncodeOwned> Default for RefSizedEncodeStore<R> {
    fn default() -> Self {
        Self {
            ctype: None,
            store: Default::default(),
        }
    }
}

impl<R: SoftEncodeOwned> Store for RefSizedEncodeStore<R> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

#[cfg(feature = "alloc")]
impl<R: StdToOwned<Owned: SoftEncodeOwned> + ?Sized> Default for RefDstEncodeStore<R> {
    fn default() -> Self {
        Self {
            ctype: None,
            store: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: StdToOwned<Owned: SoftEncodeOwned> + ?Sized> Store for RefDstEncodeStore<R> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

impl<'d, R: SoftEncodeOwned> Default for RefMutSizedEncodeStore<'d, R> {
    fn default() -> Self {
        Self {
            ctype: Default::default(),
            store: Default::default(),
            original: Default::default(),
        }
    }
}

impl<'d, R: SoftEncodeOwned + SoftDecodeOwned<'d>> Store for RefMutSizedEncodeStore<'d, R> {
    fn sync(self) -> Option<()> {
        // FIXME:
        //const {
        //    assert!(co3::impls!(R: Decode<'static>), "Not yet implemented");
        //}

        unimplemented!()
        //self.substore.sync()?;
        //if let (Some(ctype), Some(original)) = (self.ctype, self.original) {
        //    let mut decode_store = Default::default();
        //    let store_ref = unsafe {
        //        core::mem::transmute::<
        //            &mut <R as SoftDecodeOwned<'d>>::Store,
        //            &'d mut <R as SoftDecodeOwned<'d>>::Store,
        //        >(&mut decode_store)
        //    };

        //    *original = unsafe { R::decode(ctype, store_ref)? };
        //    decode_store.sync()?;
        //}

        //Some(())
    }
}

#[cfg(feature = "alloc")]
impl<'d, R: StdToOwned<Owned: SoftEncodeOwned> + ?Sized> Default for RefMutDstEncodeStore<'d, R> {
    fn default() -> Self {
        Self {
            ctype: Default::default(),
            store: Default::default(),
            original: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<'d, 'b, R: StdToOwned<Owned: SoftEncodeOwned + SoftDecodeOwned<'b>> + ?Sized> Store
    for RefMutDstEncodeStore<'d, R>
{
    fn sync(self) -> Option<()> {
        // FIXME:
        //const {
        //    assert!(co3::impls!(R: Decode<'static>), "Not yet implemented");
        //}

        unimplemented!()
    }
}

impl<R, S: Default> Default for RefSizedDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
        }
    }
}

impl<R, S: Store> Store for RefSizedDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

#[cfg(feature = "alloc")]
impl<R: StdToOwned + ?Sized, S: Default> Default for RefDstDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: StdToOwned + ?Sized, S: Store> Store for RefDstDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

impl<R, S: Default> Default for RefMutSizedDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
        }
    }
}

impl<R, S: Store> Store for RefMutSizedDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        // FIXME:
        //const {
        //    assert!(co3::impls!(R: Decode<'static>), "Not yet implemented");
        //}

        self.store.sync()
    }
}

#[cfg(feature = "alloc")]
impl<R: StdToOwned + ?Sized, S: Default> Default for RefMutDstDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: StdToOwned + ?Sized, S: Store> Store for RefMutDstDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        // FIXME:
        //const {
        //    assert!(co3::impls!(R: Decode<'static>), "Not yet implemented");
        //}

        self.store.sync()
    }
}

impl<D: Default, const N: usize> Default for ArrayStore<D, N> {
    fn default() -> Self {
        Self(core::array::from_fn(|_| D::default()))
    }
}

impl<D: Store, const N: usize> Store for ArrayStore<D, N> {
    fn sync(self) -> Option<()> {
        for store in self.0 {
            store.sync()?;
        }

        Some(())
    }
}

unsafe impl<D: Zst, const N: usize> Zst for ArrayStore<D, N> {}

impl<T: Store> Store for Option<T> {
    fn sync(self) -> Option<()> {
        match self {
            Some(store) => store.sync(),
            None => Some(()),
        }
    }
}

impl<T: Store, E: Store> Store for Result<T, E> {
    fn sync(self) -> Option<()> {
        match self {
            Ok(store) => store.sync(),
            Err(store) => store.sync(),
        }
    }
}
