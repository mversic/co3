#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};
use core::mem::ManuallyDrop;

use super::*;
use crate::dst::Sized_;

trait CloneFromWrapped<R> {
    fn clone_from_wrapped(self) -> R;
}

impl<R: Clone> CloneFromWrapped<R> for ManuallyDrop<R> {
    fn clone_from_wrapped(self) -> R {
        ManuallyDrop::into_inner(self.clone())
    }
}

#[cfg(feature = "alloc")]
impl<R, W: CloneFromWrapped<R>> CloneFromWrapped<Box<R>> for Box<W> {
    fn clone_from_wrapped(self) -> Box<R> {
        let wrapped = *self;
        Box::new(CloneFromWrapped::clone_from_wrapped(wrapped))
    }
}

#[cfg(feature = "alloc")]
impl<R, W: CloneFromWrapped<R>> CloneFromWrapped<Box<[R]>> for Box<[W]> {
    fn clone_from_wrapped(self) -> Box<[R]> {
        self.into_iter()
            .map(CloneFromWrapped::clone_from_wrapped)
            .collect()
    }
}

#[cfg(feature = "alloc")]
impl<R, W: CloneFromWrapped<R>> CloneFromWrapped<Vec<R>> for Vec<W> {
    fn clone_from_wrapped(self) -> Vec<R> {
        self.into_iter()
            .map(CloneFromWrapped::clone_from_wrapped)
            .collect()
    }
}

impl<R, W: CloneFromWrapped<R>, const N: usize> CloneFromWrapped<[R; N]> for [W; N] {
    fn clone_from_wrapped(self) -> [R; N] {
        self.map(CloneFromWrapped::clone_from_wrapped)
    }
}

disjoint_impls! {
    // FIXME: I'm not happy with the name anymore since it's implemented for all IR types
    /// [`DecodeWithStore`] helper for [`Cloned`] types.
    ///
    /// Implementations of [`DecodeWithStore`] for `&T`/`&mut T` where `T: Cloned` depend on decoding `T`,
    /// but decoding `T` may include ownership transfer in which case it must be decoded to
    /// [`ManuallyDrop<T>`]
    ///
    /// - Non-owning decode: use regular decode path.
    /// - Ownership-transferring decode: decode into a wrapper (for example [`ManuallyDrop`]),
    ///   then clone.
    ///
    /// [`decode_cloned`](Self::decode_cloned) is the method cloned containers/tuples call
    /// recursively for their elements.
    pub trait DecodeCloned<'d, const UNSAFE_OPTIMIZATIONS: bool = false>:
        DecodeWithStore<'d, false>
    {
        /// Perform the conversion from [`Self::CType`] into [`Self`]
        ///
        /// # Safety
        ///
        /// - check [`DecodeWithStore`]
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { DecodeWithStore::decode(source, store) }
        }
    }

    impl<'d, R> DecodeCloned<'d, false> for R
    where
        Self: ReprFamily<Kind = Robust> + DecodeWithStore<'d, false>,
    {}
    #[cfg(feature = "alloc")]
    impl<'d, R: Clone + 'd> DecodeCloned<'d, false> for R
    where
        Self: ReprFamily<Kind = Opaque> + DecodeWithStore<'d, false>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { <ManuallyDrop<R> as DecodeWithStore>::decode(source, store) }
                .map(CloneFromWrapped::clone_from_wrapped)
        }
    }
    // FIXME: This is not correct for Box<Opaque>!
    impl<'d, R: CheckedTransmute<Target: DecodeCloned<'d, false>>> DecodeCloned<'d, false> for R
    where
        Self: ReprFamily<Kind = Transmuted>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { <R::Target>::decode_cloned(source, store) }
                .and_then(transmute_from_target)
        }
    }

    impl<'d, R: ?Sized> DecodeCloned<'d, false> for &'d R
    where
        Self: ReprFamily<Kind = &'d Robust> + DecodeWithStore<'d>
    {
    }
    //impl<'a, R: ?Sized> DecodeCloned<'a, false> for &'a R where
    //    Self: ReprFamily<Kind = &'a Opaque> + Decode<'a>
    //{
    //}
    impl<'a, R: ?Sized> DecodeCloned<'a, false> for &'a R
    where
        Self: ReprFamily<Kind = &'a Transmuted> + DecodeWithStore<'a>
    {
    }
    impl<'d, R, S: Cloned + 'd> DecodeCloned<'d, false> for &'d R
    where
        Self: ReprFamily<Kind = &'d S> + DecodeWithStore<'d>,
        R: DstFamily<Kind = Sized_>,
    {
    }
    impl<'a, R: ?Sized, S: Cloned + ?Sized + 'a> DecodeCloned<'a, false> for &'a R
    where
        Self: ReprFamily<Kind = &'a S> + DecodeWithStore<'a>,
        R: DstFamily<Kind = SliceLike>,
    {
    }

    impl<'d, R: ?Sized> DecodeCloned<'d, false> for &'d mut R
    where
        Self: ReprFamily<Kind = &'d mut Robust> + DecodeWithStore<'d>
    {
    }
    //impl<'a, R: ?Sized> DecodeCloned<'a, false> for &'a mut R where
    //    Self: ReprFamily<Kind = &'a mut Opaque> + Decode<'a>
    //{
    //}
    impl<'a, R: ?Sized> DecodeCloned<'a, false> for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut Transmuted> + DecodeWithStore<'a>
    {
    }
    impl<'a, R, S: Cloned + 'a> DecodeCloned<'a, false> for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut S> + DecodeWithStore<'a>,
        R: DstFamily<Kind = SliceLike>,
    {
    }
    impl<'d, R: ?Sized, S: Cloned + ?Sized + 'd> DecodeCloned<'d, false> for &'d mut R
    where
        Self: ReprFamily<Kind = &'d mut S> + DecodeWithStore<'d>,
        R: DstFamily<Kind = Sized_>,
    {
    }

    //#[cfg(feature = "alloc")]
    //impl<R: ?Sized> DecodeCloned<'_, false> for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<Robust>>,
    //{
    //    #[inline(always)]
    //    unsafe fn decode_cloned<'itm: 'd>(
    //        source: Self::CType,
    //        store: &'itm mut Self::Store,
    //    ) -> Option<Self> {
    //        unsafe { ManuallyDrop::<Self>::decode(source, store) }
    //            .map(CloneFromWrapped::clone_from_wrapped)
    //    }
    //}
    //#[cfg(feature = "alloc")]
    //impl<'d, R: ?Sized> DecodeCloned<'d, false> for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<Opaque>>,
    //{
    //    #[inline(always)]
    //    unsafe fn decode_cloned<'itm: 'd>(
    //        source: Self::CType,
    //        store: &'itm mut Self::Store,
    //    ) -> Option<Self> {
    //        unsafe { Box::<[ManuallyDrop<R>]>::decode(source, store) }
    //            .map(CloneFromWrapped::clone_from_wrapped)
    //    }
    //}
    #[cfg(feature = "alloc")]
    impl<'d, R: ?Sized + SliceDst + CheckedTransmute<Target: SliceDst>> DecodeCloned<'d, false> for Box<R>
    where
        Box<<R as CheckedTransmute>::Target>: DecodeCloned<'d, false>,
        Self: ReprFamily<Kind = Box<Transmuted>>,
        R: DstFamily<Kind = SliceLike>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { <Box<R::Target>>::decode_cloned(source, store) }
                .and_then(transmute_from_target_boxed_dst)
        }
    }
    //#[cfg(feature = "alloc")]
    //impl<'d, R, S: Cloned> DecodeCloned<'d, false> for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<S>>,
    //    R: SizeFamily<Kind = Sized_>,
    //{
    //    #[inline(always)]
    //    unsafe fn decode_cloned<'itm: 'd>(
    //        source: Self::CType,
    //        store: &'itm mut Self::Store,
    //    ) -> Option<Self> {
    //        unsafe { decode_cloned_box_ptr(source, store, |item, substore| R::decode_cloned(item, substore)) }
    //    }
    //}
    //#[cfg(feature = "alloc")]
    //impl<'d, R: ?Sized, S: Cloned + ?Sized> DecodeCloned<'d, false> for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<S>>,
    //    R: SizeFamily<Kind = UnSized>,
    //{
    //    #[inline(always)]
    //    unsafe fn decode_cloned<'itm: 'd>(
    //        source: Self::CType,
    //        store: &'itm mut Self::Store,
    //    ) -> Option<Self> {
    //        unsafe {
    //            decode_cloned_boxed_slice(source, store, |item, substore| {
    //                R::decode_cloned(item, substore)
    //            })
    //        }
    //    }
    //}

    #[cfg(feature = "alloc")]
    impl<'d, R, S> DecodeCloned<'d, false> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>>,
        Box<[R]>: DecodeWithStore<'d>
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unimplemented!()
            //unsafe {
            //    decode_cloned_vec(source, store, |item, substore| {
            //        R::decode_cloned(item, substore)
            //    })
            //}
        }
    }

    impl<'d, R: DecodeCloned<'d, false>, S: Cloned, const N: usize> DecodeCloned<'d, false> for [R; N]
    where
        Self: ReprFamily<Kind = S>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            decode_cloned_array(source, store, |item, substore| {
                unsafe { R::decode_cloned(item, substore) }
            })
        }
    }

    impl<'d, R: DecodeCloned<'d, false>> DecodeCloned<'d, false> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithoutNiche>>
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            decode_cloned_option_without_niche(source, store, |source, store| unsafe {
                R::decode_cloned(source, store)
            })
        }
    }

    impl<'d, R: Niche + DecodeCloned<'d, false>> DecodeCloned<'d, false> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithCustomNiche>>,
        <R as ExternC>::CType: PartialEq,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            decode_cloned_option_with_custom_niche(source, store, R::NICHE_VALUE, |source, store| unsafe {
                R::decode_cloned(source, store)
            })
        }
    }
}

#[cfg(feature = "alloc")]
pub(super) unsafe fn decode_cloned_box_ptr<'d, R, C: ReprC, S, F>(
    source: *const C,
    store: &'d mut S,
    decoder: F,
) -> Option<Box<R>>
where
    F: FnOnce(C, &'d mut S) -> Option<R>,
{
    if source.is_null() {
        return None;
    }

    let source = unsafe { source.read() };
    Some(Box::new(decoder(source, store)?))
}

#[cfg(feature = "alloc")]
pub(super) unsafe fn decode_cloned_boxed_slice<'d, R, C: ReprC, S: Default, F>(
    source: CSlice<C>,
    store: &'d mut DecodeStoreSlice<S>,
    mut decoder: F,
) -> Option<Box<[R]>>
where
    F: FnMut(C, &'d mut S) -> Option<R>,
{
    let slice = unsafe { source.into_rust() }?;

    let store = store.0.insert(
        core::iter::repeat_with(Default::default)
            .take(slice.len())
            .collect(),
    );

    slice
        .iter()
        .zip(&mut *store)
        .map(|(&item, substore)| decoder(item, substore))
        .collect()
}

#[cfg(feature = "alloc")]
pub(super) unsafe fn decode_cloned_vec<'d, R, C: ReprC, S: Default, F>(
    source: CSlice<C>,
    store: &'d mut DecodeStoreSlice<S>,
    mut decoder: F,
) -> Option<Vec<R>>
where
    F: FnMut(C, &'d mut S) -> Option<R>,
{
    let slice = unsafe { source.into_rust() }?;

    let store = store.0.insert(
        core::iter::repeat_with(Default::default)
            .take(slice.len())
            .collect(),
    );

    slice
        .iter()
        .zip(&mut *store)
        .map(|(&item, substore)| decoder(item, substore))
        .collect()
}

pub(super) fn decode_cloned_array<'d, R, C: ReprC, S: Default, F, const N: usize>(
    source: [C; N],
    store: &'d mut ArraySyncStore<S, N>,
    mut decoder: F,
) -> Option<[R; N]>
where
    F: FnMut(C, &'d mut S) -> Option<R>,
{
    assert_arr_has_non_zero_len::<N>();
    let mut stores = store.0.iter_mut();

    let decoded = source.map(|item| decoder(item, stores.next().unwrap()));

    if decoded.iter().any(Option::is_none) {
        return None;
    }

    Some(decoded.map(|item| unsafe { item.unwrap_unchecked() }))
}

pub(super) fn decode_cloned_option_without_niche<'d, R, C: ReprC, S, F>(
    source: COption<C>,
    store: &'d mut S,
    decoder: F,
) -> Option<Option<R>>
where
    F: FnOnce(C, &'d mut S) -> Option<R>,
{
    match source.try_into().ok()? {
        Some(source) => decoder(source, store).map(Some),
        None => Some(None),
    }
}

pub(super) fn decode_cloned_option_with_custom_niche<'d, R, C: PartialEq, S, F>(
    source: C,
    store: &'d mut S,
    niche: C,
    decoder: F,
) -> Option<Option<R>>
where
    F: FnOnce(C, &'d mut S) -> Option<R>,
{
    if source == niche {
        return Some(None);
    }

    decoder(source, store).map(Some)
}
