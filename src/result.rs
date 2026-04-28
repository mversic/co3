//! FFI-safe equivalent of [`core::result`] related functionality

use crate::{
    DecodeWithStore, EncodeWithStore, ExternC, ReprC, Store,
    borrow::DropFamily,
    cloned::DecodeCloned,
    niche::{Niche, NicheFamily},
    reprC,
};

/// FFI-safe equivalent of [`core::result::Result`]
#[repr(C)]
pub union CResult<T: Copy, E: Copy> {
    ok: CResultOk<T>,
    err: CResultErr<E>,
    // TODO: Consider using:
    // ok: ManuallyDrop<CResultOk<T>>,
    // err: ManuallyDrop<CResultErr<E>>,
}

#[repr(C)]
struct CResultOk<T>(u8, T);

#[repr(C)]
struct CResultErr<E>(u8, E);

impl<T: Copy, E: Copy> CResult<T, E> {
    /// Construct the success value
    #[expect(non_snake_case)]
    pub const fn Ok(ok: T) -> Self {
        Self {
            ok: CResultOk(0, ok),
        }
    }

    /// Construct the error value
    #[expect(non_snake_case)]
    pub const fn Err(err: E) -> Self {
        Self {
            err: CResultErr(1, err),
        }
    }

    pub(crate) const fn niche() -> Self {
        Self {
            ok: CResultOk(2, unsafe { core::mem::zeroed() }),
        }
    }
}

impl<T: Copy, E: Copy> From<Result<T, E>> for CResult<T, E> {
    fn from(value: Result<T, E>) -> Self {
        match value {
            Ok(ok) => Self::Ok(ok),
            Err(err) => Self::Err(err),
        }
    }
}

impl<T: Copy, E: Copy> TryFrom<CResult<T, E>> for Result<T, E> {
    type Error = crate::FfiReturn;

    fn try_from(value: CResult<T, E>) -> Result<Self, Self::Error> {
        // SAFETY: Both variant structs have tag as the first field at offset 0
        let tag = unsafe { core::ptr::from_ref(&value).cast::<u8>().read() };

        match tag {
            0 => Ok(Ok(unsafe { value.ok.1 })),
            1 => Ok(Err(unsafe { value.err.1 })),
            _ => Err(crate::FfiReturn::TrapRepresentation),
        }
    }
}

impl<T: Copy, E: Copy> Copy for CResult<T, E> {}
impl<T: Copy, E: Copy> Clone for CResult<T, E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Copy> Copy for CResultOk<T> {}
impl<T: Copy> Clone for CResultOk<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E: Copy> Copy for CResultErr<E> {}
impl<E: Copy> Clone for CResultErr<E> {
    fn clone(&self) -> Self {
        *self
    }
}

reprC! {
    unsafe impl(T: ReprC, E: ReprC) SizedRobust for CResult<T, E> {}
}

reprC! {
    // FIXME: Result is transparent if one param is ZST
    // https://github.com/mversic/co3/issues/34
    impl(T, E) SizedCloned for Result<T, E> {}
}

impl<T, E> NicheFamily for Result<T, E>
where
    (T, E): NicheFamily,
{
    type Kind = <(T, E) as NicheFamily>::Kind;
}

impl<T: ExternC, E: ExternC> ExternC for Result<T, E> {
    type CType = CResult<T::CType, E::CType>;
}

impl<T: ExternC, E: ExternC> Niche for Result<T, E> {
    const NICHE_VALUE: Self::CType = CResult::niche();
}

impl<T, E> DropFamily for Result<T, E>
where
    T: DropFamily,
    E: DropFamily,
    T::Kind: core::ops::Add<E::Kind>,
{
    type Kind = <T::Kind as core::ops::Add<E::Kind>>::Output;
}

impl<T: EncodeWithStore, E: EncodeWithStore> EncodeWithStore for Result<T, E> {
    type Store = (T::Store, E::Store);

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

impl<'d, T: DecodeWithStore<'d>, E: DecodeWithStore<'d>> DecodeWithStore<'d> for Result<T, E> {
    type Store = Option<Result<T::Store, E::Store>>;

    unsafe fn decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
        let value = match TryInto::<Result<_, _>>::try_into(source).ok()? {
            Ok(ok) => {
                let ok_store = store.insert(Ok(Default::default()));
                let ok_store = unsafe { ok_store.as_mut().unwrap_unchecked() };
                Ok(unsafe { T::decode(ok, ok_store)? })
            }
            Err(err) => {
                let err_store = store.insert(Err(Default::default()));
                let err_store = unsafe { err_store.as_mut().unwrap_err_unchecked() };
                Err(unsafe { E::decode(err, err_store)? })
            }
        };

        Some(value)
    }
}

impl<'d, T: DecodeCloned<'d>, E: DecodeCloned<'d>> DecodeCloned<'d> for Result<T, E> {
    unsafe fn decode_cloned<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        let value = match TryInto::<Result<_, _>>::try_into(source).ok()? {
            Ok(ok) => {
                let ok_store = store.insert(Ok(Default::default()));
                Ok(unsafe { T::decode_cloned(ok, ok_store.as_mut().unwrap_unchecked())? })
            }
            Err(err) => {
                let err_store = store.insert(Err(Default::default()));
                Err(unsafe { E::decode_cloned(err, err_store.as_mut().unwrap_err_unchecked())? })
            }
        };

        Some(value)
    }
}

impl<T: Store, E: Store> Store for Option<Result<T, E>> {
    fn sync(self) -> Option<()> {
        match self.unwrap() {
            Ok(ok) => ok.sync(),
            Err(err) => err.sync(),
        }
    }
}
