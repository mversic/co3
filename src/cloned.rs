#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};
use core::mem::ManuallyDrop;

use super::*;

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
    /// [`Decode`] helper for [`Cloned`] types.
    ///
    /// Implementations of [`Decode`] for `&T`/`&mut T` where `T: Cloned` depend on decoding `T`,
    /// but decoding `T` may include ownership transfer in which case it must be decoded to
    /// [`ManuallyDrop<T>`]
    ///
    /// - Non-owning decode: use regular decode path.
    /// - Ownership-transferring decode: decode into a wrapper (for example [`ManuallyDrop`]),
    ///   then clone.
    ///
    /// [`decode_cloned`](Self::decode_cloned) is the method cloned containers/tuples call
    /// recursively for their elements.
    pub trait DecodeCloned<'d>: Decode<'d> {
        /// Perform the conversion from [`Self::CType`] into [`Self`]
        ///
        /// # Safety
        ///
        /// - check [`Decode`]
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { Decode::decode(source, store) }
        }
    }

    impl<'d, R> DecodeCloned<'d> for R
    where
        Self: ReprFamily<Kind = Robust> + Decode<'d>,
    {}
    #[cfg(feature = "alloc")]
    impl<'d, R: Clone + 'd> DecodeCloned<'d> for R
    where
        Self: ReprFamily<Kind = Opaque>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { ManuallyDrop::<R>::decode(source, store) }
                .map(CloneFromWrapped::clone_from_wrapped)
        }
    }
    impl<'d, R: CheckedTransmute<Target: DecodeCloned<'d>>> DecodeCloned<'d> for R
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

    #[cfg(feature = "unstable-refs")]
    impl<'d, R, S: Cloned> DecodeCloned<'d> for &'d R
    where
        Self: ReprFamily<Kind = &'d S> + Decode<'d>,
    {
    }

    #[cfg(feature = "unstable-refs")]
    impl<'d, R, S: Cloned> DecodeCloned<'d> for &'d mut R
    where
        Self: ReprFamily<Kind = &'d mut S> + Decode<'d>,
    {
    }

    #[cfg(feature = "alloc")]
    impl<'d, R: DecodeCloned<'d>, S: Cloned> DecodeCloned<'d> for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { decode_cloned_box_ptr(source, store, |item, substore| R::decode_cloned(item, substore)) }
        }
    }

    impl<'slice, R: ReprC> DecodeCloned<'slice> for &'slice [R] where
        Self: ReprFamily<Kind = &'slice [Robust]>
    {
    }
    #[cfg(all(feature = "alloc", feature = "unstable-refs"))]
    impl<'slice, R: Clone> DecodeCloned<'slice> for &'slice [R] where
        Self: ReprFamily<Kind = &'slice [Opaque]>
    {
    }
    impl<'slice, R: CheckedTransmute<Target: Sized + 'slice>> DecodeCloned<'slice> for &'slice [R]
    where
        &'slice [<R as CheckedTransmute>::Target]: Decode<'slice>,
        Self: ReprFamily<Kind = &'slice [Transmuted]>,
    {
    }
    #[cfg(feature = "unstable-refs")]
    impl<'slice, R, S: Cloned> DecodeCloned<'slice> for &'slice [R]
    where
        Self: ReprFamily<Kind = &'slice [S]> + Decode<'slice>,
    {
    }

    #[cfg(all(feature = "alloc", feature = "unstable-refs"))]
    impl<'slice, R: Clone> DecodeCloned<'slice> for &'slice mut [R] where
        Self: ReprFamily<Kind = &'slice mut [Opaque]>
    {
    }
    impl<'slice, R> DecodeCloned<'slice> for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [Transmuted]> + Decode<'slice>
    {
    }
    #[cfg(feature = "unstable-refs")]
    impl<'slice, R, S: Cloned> DecodeCloned<'slice> for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [S]> + Decode<'slice>,
    {
    }

    #[cfg(feature = "alloc")]
    impl<'d, R: ReprC + 'd> DecodeCloned<'d> for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Robust]>>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { ManuallyDrop::<Self>::decode(source, store) }
                .map(CloneFromWrapped::clone_from_wrapped)
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: Clone + 'd> DecodeCloned<'d> for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Opaque]>>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { Box::<[ManuallyDrop<R>]>::decode(source, store) }
                .map(CloneFromWrapped::clone_from_wrapped)
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: CheckedTransmute<Target: Sized>> DecodeCloned<'d> for Box<[R]>
    where
        Box<[<R as CheckedTransmute>::Target]>: DecodeCloned<'d>,
        Self: ReprFamily<Kind = Box<[Transmuted]>>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { <Box<[R::Target]>>::decode_cloned(source, store) }
                .and_then(transmute_from_target_boxed_slice)
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: DecodeCloned<'d>, S: Cloned> DecodeCloned<'d> for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[S]>>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe {
                decode_cloned_boxed_slice(source, store, |item, substore| {
                    R::decode_cloned(item, substore)
                })
            }
        }
    }

    #[cfg(feature = "alloc")]
    impl<'d, R: CheckedTransmute<Target: Sized>> DecodeCloned<'d> for Vec<R>
    where
        Vec<<R as CheckedTransmute>::Target>: DecodeCloned<'d>,
        Self: ReprFamily<Kind = Vec<Transmuted>>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { <Vec<R::Target>>::decode_cloned(source, store) }
                .and_then(transmute_from_target_vec)
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprC + 'd> DecodeCloned<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Robust>>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { ManuallyDrop::<Self>::decode(source, store) }
                .map(CloneFromWrapped::clone_from_wrapped)
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: Clone + 'd> DecodeCloned<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Opaque>>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { Vec::<ManuallyDrop<R>>::decode(source, store) }
                .map(CloneFromWrapped::clone_from_wrapped)
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: DecodeCloned<'d>, S: Cloned> DecodeCloned<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe {
                decode_cloned_vec(source, store, |item, substore| {
                    R::decode_cloned(item, substore)
                })
            }
        }
    }

    #[cfg(feature = "alloc")]
    impl<'d, R: Clone + 'd, const N: usize> DecodeCloned<'d> for [R; N]
    where
        Self: ReprFamily<Kind = [Opaque; N]>,
    {
        #[inline(always)]
        unsafe fn decode_cloned<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { <[ManuallyDrop<R>; N]>::decode(source, store) }
                .map(CloneFromWrapped::clone_from_wrapped)
        }
    }
    impl<'d, R: DecodeCloned<'d>, S: Cloned, const N: usize> DecodeCloned<'d> for [R; N]
    where
        Self: ReprFamily<Kind = [S; N]>,
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

    impl<'d, R: DecodeCloned<'d>> DecodeCloned<'d> for Option<R>
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

    impl<'d, R: Niche<CType: PartialEq> + DecodeCloned<'d>> DecodeCloned<'d> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithCustomNiche>>
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
    source: CBox<C>,
    store: &'d mut S,
    decoder: F,
) -> Option<Box<R>>
where
    F: FnOnce(C, &'d mut S) -> Option<R>,
{
    let source = unsafe { source.into_rust() }?;
    Some(Box::new(decoder(*source, store)?))
}

#[cfg(feature = "alloc")]
pub(super) unsafe fn decode_cloned_boxed_slice<'d, R, C: ReprC, S: Default, F>(
    source: CBoxedSlice<C>,
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
        .into_iter()
        .zip(&mut *store)
        .map(|(item, substore)| decoder(item, substore))
        .collect()
}

#[cfg(feature = "alloc")]
pub(super) unsafe fn decode_cloned_vec<'d, R, C: ReprC, S: Default, F>(
    source: CVec<C>,
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
        .into_iter()
        .zip(&mut *store)
        .map(|(item, substore)| decoder(item, substore))
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
