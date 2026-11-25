#[cfg(feature = "alloc")]
use alloc::{borrow::ToOwned, boxed::Box, vec::Vec};
#[cfg(feature = "alloc")]
use core::marker::PhantomData;

use rust_spec::Unspecified;

#[cfg(feature = "alloc")]
use crate::{
    CType,
    convert::AssignFromOwned,
    wide::{
        Wide, WideHeader,
        abi::{SoftWideRef as AbiSoftWideRef, SoftWideRefMut as AbiSoftWideRefMut},
    },
};
use crate::{
    ReprC,
    convert::{
        DecodeMove, DecodeMoveUnchecked, EmptyStore, EncodeMove, EncodeMoveUnchecked, Store,
        decode_move, decode_move_unchecked, encode_move, encode_move_unchecked,
    },
    lang::{self, Language},
    reference::abi::{SoftRef, SoftRefMut},
};

/// Combine synchronization results, exhausting the iterator even after a failure.
pub(crate) fn sync_all(results: impl IntoIterator<Item = Option<()>>) -> Option<()> {
    results.into_iter().fold(Some(()), Option::and)
}

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
        sync_all(self.into_iter().map(Store::sync))
    }
}

/// Extract an independently owned result from a reference decoding store.
///
/// # Safety
/// The returned value must remain valid after the source carrier is dropped.
/// Extraction must preserve any borrows the value holds into child stores.
pub unsafe trait TakeDecoded {
    type Value;
    /// # Safety
    /// `store` must permit exclusive access to the result slot. No reference
    /// to that slot returned by decoding may be used afterward.
    unsafe fn take_decoded(store: *mut Self) -> Option<Self::Value>;
}

/// Field extraction adapter used by generated aggregate decoders.
#[doc(hidden)]
pub struct DecodedField<'d, T: ?Sized, S, Layout, L = lang::C> {
    value: &'d T,
    store: *mut S,
    layout: core::marker::PhantomData<(Layout, fn() -> L)>,
}

impl<'d, T: ?Sized, S, Layout, L> DecodedField<'d, T, S, Layout, L> {
    /// # Safety
    /// `value` must be the result of decoding into `store`. The store pointer
    /// must be captured before decoding and remain valid through extraction.
    /// Extraction must have exclusive access to the result slot. No decoded
    /// reference to that slot may be used afterward.
    pub unsafe fn new(value: &'d T, store: *mut S) -> Self {
        Self {
            value,
            store,
            layout: core::marker::PhantomData,
        }
    }
}

impl<'d, T: ?Sized, S: 'd, Layout, L: Language> DecodedField<'d, T, S, Layout, L>
where
    &'d T: DecodeMove<'d, L, Store = S>,
    Self: TakeDecoded,
{
    /// Decode a shared field and extract its owned value.
    ///
    /// # Safety
    /// The source must satisfy `DecodeMove::soft_decode` for `&T`. The decoded
    /// reference must be available for immediate extraction by `TakeDecoded`.
    pub unsafe fn decode(
        source: <&'d T as ReprC>::CType,
        store: &'d mut S,
    ) -> Option<<Self as TakeDecoded>::Value> {
        unsafe {
            Self::decode_with(store, |store| {
                <&'d T as DecodeMove<'d, L>>::soft_decode(source, store)
            })
        }
    }
}

impl<'d, T: ?Sized, S: 'd, Layout, L> DecodedField<'d, T, S, Layout, L>
where
    Self: TakeDecoded,
{
    /// Extract the owned field from the reference returned by `decode`.
    ///
    /// # Safety
    /// `decode` must return the corresponding decoder's reference and must not
    /// retain an alias to the result slot that is used after extraction.
    unsafe fn decode_with(
        store: &'d mut S,
        decode: impl FnOnce(&'d mut S) -> Option<&'d T>,
    ) -> Option<<Self as TakeDecoded>::Value> {
        let pointer = core::ptr::from_mut(store);
        // Decode through this pointer so writes to the result slot preserve its
        // provenance for extraction. The decoded reference is used only by the
        // adapter: stable fields clone it, and store extraction discards it.
        let value = decode(unsafe { &mut *pointer })?;
        let mut field = unsafe { Self::new(value, pointer) };
        unsafe { Self::take_decoded(&mut field) }
    }
}

unsafe impl<T: Clone, S, L> TakeDecoded for DecodedField<'_, T, S, rust_spec::Specified, L> {
    type Value = T;

    unsafe fn take_decoded(field: *mut Self) -> Option<T> {
        Some(T::clone(unsafe { (*field).value }))
    }
}

unsafe impl<T: ?Sized, S: TakeDecoded, L> TakeDecoded for DecodedField<'_, T, S, Unspecified, L> {
    type Value = S::Value;

    unsafe fn take_decoded(field: *mut Self) -> Option<Self::Value> {
        unsafe { S::take_decoded((*field).store) }
    }
}

/// Field extraction adapter for decoders that take no language parameter.
#[doc(hidden)]
pub type DecodedFieldUnchecked<'d, T, S, Layout> = DecodedField<'d, T, S, Layout, ()>;

impl<'d, T: ?Sized, S: 'd, Layout> DecodedFieldUnchecked<'d, T, S, Layout>
where
    &'d T: DecodeMoveUnchecked<'d, SoftStore = S>,
    Self: TakeDecoded,
{
    /// Decode a known-valid shared field and extract its owned value.
    ///
    /// # Safety
    /// The source must satisfy `DecodeMoveUnchecked::soft_decode_unchecked` for
    /// `&T`. Its result must be available for immediate extraction by `TakeDecoded`.
    pub unsafe fn decode_unchecked(
        source: <&'d T as ReprC>::CType,
        store: &'d mut S,
    ) -> <Self as TakeDecoded>::Value {
        unsafe {
            Self::decode_with(store, |store| {
                Some(<&'d T as DecodeMoveUnchecked<'d>>::soft_decode_unchecked(
                    source, store,
                ))
            })
        }
        .expect("unchecked decoding produces a value")
    }
}

pub struct RefSizedEncodeStore<R: ReprC<CType: Sized>, S> {
    ctype: Option<R::CType>,
    store: S,
}

impl<R: ReprC<CType: Sized>, S: Store> RefSizedEncodeStore<R, S> {
    pub fn encode(&mut self, encode: impl FnOnce(&mut S) -> R::CType) -> SoftRef<R::CType> {
        self.ctype = None;
        let value = encode(&mut self.store);
        SoftRef::from_ref(self.ctype.insert(value))
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

impl<R: ReprC<CType: Sized>, S: Store> Store for RefSizedEncodeStore<R, S> {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

pub struct RefSizedDecodeStore<R, S> {
    value: Option<R>,
    store: S,
}

impl<R, S> RefSizedDecodeStore<R, S> {
    pub fn decode<'a>(&'a mut self, decode: impl FnOnce(&'a mut S) -> Option<R>) -> Option<&'a R> {
        self.value = None;
        let value = decode(&mut self.store)?;
        Some(self.value.insert(value))
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

unsafe impl<R, S> TakeDecoded for RefSizedDecodeStore<R, S> {
    type Value = R;

    unsafe fn take_decoded(store: *mut Self) -> Option<R> {
        unsafe { (*core::ptr::addr_of_mut!((*store).value)).take() }
    }
}

impl<R, S: Store> Store for RefSizedDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

pub struct RefMutSizedEncodeStore<
    'd,
    R: DecodeMove<'d, L, Store: EmptyStore + 'd>,
    S: Store + Default,
    L: Language = lang::C,
> {
    state: OwnedEncodeState<'d, R, R, S, L>,
}

impl<'d, R: DecodeMove<'d, L, Store: EmptyStore + 'd>, S: Store + Default, L: Language>
    RefMutSizedEncodeStore<'d, R, S, L>
{
    /// Encode a clone with its matching writeback store.
    #[doc(hidden)]
    pub fn encode_clone(&mut self, original: &'d mut R) -> SoftRefMut<R::CType>
    where
        R: Clone + EncodeMove<L, Store = S>,
    {
        self.encode(original, |value, fields| {
            EncodeMove::<L>::soft_encode(R::clone(value), fields)
        })
    }

    pub(crate) fn encode(
        &mut self,
        original: &'d mut R,
        encode: impl FnOnce(&mut R, &mut S) -> R::CType,
    ) -> SoftRefMut<R::CType> {
        SoftRefMut::from_mut(self.state.encode(original, encode))
    }
}

impl<'d, R: DecodeMove<'d, L, Store: EmptyStore + 'd>, S: Store + Default, L: Language> Default
    for RefMutSizedEncodeStore<'d, R, S, L>
{
    fn default() -> Self {
        Self {
            state: OwnedEncodeState::default(),
        }
    }
}

impl<'d, R: DecodeMove<'d, L, Store: EmptyStore + 'd>, S: Store + Default, L: Language> Store
    for RefMutSizedEncodeStore<'d, R, S, L>
{
    fn sync(mut self) -> Option<()> {
        let (original, value) = self.state.finish()?;
        *original = value;
        Some(())
    }
}

pub struct RefMutSizedEncodeStoreUnchecked<
    'd,
    R: DecodeMoveUnchecked<'d, SoftStore: EmptyStore + 'd>,
    S: Store + Default,
> {
    state: OwnedEncodeState<'d, R, R, S, Unchecked>,
}

impl<'d, R: DecodeMoveUnchecked<'d, SoftStore: EmptyStore + 'd>, S: Store + Default>
    RefMutSizedEncodeStoreUnchecked<'d, R, S>
{
    #[cfg(feature = "repr-rust")]
    pub(crate) fn encode(
        &mut self,
        original: &'d mut R,
        encode: impl FnOnce(&mut R, &mut S) -> R::CType,
    ) -> SoftRefMut<R::CType> {
        SoftRefMut::from_mut(self.state.encode(original, encode))
    }
}

impl<'d, R: DecodeMoveUnchecked<'d, SoftStore: EmptyStore + 'd>, S: Store + Default> Default
    for RefMutSizedEncodeStoreUnchecked<'d, R, S>
{
    fn default() -> Self {
        Self {
            state: OwnedEncodeState::default(),
        }
    }
}

impl<'d, R: DecodeMoveUnchecked<'d, SoftStore: EmptyStore + 'd>, S: Store + Default> Store
    for RefMutSizedEncodeStoreUnchecked<'d, R, S>
{
    fn sync(mut self) -> Option<()> {
        let (original, value) = self.state.finish()?;
        *original = value;
        Some(())
    }
}

// Drop, reuse, and explicit sync must all reclaim an owned carrier exactly once.
trait WritebackDecoder<'d, V: ReprC<CType: Sized>> {
    /// # Safety
    /// The carrier must satisfy the decoder's ownership and validity contract.
    unsafe fn decode(carrier: V::CType) -> Option<V>;
}

impl<'d, L: Language, V: DecodeMove<'d, L, Store: EmptyStore + 'd>> WritebackDecoder<'d, V> for L {
    unsafe fn decode(carrier: V::CType) -> Option<V> {
        unsafe { decode_move::<V, L>(carrier) }
    }
}

struct Unchecked;

impl<'d, V: DecodeMoveUnchecked<'d, SoftStore: EmptyStore + 'd>> WritebackDecoder<'d, V>
    for Unchecked
{
    unsafe fn decode(carrier: V::CType) -> Option<V> {
        Some(unsafe { decode_move_unchecked::<V>(carrier) })
    }
}

/// Owns one pending carrier. Finishing consumes it exactly once; callers decide
/// whether to commit the decoded value or drop it during cleanup and reuse.
struct OwnedEncodeState<
    'd,
    R: ?Sized,
    V: ReprC<CType: Sized>,
    S: Store + Default,
    D: WritebackDecoder<'d, V>,
> {
    decoder: core::marker::PhantomData<fn() -> D>,
    ctype: Option<V::CType>,
    store: S,
    original: Option<&'d mut R>,
}

impl<'d, R: ?Sized, V: ReprC<CType: Sized>, S: Store + Default, D: WritebackDecoder<'d, V>>
    OwnedEncodeState<'d, R, V, S, D>
{
    fn encode(
        &mut self,
        original: &'d mut R,
        build: impl FnOnce(&mut R, &mut S) -> V::CType,
    ) -> &mut V::CType {
        drop(self.finish());
        let ctype = build(&mut *original, &mut self.store);
        self.original = Some(original);
        self.ctype.insert(ctype)
    }

    fn finish(&mut self) -> Option<(&'d mut R, V)> {
        let ctype = self.ctype.take()?;
        let value = unsafe { D::decode(ctype) };
        let synced = core::mem::take(&mut self.store).sync();
        let original = self.original.take()?;
        synced?;
        Some((original, value?))
    }
}

impl<'d, R: ?Sized, V: ReprC<CType: Sized>, S: Store + Default, D: WritebackDecoder<'d, V>> Default
    for OwnedEncodeState<'d, R, V, S, D>
{
    fn default() -> Self {
        Self {
            decoder: core::marker::PhantomData,
            ctype: None,
            store: S::default(),
            original: None,
        }
    }
}

impl<'d, R: ?Sized, V: ReprC<CType: Sized>, S: Store + Default, D: WritebackDecoder<'d, V>> Drop
    for OwnedEncodeState<'d, R, V, S, D>
{
    fn drop(&mut self) {
        drop(self.finish());
    }
}

type DecodeWriteback<R, P> = (P, fn(R, P) -> Option<()>, bool);

pub struct RefMutSizedDecodeStore<R: ReprC, S, P = *mut <R as ReprC>::CType> {
    value: Option<R>,
    store: S,
    writeback: Option<DecodeWriteback<R, P>>,
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
                    source.write(encode_move::<_, lang::C>(value));
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

impl<R: ReprC, S: Default, P> Default for RefMutSizedDecodeStore<R, S, P> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
            writeback: None,
        }
    }
}

impl<R: ReprC, S: Store + Default, P> Store for RefMutSizedDecodeStore<R, S, P> {
    fn sync(mut self) -> Option<()> {
        self.finish()
    }
}

pub struct RefMutSizedDecodeStoreUnchecked<R: ReprC, S, P = *mut <R as ReprC>::CType>(
    RefMutSizedDecodeStore<R, S, P>,
);

impl<R: EncodeMoveUnchecked<SoftStore: EmptyStore>, S: Store + Default>
    RefMutSizedDecodeStoreUnchecked<R, S>
{
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
            self.0.decode_with_order(
                |fields| build(fields).map(|value| (value, source)),
                |value, source| {
                    source.write(encode_move_unchecked::<_>(value));
                    Some(())
                },
                true,
            )
        }
    }
}

impl<R: ReprC, S: Store + Default, P> RefMutSizedDecodeStoreUnchecked<R, S, P> {
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
        unsafe { self.0.decode_with(build, writeback) }
    }
}

impl<R: ReprC, S: Default, P> Default for RefMutSizedDecodeStoreUnchecked<R, S, P> {
    fn default() -> Self {
        Self(RefMutSizedDecodeStore::default())
    }
}

impl<R: ReprC, S: Store + Default, P> Store for RefMutSizedDecodeStoreUnchecked<R, S, P> {
    fn sync(self) -> Option<()> {
        self.0.sync()
    }
}

#[cfg(feature = "alloc")]
pub struct RefWideEncodeStore<R: ReprC<CType: Wide> + ?Sized, S> {
    ctype: Option<Box<R::CType>>,
    fields: S,
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, S> RefWideEncodeStore<R, S> {
    pub fn encode(
        &mut self,
        encode: impl FnOnce(&mut S) -> Box<R::CType>,
    ) -> AbiSoftWideRef<R::CType> {
        // Moving the previous box can invalidate pointers borrowed from it.
        // Finish construction before replacing its owner.
        let next = encode(&mut self.fields);
        self.ctype = Some(next);
        let value = &**self.ctype.as_ref().unwrap();
        unsafe {
            AbiSoftWideRef::from_raw_parts(
                <R::CType as Wide>::as_ptr(value).cast(),
                <R::CType as Wide>::metadata(value),
            )
        }
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
impl<R: ReprC<CType: Wide> + ?Sized, S: Store> Store for RefWideEncodeStore<R, S> {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

#[cfg(feature = "alloc")]
pub struct RefDstDecodeStore<R: ToOwned + ?Sized, S> {
    value: Option<R::Owned>,
    store: S,
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
impl<R: ToOwned + ?Sized, S: Default> Default for RefDstDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
        }
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
/// use co3::{convert::Store, lang};
/// let mut values = [false, true];
/// let mut store = Default::default();
/// unsafe { co3::soft_encode::<_, lang::C>(&mut values[..], &mut store) };
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
    ) -> AbiSoftWideRefMut<R::CType> {
        self.writeback = None;
        let previous = self.ctype.take();
        let (value, state) = build(&mut self.fields);
        self.ctype = Some(value);
        self.writeback = Some((state, copy_back));
        drop(previous);
        let value = &mut **self.ctype.as_mut().unwrap();
        unsafe {
            AbiSoftWideRefMut::from_raw_parts_mut(
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

#[cfg(feature = "alloc")]
pub struct RefMutDstEncodeStore<'d, R, S: Store + Default, L: Language = lang::C>
where
    R: ToOwned<Owned: DecodeMove<'d, L, Store: EmptyStore + 'd>> + ?Sized,
{
    state: OwnedEncodeState<'d, R, R::Owned, S, L>,
}

#[cfg(feature = "alloc")]
impl<'d, R, S: Store + Default, L: Language> RefMutDstEncodeStore<'d, R, S, L>
where
    R: ToOwned<Owned: DecodeMove<'d, L, Store: EmptyStore + 'd>> + ?Sized,
{
    #[cfg(feature = "repr-rust")]
    pub(crate) fn encode(
        &mut self,
        original: &'d mut R,
        build: impl FnOnce(&mut R, &mut S) -> <R::Owned as ReprC>::CType,
    ) -> &mut <R::Owned as ReprC>::CType {
        self.state.encode(original, build)
    }
}

#[cfg(feature = "alloc")]
impl<'d, R, S: Store + Default, L: Language> Default for RefMutDstEncodeStore<'d, R, S, L>
where
    R: ToOwned<Owned: DecodeMove<'d, L, Store: EmptyStore + 'd>> + ?Sized,
{
    fn default() -> Self {
        Self {
            state: OwnedEncodeState::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<'d, R, S: Store + Default, L: Language> Store for RefMutDstEncodeStore<'d, R, S, L>
where
    R: AssignFromOwned + ToOwned<Owned: DecodeMove<'d, L, Store: EmptyStore + 'd>> + ?Sized,
{
    fn sync(mut self) -> Option<()> {
        let (original, owned) = self.state.finish()?;
        original.assign_from_owned(owned)
    }
}

#[cfg(feature = "alloc")]
pub struct RefMutSliceDecodeStore<R: ReprC<CType: Sized>, S> {
    value: Option<Vec<R>>,
    store: S,
    source: Option<AbiSoftWideRefMut<<[R] as crate::ReprC>::CType>>,
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Sized>, S> RefMutSliceDecodeStore<R, S> {
    /// The source must remain valid and exclusively accessible until synchronization.
    #[cfg(feature = "repr-rust")]
    pub(crate) unsafe fn decode<'a>(
        &'a mut self,
        source: AbiSoftWideRefMut<<[R] as crate::ReprC>::CType>,
        decode: impl FnOnce(&'a [R::CType], &'a mut S) -> Option<Vec<R>>,
    ) -> Option<&'a mut [R]> {
        self.source = Some(source);
        let source = unsafe { source.into_mut()? };
        let value = decode(source, &mut self.store)?;
        Some(self.value.insert(value))
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
impl<R: ReprC<CType: Sized>, S: Store> RefMutSliceDecodeStore<R, S> {
    fn sync_with(self, encode: impl Fn(R) -> R::CType) -> Option<()> {
        let source = self.source?;
        self.store.sync()?;
        let value = self.value?;

        if source.metadata() != value.len() {
            return None;
        }

        let source = unsafe { source.into_mut()? };
        for (destination, value) in source.iter_mut().zip(value) {
            *destination = encode(value);
        }
        Some(())
    }
}

#[cfg(feature = "alloc")]
impl<R: EncodeMove<CType: Sized, Store: EmptyStore>, S: Store> Store
    for RefMutSliceDecodeStore<R, S>
{
    fn sync(self) -> Option<()> {
        self.sync_with(|value| unsafe { encode_move::<_, lang::C>(value) })
    }
}

#[cfg(feature = "alloc")]
pub struct RefMutSliceDecodeStoreUnchecked<R: ReprC<CType: Sized>, S>(RefMutSliceDecodeStore<R, S>);

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Sized>, S> RefMutSliceDecodeStoreUnchecked<R, S> {
    /// The source must remain valid and exclusively accessible until synchronization.
    #[cfg(feature = "repr-rust")]
    pub(crate) unsafe fn decode<'a>(
        &'a mut self,
        source: AbiSoftWideRefMut<<[R] as crate::ReprC>::CType>,
        decode: impl FnOnce(&'a [R::CType], &'a mut S) -> Option<Vec<R>>,
    ) -> Option<&'a mut [R]> {
        unsafe { self.0.decode(source, decode) }
    }
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Sized>, S: Default> Default for RefMutSliceDecodeStoreUnchecked<R, S> {
    fn default() -> Self {
        Self(RefMutSliceDecodeStore::default())
    }
}

#[cfg(feature = "alloc")]
impl<R: EncodeMoveUnchecked<CType: Sized, SoftStore: EmptyStore>, S: Store> Store
    for RefMutSliceDecodeStoreUnchecked<R, S>
{
    fn sync(self) -> Option<()> {
        self.0
            .sync_with(|value| unsafe { encode_move_unchecked(value) })
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

/// This struct exists only because [arrays don't yet implement Default](https://github.com/rust-lang/rust/issues/61415)
pub struct ArrayStore<D, const N: usize>(pub(crate) [D; N]);

impl<D: Default, const N: usize> Default for ArrayStore<D, N> {
    fn default() -> Self {
        Self(core::array::from_fn(|_| D::default()))
    }
}

impl<D: Store, const N: usize> Store for ArrayStore<D, N> {
    fn sync(self) -> Option<()> {
        sync_all(self.0.into_iter().map(Store::sync))
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

/// Owns converted vector elements while a shared carrier borrows their allocation.
#[cfg(feature = "alloc")]
pub struct RefVecEncodeStore<T: ReprC<CType: Sized>, S> {
    elements: Vec<T::CType>,
    fields: S,
}

#[cfg(feature = "alloc")]
impl<T: ReprC<CType: CType + Sized>, S> RefVecEncodeStore<T, S> {
    pub fn encode(
        &mut self,
        encode: impl FnOnce(&mut S) -> Vec<T::CType>,
    ) -> crate::vec::abi::Vec<T::CType> {
        self.elements = encode(&mut self.fields);
        unsafe {
            crate::vec::abi::Vec::<T::CType>::from_raw_parts(
                self.elements.as_mut_ptr(),
                self.elements.len(),
                self.elements.capacity(),
            )
        }
    }
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
impl<T: ReprC<CType: Sized>, S: Store> Store for RefVecEncodeStore<T, S> {
    fn sync(self) -> Option<()> {
        self.fields.sync()
    }
}

#[cfg(test)]
mod tests {
    use crate::lang;

    #[test]
    fn take_decoded_preserves_non_static_borrows_and_extracts_once() {
        let value = 7;
        let mut store = super::RefSizedDecodeStore {
            value: Some(&value),
            store: (),
        };
        let decoded = unsafe { super::TakeDecoded::take_decoded(&mut store) }.unwrap();
        assert!(core::ptr::eq(decoded, &value));
        assert!(unsafe { super::TakeDecoded::take_decoded(&mut store) }.is_none());
    }

    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, sync::Arc, vec::Vec};
    use core::cell::RefCell;
    #[cfg(feature = "alloc")]
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::{
        ArrayStore, RefMutSizedDecodeStore, RefSizedDecodeStore, RefSizedEncodeStore, Store,
    };
    #[cfg(feature = "alloc")]
    use super::{RefMutWideEncodeStore, RefWideEncodeStore, *};
    #[cfg(feature = "alloc")]
    use crate::wide::tests::{Custom, Metadata};

    #[test]
    fn encoding_store_uses_its_language_for_sync_reuse_and_drop() {
        use core::sync::atomic::{AtomicUsize, Ordering};

        use crate::lang::{self, Language};

        static C_CALLS: AtomicUsize = AtomicUsize::new(0);
        static CPP_CALLS: AtomicUsize = AtomicUsize::new(0);
        struct Value(u8);
        impl crate::ReprC for Value {
            type CType = u8;
        }
        macro_rules! decoder {
            ($language:ty, $calls:ident) => {
                unsafe impl<'d> crate::convert::DecodeMove<'d, $language> for Value {
                    type Store = ();
                    unsafe fn soft_decode<'itm: 'd>(source: u8, _: &'itm mut ()) -> Option<Self> {
                        $calls.fetch_add(1, Ordering::SeqCst);
                        Some(Value(source))
                    }
                }
            };
        }
        decoder!(lang::C, C_CALLS);
        decoder!(lang::Cpp, CPP_CALLS);
        fn run<L: Language>()
        where
            for<'d> Value: crate::convert::DecodeMove<'d, L, Store = ()>,
        {
            let mut first = Value(1);
            let mut second = Value(2);
            let mut store = super::RefMutSizedEncodeStore::<Value, (), L>::default();
            store.encode(&mut first, |_, _| 3);
            // Reuse decodes the previous carrier before replacing it.
            store.encode(&mut second, |_, _| 4);
            Store::sync(store).unwrap();
            assert_eq!(second.0, 4);
            // Cleanup also consumes a pending carrier once.
            let mut store = super::RefMutSizedEncodeStore::<Value, (), L>::default();
            store.encode(&mut first, |_, _| 5);
            drop(store);
        }
        run::<lang::C>();
        run::<lang::Cpp>();
        assert_eq!(C_CALLS.load(Ordering::SeqCst), 3);
        assert_eq!(CPP_CALLS.load(Ordering::SeqCst), 3);
    }

    #[cfg(feature = "alloc")]
    #[derive(Clone)]
    struct Tracked {
        drops: Arc<AtomicUsize>,
        decodes: Arc<AtomicUsize>,
    }

    #[cfg(feature = "alloc")]
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

    #[cfg(feature = "alloc")]
    impl Drop for Tracked {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[cfg(feature = "alloc")]
    impl ReprC for Tracked {
        type CType = *mut u8;
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d> DecodeMoveUnchecked<'d> for Tracked {
        type SoftStore = ();
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: *mut u8, _: &'itm mut ()) -> Self {
            let value = *unsafe { Box::from_raw(source.cast::<Tracked>()) };
            value.decodes.fetch_add(1, Ordering::SeqCst);
            value
        }
    }
    #[cfg(feature = "alloc")]
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

    #[cfg(feature = "alloc")]
    impl AssignFromOwned for Tracked {
        fn assign_from_owned(&mut self, owned: Self) -> Option<()> {
            *self = owned;
            Some(())
        }
    }

    #[cfg(feature = "alloc")]
    #[derive(Default)]
    struct Auxiliary {
        fail: bool,
    }
    #[cfg(feature = "alloc")]
    impl Store for Auxiliary {
        fn sync(self) -> Option<()> {
            (!self.fail).then_some(())
        }
    }

    #[cfg(feature = "alloc")]
    struct CheckCleanup {
        decodes: Arc<AtomicUsize>,
        drops: Arc<AtomicUsize>,
        second_decodes: Arc<AtomicUsize>,
    }

    #[cfg(feature = "alloc")]
    impl Drop for CheckCleanup {
        fn drop(&mut self) {
            assert_eq!(self.decodes.load(Ordering::SeqCst), 1);
            // One temporary and the original source were dropped.
            assert_eq!(self.drops.load(Ordering::SeqCst), 2);
            assert_eq!(self.second_decodes.load(Ordering::SeqCst), 0);
        }
    }

    #[cfg(feature = "alloc")]
    macro_rules! state_tests {
        ($store:ident, $drop_test:ident, $sync_test:ident, $panic_test:ident, $failure_test:ident) => {
            #[test]
            fn $drop_test() {
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
            fn $sync_test() {
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
            #[should_panic(expected = "build failed")]
            fn $panic_test() {
                let first = Tracked::new();
                let second = Tracked::new();
                // This guard runs after the store and both moved sources unwind.
                let _cleanup = CheckCleanup {
                    decodes: first.decodes.clone(),
                    drops: first.drops.clone(),
                    second_decodes: second.decodes.clone(),
                };
                let mut first = first;
                let mut second = second;
                let mut store = $store::<Tracked, ()>::default();
                store.encode(&mut first, |source, _| source.carrier());
                store.encode(&mut second, |_, _| panic!("build failed"));
            }

            #[test]
            fn $failure_test() {
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
        };
    }
    #[cfg(feature = "alloc")]
    state_tests!(
        RefMutSizedEncodeStore,
        sized_drop,
        sized_sync,
        sized_panic,
        sized_failure
    );
    #[cfg(feature = "alloc")]
    state_tests!(
        RefMutSizedEncodeStoreUnchecked,
        unchecked_sized_drop,
        unchecked_sized_sync,
        unchecked_sized_panic,
        unchecked_sized_failure
    );
    #[cfg(feature = "alloc")]
    state_tests!(
        RefMutDstEncodeStore,
        dst_drop,
        dst_sync,
        dst_panic,
        dst_failure
    );

    #[cfg(feature = "alloc")]
    #[test]
    fn wide_store_returns_custom_metadata() {
        type Pointee = Custom;
        let mut store = RefWideEncodeStore::<Pointee, ()>::default();
        let carrier = store.encode(|_| {
            let bytes: Box<[u8]> = alloc::vec![2, 4].into_boxed_slice();
            let ptr = Box::into_raw(bytes) as *mut Pointee;
            unsafe { Box::from_raw(ptr) }
        });
        assert_eq!(
            carrier.metadata(),
            Metadata {
                length: 2,
                marker: 7
            }
        );
        assert_eq!(unsafe { &carrier.into_ref().unwrap().0 }, &[2, 4]);
        assert_eq!(store.sync(), Some(()));
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn mutable_wide_store_returns_custom_metadata_and_syncs() {
        type Pointee = Custom;
        let mut result = 0;
        let mut store = RefMutWideEncodeStore::<Pointee, (), &mut u8>::default();
        let carrier = store.encode(
            |_| {
                let bytes = alloc::vec![2_u8, 4].into_boxed_slice();
                let ptr = Box::into_raw(bytes) as *mut Pointee;
                (unsafe { Box::from_raw(ptr) }, &mut result)
            },
            |value, result| *result = value.0[0],
        );
        assert_eq!(
            carrier.metadata(),
            Metadata {
                length: 2,
                marker: 7
            }
        );
        unsafe { carrier.into_mut().unwrap().0[0] = 9 };
        assert_eq!(store.sync(), Some(()));
        assert_eq!(result, 9);
    }

    #[test]
    fn aggregate_stores_sync_every_child_in_order() {
        struct Probe<'a> {
            calls: &'a RefCell<([usize; 3], usize)>,
            index: usize,
            fail: bool,
        }

        impl Store for Probe<'_> {
            fn sync(self) -> Option<()> {
                let mut calls = self.calls.borrow_mut();
                let next = calls.1;
                calls.0[next] = self.index;
                calls.1 += 1;
                (!self.fail).then_some(())
            }
        }

        for failure in [None, Some(0), Some(1), Some(2)] {
            let calls = RefCell::new(([0; 3], 0));
            let probe = |index| Probe {
                calls: &calls,
                index,
                fail: failure == Some(index),
            };
            let expected = failure.is_none().then_some(());

            assert_eq!((probe(0), (probe(1), probe(2))).sync(), expected);
            assert_eq!(*calls.borrow(), ([0, 1, 2], 3));
            *calls.borrow_mut() = ([0; 3], 0);

            assert_eq!(ArrayStore([probe(0), probe(1), probe(2)]).sync(), expected);
            assert_eq!(*calls.borrow(), ([0, 1, 2], 3));

            #[cfg(feature = "alloc")]
            {
                *calls.borrow_mut() = ([0; 3], 0);
                let stores = Vec::from([probe(0), probe(1), probe(2)]).into_boxed_slice();
                assert_eq!(stores.sync(), expected);
                assert_eq!(*calls.borrow(), ([0, 1, 2], 3));
            }
        }

        assert_eq!(ArrayStore::<(), 0>([]).sync(), Some(()));
        #[cfg(feature = "alloc")]
        assert_eq!(Vec::<()>::new().into_boxed_slice().sync(), Some(()));
    }

    #[test]
    fn tuple_writeback_continues_after_invalid_field() {
        let (mut first, mut second) = (false, false);
        let mut store = Default::default();
        let encoded =
            unsafe { crate::soft_encode::<_, lang::C>((&mut first, &mut second), &mut store) };
        unsafe {
            // Both carriers point into live conversion stores. Only the first
            // carrier contains an invalid Rust bool representation.
            *encoded.0.0.as_ptr() = crate::primitive::abi::bool(2);
            *encoded.1.as_ptr() = crate::primitive::abi::bool::TRUE;
        }

        assert_eq!(store.sync(), None);
        assert!(!first);
        assert!(second);
    }

    #[test]
    #[should_panic(expected = "encoding failed")]
    fn failed_build_clears_carrier_and_allows_reuse() {
        struct CheckReuse(RefSizedEncodeStore<u32, ()>);

        impl Drop for CheckReuse {
            fn drop(&mut self) {
                assert!(self.0.ctype.is_none());
                let encoded = self.0.encode(|_| 11);
                assert_eq!(unsafe { *encoded.as_ptr() }, 11);
            }
        }

        let mut store = CheckReuse(RefSizedEncodeStore::default());
        store.0.encode(|_| 7);
        store.0.encode(|_| panic!("encoding failed"));
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
