#[cfg(feature = "alloc")]
use alloc::{borrow::ToOwned, boxed::Box, vec::Vec};
#[cfg(feature = "alloc")]
use core::marker::PhantomData;

#[cfg(feature = "alloc")]
use crate::{
    CType,
    slice::{CWideRef, CWideRefMut},
    stored::AssignFromOwned,
    wide::{Wide, WideHeader},
};
use crate::{
    ReprC,
    reference::{CRef, CRefMut},
    stored::{DecodeMove, EmptyStore, EncodeMove, Store, decode_move, encode_move},
};

impl Store for () {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

#[cfg(feature = "alloc")]
impl<D: Store> Store for Box<D> {
    fn sync(self) -> Option<()> {
        (*self).sync()
    }
}

#[cfg(feature = "alloc")]
impl<D: Store> Store for Box<[D]> {
    fn sync(self) -> Option<()> {
        let mut is_valid = true;

        for store in self {
            if store.sync().is_none() {
                is_valid = false;
            }
        }

        is_valid.then_some(())
    }
}

pub struct RefSizedEncodeStore<R: ReprC<CType: Sized>, S> {
    ctype: Option<R::CType>,
    store: S,
}

pub struct RefSizedDecodeStore<R, S> {
    value: Option<R>,
    store: S,
}

impl<R: ReprC<CType: Sized>, S: Store> RefSizedEncodeStore<R, S> {
    pub fn encode(&mut self, encode: impl FnOnce(&mut S) -> R::CType) -> CRef<R> {
        self.ctype = None;
        let value = encode(&mut self.store);
        CRef::from_ref(self.ctype.insert(value))
    }
}

impl<R, S> RefSizedDecodeStore<R, S> {
    pub fn decode<'a>(&'a mut self, decode: impl FnOnce(&'a mut S) -> Option<R>) -> Option<&'a R> {
        self.value = None;
        let value = decode(&mut self.store)?;
        Some(self.value.insert(value))
    }
}

impl<R: ReprC<CType: Sized>, S: Default> Default for RefSizedEncodeStore<R, S> {
    fn default() -> Self {
        Self {
            ctype: None,
            store: Default::default(),
        }
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

impl<R: ReprC<CType: Sized>, S: Store> Store for RefSizedEncodeStore<R, S> {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

impl<R, S: Store> Store for RefSizedDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

pub struct RefMutSizedEncodeStore<'d, R: DecodeMove<'d, Store: EmptyStore + 'd>, S: Store + Default>
{
    ctype: Option<R::CType>,
    store: S,
    original: Option<&'d mut R>,
}

type DecodeWriteback<R, P> = (P, fn(R, P) -> Option<()>, bool);

pub struct RefMutSizedDecodeStore<R: ReprC, S, P = *mut <R as ReprC>::CType> {
    value: Option<R>,
    store: S,
    writeback: Option<DecodeWriteback<R, P>>,
}

impl<'d, R: DecodeMove<'d, Store: EmptyStore + 'd>, S: Store + Default>
    RefMutSizedEncodeStore<'d, R, S>
{
    #[doc(hidden)]
    pub fn encode(
        &mut self,
        original: &'d mut R,
        encode: impl FnOnce(&mut R, &mut S) -> R::CType,
    ) -> CRefMut<R, true> {
        drop(self.finish());
        let ctype = encode(&mut *original, &mut self.store);
        self.original = Some(original);
        CRefMut::from_mut(self.ctype.insert(ctype))
    }

    fn finish(&mut self) -> Option<(&'d mut R, R)> {
        let ctype = self.ctype.take()?;
        let value = unsafe { decode_move(ctype) };
        let synced = core::mem::take(&mut self.store).sync();
        let original = self.original.take()?;
        synced?;
        Some((original, value?))
    }
}

impl<R: EncodeMove<Store: EmptyStore>, S: Store + Default> RefMutSizedDecodeStore<R, S> {
    /// The source must remain valid and exclusively accessible until synchronization or reuse.
    #[doc(hidden)]
    pub unsafe fn decode<'a>(
        &'a mut self,
        source: *mut R::CType,
        build: impl FnOnce(&'a mut S) -> Option<R>,
    ) -> Option<&'a mut R> {
        if source.is_null() {
            return None;
        }
        unsafe {
            self.decode_with_order(
                |fields| build(fields).map(|value| (value, source)),
                |value, source| {
                    source.write(encode_move(value));
                    Some(())
                },
                true,
            )
        }
    }
}

impl<R: ReprC, S: Store + Default, P> RefMutSizedDecodeStore<R, S, P> {
    /// Retain a reconstructed value and type-specific writeback state.
    ///
    /// # Safety
    /// State must remain valid until synchronization. Any pointers into field
    /// stores must point into pinned backing allocations, since sync moves Self.
    /// The callback must not let references to temporary storage escape.
    #[doc(hidden)]
    pub unsafe fn decode_with<'a>(
        &'a mut self,
        build: impl FnOnce(&'a mut S) -> Option<(R, P)>,
        writeback: fn(R, P) -> Option<()>,
    ) -> Option<&'a mut R> {
        unsafe { self.decode_with_order(build, writeback, false) }
    }

    unsafe fn decode_with_order<'a>(
        &'a mut self,
        build: impl FnOnce(&'a mut S) -> Option<(R, P)>,
        writeback: fn(R, P) -> Option<()>,
        sync_fields_first: bool,
    ) -> Option<&'a mut R> {
        if self.writeback.is_some() {
            self.finish()?;
        }
        let (value, state) = build(&mut self.store)?;
        self.writeback = Some((state, writeback, sync_fields_first));
        Some(self.value.insert(value))
    }

    fn finish(&mut self) -> Option<()> {
        let (state, writeback, sync_fields_first) = self.writeback.take()?;
        let value = self.value.take()?;
        // Whole-value writeback must not commit when field validation fails.
        if sync_fields_first {
            core::mem::take(&mut self.store).sync()?;
            return writeback(value, state);
        }
        // Reborrowed field destinations must be updated before their stores sync.
        let written = writeback(value, state);
        let synced = core::mem::take(&mut self.store).sync();
        written.and(synced)
    }
}

impl<'d, R: DecodeMove<'d, Store: EmptyStore + 'd>, S: Store + Default> Default
    for RefMutSizedEncodeStore<'d, R, S>
{
    fn default() -> Self {
        Self {
            ctype: None,
            store: S::default(),
            original: None,
        }
    }
}

impl<R: ReprC, S: Default, P> Default for RefMutSizedDecodeStore<R, S, P> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
            writeback: None,
        }
    }
}

impl<'d, R: DecodeMove<'d, Store: EmptyStore + 'd>, S: Store + Default> Store
    for RefMutSizedEncodeStore<'d, R, S>
{
    fn sync(mut self) -> Option<()> {
        let (original, value) = self.finish()?;
        *original = value;
        Some(())
    }
}

impl<R: ReprC, S: Store + Default, P> Store for RefMutSizedDecodeStore<R, S, P> {
    fn sync(mut self) -> Option<()> {
        self.finish()
    }
}

impl<'d, R: DecodeMove<'d, Store: EmptyStore + 'd>, S: Store + Default> Drop
    for RefMutSizedEncodeStore<'d, R, S>
{
    fn drop(&mut self) {
        drop(self.finish());
    }
}

#[cfg(feature = "alloc")]
pub struct RefWideEncodeStore<R: ReprC<CType: Wide> + ?Sized, S> {
    ctype: Option<Box<R::CType>>,
    fields: S,
}

#[cfg(feature = "alloc")]
pub struct RefDstDecodeStore<R: ToOwned + ?Sized, S> {
    value: Option<R::Owned>,
    store: S,
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, S> RefWideEncodeStore<R, S> {
    pub fn encode(&mut self, encode: impl FnOnce(&mut S) -> Box<R::CType>) -> CWideRef<R> {
        let previous = self.ctype.take();
        self.ctype = Some(encode(&mut self.fields));
        drop(previous);
        let value = &**self.ctype.as_ref().unwrap();
        unsafe {
            CWideRef::from_raw_parts(
                <R::CType as Wide>::as_ptr(value).cast(),
                <R::CType as Wide>::metadata(value),
            )
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: ToOwned + ?Sized, S> RefDstDecodeStore<R, S> {
    pub fn decode<'a>(
        &'a mut self,
        decode: impl FnOnce(&'a mut S) -> Option<R::Owned>,
    ) -> Option<&'a R::Owned> {
        self.value = None;
        let value = decode(&mut self.store)?;
        Some(self.value.insert(value))
    }
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Wide> + ?Sized, S: Default> Default for RefWideEncodeStore<R, S> {
    fn default() -> Self {
        Self {
            ctype: None,
            fields: S::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: ToOwned + ?Sized, S: Default> Default for RefDstDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Wide> + ?Sized, S: Store> Store for RefWideEncodeStore<R, S> {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

#[cfg(feature = "alloc")]
impl<R: ToOwned + ?Sized, S: Store> Store for RefDstDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

/// Owns a slice or slice-tailed C pointee and retains its source borrow until writeback.
///
/// ```compile_fail,E0506
/// use co3::stored::Store;
/// let mut values = [false, true];
/// let mut store = Default::default();
/// co3::soft_encode(&mut values[..], &mut store);
/// values[0] = true;
/// store.sync();
/// ```
#[cfg(feature = "alloc")]
pub struct RefMutWideEncodeStore<
    'a,
    R: ReprC<CType: Wide> + ?Sized,
    S: Store,
    P,
    B = Box<<R as ReprC>::CType>,
> where
    B: core::ops::DerefMut<Target = R::CType>,
{
    ctype: Option<B>,
    fields: Box<S>,
    #[expect(
        clippy::type_complexity,
        reason = "writeback keeps its destination and callback together"
    )]
    writeback: Option<(P, fn(&R::CType, P))>,
    // Retains the exclusive source borrow represented by the writeback state.
    borrowed: PhantomData<&'a mut ()>,
}

#[cfg(feature = "alloc")]
pub struct RefMutDstEncodeStore<'d, R, S: Store + Default>
where
    R: ToOwned<Owned: DecodeMove<'d, Store: EmptyStore + 'd>> + ?Sized,
{
    ctype: Option<<R::Owned as ReprC>::CType>,
    store: S,
    original: Option<&'d mut R>,
}

#[cfg(feature = "alloc")]
pub struct RefMutSliceDecodeStore<R: ReprC<CType: Sized>, S> {
    value: Option<Vec<R>>,
    store: S,
    source: Option<CWideRefMut<[R], true>>,
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Sized>, S> RefMutSliceDecodeStore<R, S> {
    /// The source must remain valid and exclusively accessible until synchronization.
    pub(crate) unsafe fn decode<'a>(
        &'a mut self,
        source: CWideRefMut<[R], true>,
        decode: impl FnOnce(&'a [R::CType], &'a mut S) -> Option<Vec<R>>,
    ) -> Option<&'a mut [R]> {
        self.source = Some(source);
        let source = unsafe { source.into_mut()? };
        let value = decode(source, &mut self.store)?;
        Some(self.value.insert(value))
    }
}

/// Retains a mutable reborrow without allowing its temporary pointer to escape
/// into the longer-lived Rust reference slot.
#[cfg(feature = "alloc")]
pub struct RefMutReferenceEncodeStore<C: CType + Copy + PartialEq, S: Store> {
    pub(crate) value: Option<C>,
    pub(crate) original: Option<C>,
    pub(crate) inner: Box<S>,
}

#[cfg(feature = "alloc")]
impl<C: CType + Copy + PartialEq, S: Store + Default> Default for RefMutReferenceEncodeStore<C, S> {
    fn default() -> Self {
        Self {
            value: None,
            original: None,
            inner: Box::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<C: CType + Copy + PartialEq, S: Store> Store for RefMutReferenceEncodeStore<C, S> {
    fn sync(self) -> Option<()> {
        let unchanged = self.value.is_some() && self.value == self.original;
        // Restore the original pointee even when pointer replacement is rejected.
        self.inner.sync()?;
        unchanged.then_some(())
    }
}

#[cfg(feature = "alloc")]
impl<'d, R, S: Store + Default> RefMutDstEncodeStore<'d, R, S>
where
    R: ToOwned<Owned: DecodeMove<'d, Store: EmptyStore + 'd>> + ?Sized,
{
    pub(crate) fn encode(
        &mut self,
        original: &'d mut R,
        build: impl FnOnce(&mut R, &mut S) -> <R::Owned as ReprC>::CType,
    ) -> &mut <R::Owned as ReprC>::CType {
        drop(self.finish());
        let ctype = build(&mut *original, &mut self.store);
        self.original = Some(original);
        self.ctype.insert(ctype)
    }

    fn finish(&mut self) -> Option<(&'d mut R, R::Owned)> {
        let ctype = self.ctype.take()?;
        let value = unsafe { decode_move::<'d, R::Owned>(ctype) };
        let synced = core::mem::take(&mut self.store).sync();
        let original = self.original.take()?;
        synced?;
        Some((original, value?))
    }
}

#[cfg(feature = "alloc")]
impl<'d, R, S: Store + Default> Default for RefMutDstEncodeStore<'d, R, S>
where
    R: ToOwned<Owned: DecodeMove<'d, Store: EmptyStore + 'd>> + ?Sized,
{
    fn default() -> Self {
        Self {
            ctype: None,
            store: S::default(),
            original: None,
        }
    }
}

#[cfg(feature = "alloc")]
impl<'d, R, S: Store + Default> Store for RefMutDstEncodeStore<'d, R, S>
where
    R: AssignFromOwned + ToOwned<Owned: DecodeMove<'d, Store: EmptyStore + 'd>> + ?Sized,
{
    fn sync(mut self) -> Option<()> {
        let (original, owned) = self.finish()?;
        original.assign_from_owned(owned)
    }
}

#[cfg(feature = "alloc")]
impl<'d, R, S: Store + Default> Drop for RefMutDstEncodeStore<'d, R, S>
where
    R: ToOwned<Owned: DecodeMove<'d, Store: EmptyStore + 'd>> + ?Sized,
{
    fn drop(&mut self) {
        drop(self.finish());
    }
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Sized>, S: Default> Default for RefMutSliceDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
            source: None,
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: EncodeMove<CType: Sized, Store: EmptyStore>, S: Store> Store
    for RefMutSliceDecodeStore<R, S>
{
    fn sync(self) -> Option<()> {
        let source = self.source?;
        self.store.sync()?;
        let value = self.value?;

        if source.metadata() != value.len() {
            return None;
        }

        let source = unsafe { source.into_mut()? };
        for (destination, value) in source.iter_mut().zip(value) {
            *destination = encode_move(value);
        }
        Some(())
    }
}

#[cfg(feature = "alloc")]
impl<
    'a,
    R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized,
    S: Store,
    P,
    B: core::ops::DerefMut<Target = R::CType>,
> RefMutWideEncodeStore<'a, R, S, P, B>
{
    /// Construct the pointee and its writeback state using stable field storage.
    /// Reuse cancels pending writeback before invoking the builder. A panic
    /// leaves no pending conversion; the previous pointee survives the builder.
    pub fn encode(
        &mut self,
        build: impl FnOnce(&mut S) -> (B, P),
        copy_back: fn(&R::CType, P),
    ) -> CWideRefMut<R, true> {
        self.writeback = None;
        let previous = self.ctype.take();
        let (value, state) = build(&mut self.fields);
        self.ctype = Some(value);
        self.writeback = Some((state, copy_back));
        drop(previous);
        let value = &mut **self.ctype.as_mut().unwrap();
        unsafe {
            CWideRefMut::from_raw_parts_mut(
                <R::CType as Wide>::as_mut_ptr(value).cast(),
                <R::CType as Wide>::metadata(value),
            )
        }
    }
}

#[cfg(feature = "alloc")]
impl<
    'a,
    R: ReprC<CType: Wide> + ?Sized,
    S: Store + Default,
    P,
    B: core::ops::DerefMut<Target = R::CType>,
> Default for RefMutWideEncodeStore<'a, R, S, P, B>
{
    fn default() -> Self {
        Self {
            borrowed: PhantomData,
            ctype: None,
            fields: Box::new(S::default()),
            writeback: None,
        }
    }
}

#[cfg(feature = "alloc")]
impl<'a, R: ReprC<CType: Wide> + ?Sized, S: Store, P, B: core::ops::DerefMut<Target = R::CType>>
    Store for RefMutWideEncodeStore<'a, R, S, P, B>
{
    fn sync(self) -> Option<()> {
        if let Some((state, copy_back)) = self.writeback {
            copy_back(self.ctype.as_deref()?, state);
        }
        drop(self.ctype);
        self.fields.sync()
    }
}

impl<D: Default, const N: usize> Default for ArrayStore<D, N> {
    fn default() -> Self {
        Self(core::array::from_fn(|_| D::default()))
    }
}

impl<D: Store, const N: usize> Store for ArrayStore<D, N> {
    fn sync(self) -> Option<()> {
        let mut is_valid = true;

        for store in self.0 {
            if store.sync().is_none() {
                is_valid = false;
            }
        }

        is_valid.then_some(())
    }
}

unsafe impl<D: EmptyStore, const N: usize> EmptyStore for ArrayStore<D, N> {}

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

/// This struct exists only because [arrays don't yet implement Default](https://github.com/rust-lang/rust/issues/61415)
pub struct ArrayStore<D, const N: usize>(pub(crate) [D; N]);

#[cfg(all(test, feature = "alloc"))]
mod owned_encode_state_tests {
    extern crate std;

    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use super::*;

    #[derive(Clone)]
    struct Tracked {
        drops: Arc<AtomicUsize>,
        decodes: Arc<AtomicUsize>,
    }

    impl Tracked {
        fn new() -> Self {
            Self {
                drops: Arc::new(AtomicUsize::new(0)),
                decodes: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn carrier(&self) -> *mut u8 {
            Box::into_raw(Box::new(self.clone())).cast()
        }
    }

    impl Drop for Tracked {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl ReprC for Tracked {
        type CType = *mut u8;
    }

    unsafe impl<'d> DecodeMove<'d> for Tracked {
        type Store = ();
        unsafe fn soft_decode<'itm: 'd>(source: *mut u8, _: &'itm mut ()) -> Option<Self> {
            if source.is_null() {
                return None;
            }
            let value = *unsafe { Box::from_raw(source.cast::<Tracked>()) };
            value.decodes.fetch_add(1, Ordering::SeqCst);
            Some(value)
        }
    }

    impl AssignFromOwned for Tracked {
        fn assign_from_owned(&mut self, owned: Self) -> Option<()> {
            *self = owned;
            Some(())
        }
    }

    #[derive(Default)]
    struct Auxiliary {
        fail: bool,
    }
    impl Store for Auxiliary {
        fn sync(self) -> Option<()> {
            (!self.fail).then_some(())
        }
    }

    macro_rules! state_tests {
        ($module:ident, $store:ident) => {
            mod $module {
                use super::*;

                #[test]
                fn drop_decodes_once_without_writeback() {
                    let mut source = Tracked::new();
                    let drops = source.drops.clone();
                    let decodes = source.decodes.clone();
                    let mut store = $store::<Tracked, ()>::default();
                    store.encode(&mut source, |source, _| source.carrier());
                    drop(store);
                    assert_eq!(decodes.load(Ordering::SeqCst), 1);
                    assert_eq!(drops.load(Ordering::SeqCst), 1);
                    drop(source);
                    assert_eq!(drops.load(Ordering::SeqCst), 2);
                }

                #[test]
                fn sync_decodes_once_and_writes_back() {
                    let mut source = Tracked::new();
                    let drops = source.drops.clone();
                    let decodes = source.decodes.clone();
                    let mut store = $store::<Tracked, ()>::default();
                    store.encode(&mut source, |source, _| source.carrier());
                    assert_eq!(store.sync(), Some(()));
                    assert_eq!(decodes.load(Ordering::SeqCst), 1);
                    assert_eq!(drops.load(Ordering::SeqCst), 1);
                    drop(source);
                    assert_eq!(drops.load(Ordering::SeqCst), 2);
                }

                #[test]
                fn reuse_then_builder_panic_cannot_reuse_old_carrier() {
                    let mut first = Tracked::new();
                    let mut second = Tracked::new();
                    let decodes = first.decodes.clone();
                    let drops = first.drops.clone();
                    let second_decodes = second.decodes.clone();
                    assert!(
                        catch_unwind(AssertUnwindSafe(move || {
                            let mut store = $store::<Tracked, ()>::default();
                            store.encode(&mut first, |source, _| source.carrier());
                            store.encode(&mut second, |_, _| panic!("build failed"));
                        }))
                        .is_err()
                    );
                    assert_eq!(decodes.load(Ordering::SeqCst), 1);
                    // One temporary and the original source were dropped.
                    assert_eq!(drops.load(Ordering::SeqCst), 2);
                    assert_eq!(second_decodes.load(Ordering::SeqCst), 0);
                }

                #[test]
                fn auxiliary_failure_releases_decoded_value() {
                    let mut source = Tracked::new();
                    let drops = source.drops.clone();
                    let decodes = source.decodes.clone();
                    let mut store = $store::<Tracked, Auxiliary>::default();
                    store.encode(&mut source, |source, fields| {
                        fields.fail = true;
                        source.carrier()
                    });
                    assert_eq!(store.sync(), None);
                    assert_eq!(decodes.load(Ordering::SeqCst), 1);
                    assert_eq!(drops.load(Ordering::SeqCst), 1);
                }
            }
        };
    }
    state_tests!(sized, RefMutSizedEncodeStore);
    state_tests!(dst, RefMutDstEncodeStore);
}

/// Owns converted vector elements while a shared carrier borrows their allocation.
#[cfg(feature = "alloc")]
pub struct RefVecEncodeStore<T: ReprC<CType: Sized>, S> {
    elements: Vec<T::CType>,
    fields: S,
}

#[cfg(feature = "alloc")]
impl<T: ReprC<CType: Sized>, S: Default> Default for RefVecEncodeStore<T, S> {
    fn default() -> Self {
        Self {
            elements: Vec::new(),
            fields: S::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<T: ReprC<CType: CType + Sized>, S> RefVecEncodeStore<T, S> {
    pub fn encode(&mut self, encode: impl FnOnce(&mut S) -> Vec<T::CType>) -> crate::vec::CVec<T> {
        self.elements = encode(&mut self.fields);
        unsafe {
            crate::vec::CVec::<T>::from_raw_parts(
                self.elements.as_mut_ptr(),
                self.elements.len(),
                self.elements.capacity(),
            )
        }
    }
}

#[cfg(feature = "alloc")]
impl<T: ReprC<CType: Sized>, S: Store> Store for RefVecEncodeStore<T, S> {
    fn sync(self) -> Option<()> {
        self.fields.sync()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::{RefMutSizedDecodeStore, RefSizedDecodeStore, RefSizedEncodeStore, Store};

    #[test]
    fn failed_build_clears_carrier_and_allows_reuse() {
        let mut store = RefSizedEncodeStore::<u32, ()>::default();
        store.encode(|_| 7);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.encode(|_| panic!("encoding failed"));
        }));
        assert!(result.is_err());
        assert!(store.ctype.is_none());

        let encoded = store.encode(|_| 11);
        assert_eq!(unsafe { *encoded.as_ptr() }, 11);
    }

    #[test]
    fn failed_decode_clears_value_and_allows_reuse() {
        let mut store = RefSizedDecodeStore::<u32, ()>::default();
        assert_eq!(store.decode(|_| Some(7)), Some(&7));
        assert_eq!(store.decode(|_| None), None);
        assert!(store.value.is_none());
        assert_eq!(store.decode(|_| Some(11)), Some(&11));
    }

    #[test]
    fn mutable_decode_only_arms_writeback_after_success() {
        let mut source = 7_u32;
        let mut store = RefMutSizedDecodeStore::<u32, ()>::default();
        let value = unsafe { store.decode(&mut source, |_| Some(7)) }.unwrap();
        *value = 11;
        assert_eq!(store.sync(), Some(()));
        assert_eq!(source, 11);

        let mut store = RefMutSizedDecodeStore::<u32, ()>::default();
        assert!(unsafe { store.decode(&mut source, |_| None) }.is_none());
        assert_eq!(store.sync(), None);
        assert_eq!(source, 11);
    }

    #[test]
    fn mutable_decode_reuse_synchronizes_previous_value() {
        let mut first = 7_u32;
        let mut second = 3_u32;
        let mut store = RefMutSizedDecodeStore::<u32, ()>::default();
        *unsafe { store.decode(&mut first, |_| Some(7)) }.unwrap() = 11;
        *unsafe { store.decode(&mut second, |_| Some(3)) }.unwrap() = 13;
        assert_eq!(first, 11);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(second, 13);
    }

    #[test]
    fn mutable_decode_reuse_stops_when_synchronization_fails() {
        #[derive(Default)]
        struct Fields(bool);
        impl Store for Fields {
            fn sync(self) -> Option<()> {
                (!self.0).then_some(())
            }
        }

        let mut first = 7_u32;
        let mut second = 3_u32;
        let mut store = RefMutSizedDecodeStore::<u32, Fields>::default();
        *unsafe {
            store.decode(&mut first, |fields| {
                fields.0 = true;
                Some(7)
            })
        }
        .unwrap() = 11;
        let mut called = false;
        assert!(
            unsafe {
                store.decode(&mut second, |_| {
                    called = true;
                    Some(3)
                })
            }
            .is_none()
        );
        assert!(!called);
        assert_eq!(first, 7);
        assert_eq!(second, 3);
        assert_eq!(store.sync(), None);
    }
}
