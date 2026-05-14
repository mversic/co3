//! FFI-safe equivalent of [`core::result`] related functionality

use crate::{ReprC, borrow::BorrowCast, reprC};

/// FFI-safe equivalent of [`core::result::Result`]
#[repr(C)]
pub union CResult<T: Copy, E: Copy> {
    ok: CResultOk<T>,
    err: CResultErr<E>,
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

    #[inline(always)]
    fn tag(self) -> u8 {
        // SAFETY: Variant structs have tag as the first field
        unsafe { core::ptr::from_ref(&self).cast::<u8>().read() }
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
        match value.tag() {
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
    unsafe impl(T: ReprC + Copy, E: ReprC + Copy) SizedRobust for CResult<T, E> {}
}

unsafe impl<
    T: BorrowCast<AsConst: Copy, AsMut: Copy> + Copy,
    E: BorrowCast<AsConst: Copy, AsMut: Copy> + Copy,
> BorrowCast for CResult<T, E>
{
    type AsConst = CResult<T::AsConst, E::AsConst>;
    type AsMut = CResult<T::AsMut, E::AsMut>;
}
