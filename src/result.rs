//! FFI-safe equivalent of [`core::result`] related functionality

use crate::{
    Decode, Encode, ExternC, ReprC,
    ir::ReprFamily,
    mineral,
    niche::{Niche, NicheFamily},
};

/// FFI-safe equivalent of [`core::result::Result`]
#[repr(C)]
pub struct CResult<T: ReprC, E: ReprC> {
    tag: u8,
    payload: CResultPayload<T, E>,
}

/// Payload of [`CResult`]
#[repr(C)]
#[expect(non_snake_case)]
union CResultPayload<T: ReprC, E: ReprC> {
    Ok: T,
    Err: E,
}

impl<T: ReprC, E: ReprC> CResult<T, E> {
    /// Construct the success value
    #[expect(non_snake_case)]
    pub const fn Ok(ok: T) -> Self {
        Self {
            tag: 0,
            payload: CResultPayload { Ok: ok },
        }
    }

    /// Construct the error value
    #[expect(non_snake_case)]
    pub const fn Err(err: E) -> Self {
        Self {
            tag: 1,
            payload: CResultPayload { Err: err },
        }
    }

    pub(crate) const fn niche() -> Self {
        Self {
            tag: 2,
            payload: unsafe { core::mem::zeroed() },
        }
    }
}

impl<T: ReprC, E: ReprC> From<Result<T, E>> for CResult<T, E> {
    fn from(value: Result<T, E>) -> Self {
        match value {
            Ok(ok) => Self::Ok(ok),
            Err(err) => Self::Err(err),
        }
    }
}

impl<T: ReprC, E: ReprC> TryFrom<CResult<T, E>> for Result<T, E> {
    type Error = crate::FfiReturn;

    fn try_from(value: CResult<T, E>) -> Result<Self, Self::Error> {
        match value.tag {
            0 => Ok(Ok(unsafe { value.payload.Ok })),
            1 => Ok(Err(unsafe { value.payload.Err })),
            _ => Err(crate::FfiReturn::TrapRepresentation),
        }
    }
}

impl<T: ReprC, E: ReprC> Copy for CResult<T, E> {}
impl<T: ReprC, E: ReprC> Clone for CResult<T, E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ReprC, E: ReprC> Copy for CResultPayload<T, E> {}
impl<T: ReprC, E: ReprC> Clone for CResultPayload<T, E> {
    fn clone(&self) -> Self {
        *self
    }
}

mineral! {
    unsafe impl(T: ReprC, E: ReprC) Robust for CResult<T, E> {}
}

mineral! {
    unsafe impl(T: ReprC, E: ReprC) Robust for CResultPayload<T, E> {}
}

impl<T, E> ReprFamily for Result<T, E> {
    type Kind = Self;
}

impl<T, E> NicheFamily for Result<T, E>
where
    (T, E): NicheFamily,
{
    type Kind = <(T, E) as NicheFamily>::Kind;
}

impl<T: ExternC, E: ExternC> Niche for Result<T, E> {
    const NICHE_VALUE: Self::CType = CResult::niche();
}

impl<T: ExternC, E: ExternC> ExternC for Result<T, E> {
    type CType = CResult<T::CType, E::CType>;
}

impl<T: Encode, E: Encode> Encode for Result<T, E> {
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

impl<'d, T: Decode<'d>, E: Decode<'d>> Decode<'d> for Result<T, E> {
    type Store = (T::Store, E::Store);

    unsafe fn decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> crate::Result<Self> {
        match TryInto::<Result<_, _>>::try_into(source)? {
            Ok(ok) => Ok(Ok(unsafe { T::decode(ok, &mut store.0)? })),
            Err(err) => Ok(Err(unsafe { E::decode(err, &mut store.1)? })),
        }
    }
}
