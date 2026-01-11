//! FFI-safe equivalent of [`core::result`] related functionality

use crate::{
    Decode, Encode, ExternC, ReprC,
    ir::Ir,
    mineral,
    niche::{Ir as NicheIr, Niche},
};

pub use private::{Result as CResult, ResultPayload as CResultPayload};

mod private {
    use super::*;

    /// FFI-safe equivalent of [`core::result::Result`]
    #[repr(C)]
    pub struct Result<T: ReprC, E: ReprC> {
        pub tag: u8,
        pub payload: ResultPayload<T, E>,
    }

    /// Payload of [`Result`]
    #[repr(C)]
    #[expect(non_snake_case)]
    pub union ResultPayload<T: ReprC, E: ReprC> {
        pub Ok: T,
        pub Err: E,
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

impl<T, E> Ir for Result<T, E> {
    type Type = Self;
}

impl<T, E> NicheIr for Result<T, E>
where
    (T, E): NicheIr,
{
    type Type = <(T, E) as NicheIr>::Type;
}

impl<T: ExternC, E: ExternC> Niche for Result<T, E> {
    const NICHE_VALUE: Self::CType = CResult {
        tag: 2,
        payload: unsafe { core::mem::zeroed() },
    };
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
            Ok(ok) => CResult {
                tag: 0,
                payload: CResultPayload {
                    Ok: ok.encode(&mut store.0),
                },
            },
            Err(err) => CResult {
                tag: 1,
                payload: CResultPayload {
                    Err: err.encode(&mut store.1),
                },
            },
        }
    }
}

impl<'d, T: Decode<'d>, E: Decode<'d>> Decode<'d> for Result<T, E> {
    type Store = (T::Store, E::Store);

    unsafe fn decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> crate::Result<Self> {
        match source.tag {
            0 => Ok(Ok(unsafe { T::decode(source.payload.Ok, &mut store.0)? })),
            1 => Ok(Err(unsafe { E::decode(source.payload.Err, &mut store.1)? })),
            _ => Err(crate::FfiReturn::TrapRepresentation),
        }
    }
}
