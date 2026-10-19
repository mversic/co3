#[cfg(feature = "alloc")]
use alloc::{borrow::ToOwned, boxed::Box, vec::Vec};

use crate::ReprC;
#[cfg(feature = "alloc")]
use crate::slice::CWideRefMut;
#[cfg(feature = "alloc")]
use crate::stored::AssignFromOwned;
use crate::stored::{DecodeOwned, EmptyStore, EncodeOwned, decode_owned, encode_owned};

use crate::stored::Store;

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

pub struct RefSizedEncodeStore<R: ReprC<CType: Sized>, S: Store> {
    pub(crate) ctype: Option<R::CType>,
    pub(crate) store: S,
}

#[cfg(feature = "alloc")]
pub struct RefDstEncodeStore<R: ToOwned<Owned: ReprC<CType: Sized>> + ?Sized, S: Store> {
    pub(crate) ctype: Option<<R::Owned as ReprC>::CType>,
    pub(crate) store: S,
}

pub struct RefMutSizedEncodeStore<'d, R: ReprC<CType: Sized>, S: Store> {
    pub(crate) ctype: Option<R::CType>,
    pub(crate) store: S,
    pub(crate) original: Option<&'d mut R>,
}

#[cfg(feature = "alloc")]
pub struct RefMutDstEncodeStore<'d, R: ToOwned<Owned: ReprC<CType: Sized>> + ?Sized, S: Store> {
    pub(crate) ctype: Option<<R::Owned as ReprC>::CType>,
    pub(crate) store: S,
    pub(crate) original: Option<&'d mut R>,
}

pub struct RefSizedDecodeStore<R, S> {
    pub(crate) value: Option<R>,
    pub(crate) store: S,
}

pub struct RefMutSizedDecodeStore<R: ReprC, S> {
    pub(crate) value: Option<R>,
    pub(crate) store: S,
    pub(crate) source: Option<*mut R::CType>,
}

#[cfg(feature = "alloc")]
pub struct RefDstDecodeStore<R: ToOwned + ?Sized, S> {
    pub(crate) value: Option<R::Owned>,
    pub(crate) store: S,
}

#[cfg(feature = "alloc")]
pub struct RefMutSliceDecodeStore<R: ReprC<CType: Sized>, S> {
    pub(crate) value: Option<Vec<R>>,
    pub(crate) store: S,
    pub(crate) source: Option<CWideRefMut<[R::CType], true>>,
}

/// This struct exists only because [arrays don't yet implement Default](https://github.com/rust-lang/rust/issues/61415)
pub struct ArrayStore<D, const N: usize>(pub(crate) [D; N]);

impl<R: ReprC<CType: Sized>, S: Store + Default> Default for RefSizedEncodeStore<R, S> {
    fn default() -> Self {
        Self {
            ctype: None,
            store: Default::default(),
        }
    }
}

impl<R: ReprC<CType: Sized>, S: Store> Store for RefSizedEncodeStore<R, S> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

#[cfg(feature = "alloc")]
impl<R: ToOwned<Owned: ReprC<CType: Sized>> + ?Sized, S: Store + Default> Default
    for RefDstEncodeStore<R, S>
{
    fn default() -> Self {
        Self {
            ctype: None,
            store: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: ToOwned<Owned: ReprC<CType: Sized>> + ?Sized, S: Store> Store for RefDstEncodeStore<R, S> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

impl<'d, R: ReprC<CType: Sized>, S: Store + Default> Default for RefMutSizedEncodeStore<'d, R, S> {
    fn default() -> Self {
        Self {
            ctype: Default::default(),
            store: Default::default(),
            original: Default::default(),
        }
    }
}

// TODO: I'd bet the store doesn't have to be empty on decode during sync
// it should be possible to traverse the ctype and update current type.
impl<'d, R, S> Store for RefMutSizedEncodeStore<'d, R, S>
where
    R: DecodeOwned<'d, Store: EmptyStore + 'd>,
    S: Store,
{
    fn sync(self) -> Option<()> {
        let original = self.original?;
        self.store.sync()?;
        let ctype = self.ctype?;

        let value = unsafe { decode_owned(ctype)? };
        *original = value;
        Some(())
    }
}

#[cfg(feature = "alloc")]
impl<'d, R: ToOwned<Owned: ReprC<CType: Sized>> + ?Sized, S: Store + Default> Default
    for RefMutDstEncodeStore<'d, R, S>
{
    fn default() -> Self {
        Self {
            ctype: Default::default(),
            store: Default::default(),
            original: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<'d, R: AssignFromOwned, S: Store> Store for RefMutDstEncodeStore<'d, R, S>
where
    R: ToOwned<Owned: DecodeOwned<'d, Store: EmptyStore>> + ?Sized,
{
    fn sync(self) -> Option<()> {
        let original = self.original?;
        self.store.sync()?;
        let ctype = self.ctype?;

        let owned = unsafe { decode_owned::<'d, R::Owned>(ctype)? };
        original.assign_from_owned(owned)
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

impl<R: ReprC, S: Default> Default for RefMutSizedDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
            source: None,
        }
    }
}

impl<R, S: Store> Store for RefSizedDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

impl<R: EncodeOwned<Store: EmptyStore>, S: Store> Store for RefMutSizedDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        let source = self.source?;
        self.store.sync()?;
        let value = self.value?;
        unsafe { source.write(encode_owned(value)) };
        Some(())
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
impl<R: ToOwned + ?Sized, S: Store> Store for RefDstDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

#[cfg(feature = "alloc")]
impl<R: EncodeOwned<CType: Sized, Store: EmptyStore>, S: Store> Store
    for RefMutSliceDecodeStore<R, S>
{
    fn sync(self) -> Option<()> {
        let source = self.source?;
        self.store.sync()?;
        let value = self.value?;

        if source.len() != value.len() {
            return None;
        }

        let source = unsafe { source.into_mut()? };
        for (destination, value) in source.iter_mut().zip(value) {
            *destination = encode_owned(value);
        }
        Some(())
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
