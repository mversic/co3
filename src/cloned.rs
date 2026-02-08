use core::mem::ManuallyDrop;

use super::*;

pub trait CloneFromWrapped<R> {
    fn clone_from_wrapped(self) -> R;
}

impl<R: Clone> CloneFromWrapped<R> for ManuallyDrop<R> {
    fn clone_from_wrapped(self) -> R {
        ManuallyDrop::into_inner(self.clone())
    }
}

impl<'a, R> CloneFromWrapped<&'a R> for &'a R {
    fn clone_from_wrapped(self) -> &'a R {
        self
    }
}

impl<'a, R> CloneFromWrapped<&'a mut R> for &'a mut R {
    fn clone_from_wrapped(self) -> &'a mut R {
        self
    }
}

impl<'a, R> CloneFromWrapped<&'a [R]> for &'a [R] {
    fn clone_from_wrapped(self) -> &'a [R] {
        self
    }
}

impl<'a, R> CloneFromWrapped<&'a mut [R]> for &'a mut [R] {
    fn clone_from_wrapped(self) -> &'a mut [R] {
        self
    }
}

impl<R, W: CloneFromWrapped<R>> CloneFromWrapped<Box<R>> for Box<W> {
    fn clone_from_wrapped(self) -> Box<R> {
        let wrapped = *self;
        Box::new(CloneFromWrapped::clone_from_wrapped(wrapped))
    }
}

impl<R, W: CloneFromWrapped<R>> CloneFromWrapped<Box<[R]>> for Box<[W]> {
    fn clone_from_wrapped(self) -> Box<[R]> {
        self.into_iter()
            .map(CloneFromWrapped::clone_from_wrapped)
            .collect()
    }
}

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

//impl<R> CloneFromWrapped<Option<R>> for Option<ManuallyDrop<R>> {
//    fn clone_from_wrapped(self) -> Option<R> {
//        self.map(ManuallyDrop::into_inner)
//    }
//}

disjoint_impls! {
    /// Controls how decoded values are wrapped for cloned references.
    ///
    /// The wrapper type determines drop behavior (e.g. `ManuallyDrop<Self>` to prevent
    /// freeing FFI-owned allocations).
    pub trait DecodeCloneWrapper<'d>: ReprFamily<Kind: Cloned> + Decode<'d> {
        #[inline(always)]
        unsafe fn decode_wrapped<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { Decode::decode(source, store) }
        }
    }

    #[cfg(feature = "cloned_refs")]
    impl<'d, R: DecodeCloneWrapper<'d>, S: Cloned> DecodeCloneWrapper<'d> for &'d R
    where
        Self: ReprFamily<Kind = &'d S>,
    {
    }

    #[cfg(feature = "cloned_refs")]
    impl<'d, R: DecodeCloneWrapper<'d> + Encode + NonLocal, S: Cloned> DecodeCloneWrapper<'d> for &'d mut R
    where
        Self: ReprFamily<Kind = &'d mut S>,
    {
    }

    #[cfg(feature = "owned_types")]
    impl<'d, R: DecodeCloneWrapper<'d>, S: Cloned> DecodeCloneWrapper<'d> for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
    {
        #[inline(always)]
        unsafe fn decode_wrapped<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { decode_cloned_box_ptr(source, store, |item, substore| R::decode_wrapped(item, substore)) }
        }
    }

    impl<'slice, R: CheckedTransmute> DecodeCloneWrapper<'slice> for &'slice [R]
    where
        &'slice [<R as CheckedTransmute>::Target]: Decode<'slice>,
        Self: ReprFamily<Kind = &'slice [Transmuted]>,
    {
    }
    impl<'slice, R: ReprC> DecodeCloneWrapper<'slice> for &'slice [R] where
        Self: ReprFamily<Kind = &'slice [Robust]>
    {
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R: Clone> DecodeCloneWrapper<'slice> for &'slice [R] where
        Self: ReprFamily<Kind = &'slice [Opaque]>
    {
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R: DecodeCloneWrapper<'slice>, S: Cloned> DecodeCloneWrapper<'slice> for &'slice [R]
    where
        Self: ReprFamily<Kind = &'slice [S]>,
    {
    }

    impl<'slice, R: CheckedTransmute> DecodeCloneWrapper<'slice> for &'slice mut [R]
    where
        &'slice mut [<R as CheckedTransmute>::Target]: Decode<'slice>,
        Self: ReprFamily<Kind = &'slice mut [Transmuted]>
    {
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R: Clone> DecodeCloneWrapper<'slice> for &'slice mut [R] where
        Self: ReprFamily<Kind = &'slice mut [Opaque]>
    {
    }
    #[cfg(feature = "cloned_refs")]
    impl<'slice, R: DecodeCloneWrapper<'slice> + Encode + NonLocal, S: Cloned> DecodeCloneWrapper<'slice> for &'slice mut [R]
    where
        Self: ReprFamily<Kind = &'slice mut [S]>,
    {
    }

    //#[cfg(feature = "owned_as_ref")]
    //impl<'d, R: CheckedTransmute> DecodeCloneWrapper<'d> for Box<[R]>
    //where
    //    Box<[<R as CheckedTransmute>::Target]>: Decode<'d>,
    //    Self: ReprFamily<Kind = Box<[Transmuted]>>,
    //{
    //    #[inline(always)]
    //    unsafe fn decode_wrapped<'itm: 'd>(
    //        source: Self::CType,
    //        store: &'itm mut Self::Store,
    //    ) -> Option<Self> {
    //        unimplemented!()
    //    }
    //}
    #[cfg(feature = "owned_types")]
    impl<'d, R: ReprC + 'd> DecodeCloneWrapper<'d> for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Robust]>>,
    {
        #[inline(always)]
        #[cfg(not(feature = "owned_as_ref"))]
        unsafe fn decode_wrapped<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { ManuallyDrop::<Self>::decode(source, store) }
                .map(CloneFromWrapped::clone_from_wrapped)
        }
    }
    #[cfg(feature = "owned_types")]
    impl<'d, R: Clone + 'd> DecodeCloneWrapper<'d> for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Opaque]>>,
    {
        #[inline(always)]
        unsafe fn decode_wrapped<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { Box::<[ManuallyDrop<R>]>::decode(source, store) }
                .map(CloneFromWrapped::clone_from_wrapped)
        }
    }
    #[cfg(feature = "owned_types")]
    impl<'d, R: DecodeCloneWrapper<'d>, S: Cloned> DecodeCloneWrapper<'d> for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[S]>>,
    {
        #[inline(always)]
        unsafe fn decode_wrapped<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe {
                decode_cloned_collection(source, store, |item, substore| {
                    R::decode_wrapped(item, substore)
                })
            }
        }
    }

    //#[cfg(feature = "owned_as_ref")]
    //impl<'d, R: CheckedTransmute> DecodeCloneWrapper<'d> for Vec<R>
    //where
    //    Vec<<R as CheckedTransmute>::Target>: Decode<'d>,
    //    Self: ReprFamily<Kind = Vec<Transmuted>>,
    //{
    //    #[inline(always)]
    //    unsafe fn decode_wrapped<'itm: 'd>(
    //        source: Self::CType,
    //        store: &'itm mut Self::Store,
    //    ) -> Option<Self> {
    //        unimplemented!()
    //    }
    //}
    #[cfg(feature = "owned_types")]
    impl<'d, R: ReprC + 'd> DecodeCloneWrapper<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Robust>>,
    {
        #[inline(always)]
        #[cfg(not(feature = "owned_as_ref"))]
        unsafe fn decode_wrapped<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { ManuallyDrop::<Self>::decode(source, store) }
                .map(CloneFromWrapped::clone_from_wrapped(wrapped))
        }
    }
    #[cfg(feature = "owned_types")]
    impl<'d, R: Clone + 'd> DecodeCloneWrapper<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Opaque>>,
    {
        #[inline(always)]
        unsafe fn decode_wrapped<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { Vec::<ManuallyDrop<R>>::decode(source, store) }
                .map(CloneFromWrapped::clone_from_wrapped)
        }
    }
    #[cfg(feature = "owned_types")]
    impl<'d, R: DecodeCloneWrapper<'d>, S: Cloned> DecodeCloneWrapper<'d> for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>>,
    {
        #[inline(always)]
        unsafe fn decode_wrapped<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe {
                decode_cloned_collection(source, store, |item, substore| {
                    R::decode_wrapped(item, substore)
                })
            }
        }
    }

    impl<'d, R: Clone + 'd, const N: usize> DecodeCloneWrapper<'d> for [R; N]
    where
        Self: ReprFamily<Kind = [Opaque; N]>,
    {
        #[inline(always)]
        unsafe fn decode_wrapped<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { <[ManuallyDrop<R>; N]>::decode(source, store) }
                .map(CloneFromWrapped::clone_from_wrapped)
        }
    }
    impl<'d, R: DecodeCloneWrapper<'d>, S: Cloned, const N: usize> DecodeCloneWrapper<'d> for [R; N]
    where
        Self: ReprFamily<Kind = [S; N]>,
    {
        #[inline(always)]
        unsafe fn decode_wrapped<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe {
                decode_cloned_array(source, store, |item, substore| {
                    R::decode_wrapped(item, substore)
                })
            }
        }
    }

    //impl<'d, R: ReprFamily<Kind = Transmuted> + 'd> DecodeCloneWrapper<'d> for Option<R>
    //where
    //    Self: ReprFamily<Kind = Option<WithoutNiche>>,
    //{
    //}
    impl<'d, R: ReprFamily<Kind = Robust> + ReprC + 'd> DecodeCloneWrapper<'d> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithoutNiche>>,
    {
    }
    impl<'d, R: ReprFamily<Kind: Cloned> + 'd> DecodeCloneWrapper<'d> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithoutNiche>> + Decode<'d>,
    {
        #[inline(always)]
        unsafe fn decode_wrapped<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unimplemented!()
        }
    }

    //impl<'d, R: ReprFamily<Kind = Transmuted> + 'd> DecodeCloneWrapper<'d> for Option<R>
    //where
    //    Self: ReprFamily<Kind = Option<WithCustomNiche>> + Decode<'d>,
    //{
    //    #[inline(always)]
    //    unsafe fn decode_wrapped<'itm: 'd>(
    //        source: Self::CType,
    //        store: &'itm mut Self::Store,
    //    ) -> Option<Self> {
    //        unimplemented!()
    //    }
    //}
    impl<'d, R: ReprFamily<Kind = Opaque>> DecodeCloneWrapper<'d> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithCustomNiche>> + Decode<'d>,
    {
        #[inline(always)]
        unsafe fn decode_wrapped<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unimplemented!()
        }
    }
    impl<'d, R: ReprFamily<Kind: Cloned>> DecodeCloneWrapper<'d> for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithCustomNiche>> + Decode<'d>,
    {
        #[inline(always)]
        unsafe fn decode_wrapped<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unimplemented!()
        }
    }
    // TODO: Not sure this situation is possible to have
    //impl<'d, R: ReprFamily<Kind: Cloned> + NicheFamily<Kind = WithStableNiche>> DecodeCloneWrapper<'d> for Option<R>
    //where
    //    Self: ReprFamily<Kind = Option<WithCustomNiche>> + Decode<'d>,
    //{
    //    #[inline(always)]
    //    unsafe fn decode_wrapped<'itm: 'd>(
    //        source: Self::CType,
    //        store: &'itm mut Self::Store,
    //    ) -> Option<Self> {
    //        unimplemented!()
    //    }
    //}
}

pub(super) unsafe fn decode_cloned_box_ptr<'d, R, C: ReprC, S, F>(
    source: *mut C,
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

pub(super) unsafe fn decode_cloned_collection<'d, R, C: ReprC, S: Default, F, Out>(
    source: CSliceMut<C>,
    store: &'d mut DecodeStoreSlice<S>,
    mut decoder: F,
) -> Option<Out>
where
    F: FnMut(C, &'d mut S) -> Option<R>,
    Out: FromIterator<R>,
{
    let slice = unsafe { source.into_rust() }?;

    let store = store.0.insert(
        core::iter::repeat_with(Default::default)
            .take(slice.len())
            .collect(),
    );

    slice
        .iter()
        .copied()
        .zip(&mut *store)
        .map(|(item, substore)| decoder(item, substore))
        .collect()
}

pub(super) unsafe fn decode_cloned_array<'d, R, C: ReprC, S: Default, F, const N: usize>(
    source: [C; N],
    store: &'d mut ArraySyncStore<S, N>,
    mut decoder: F,
) -> Option<[R; N]>
where
    F: FnMut(C, &'d mut S) -> Option<R>,
{
    assert_arr_has_non_zero_len::<N>();

    let store = store
        .0
        .insert(core::iter::repeat_with(Default::default).take(N).collect());

    let vec = source
        .into_iter()
        .zip(store)
        .map(|(item, substore)| decoder(item, substore))
        .collect::<Option<Vec<_>>>()?;

    Some(unsafe { vec.try_into().unwrap_unchecked() })
}
