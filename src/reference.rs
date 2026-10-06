//! ABI carriers for mutable pointers.

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::{Borrow, FromBorrow},
    stored::{DecodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
};

/// A mutable pointer carrier whose access contract is selected by `RESTRICTED`.
///
/// The default, `true`, represents exclusive mutable access. Set it to `false`
/// for pointers derived from shared references to interior mutable values.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CRefMut<C: ?Sized, const RESTRICTED: bool = true> {
    data: *mut C,
}

impl<C: ?Sized, const RESTRICTED: bool> CRefMut<C, RESTRICTED> {
    /// Wrap a pointer in a mutable ABI carrier.
    ///
    /// # Safety
    ///
    /// The pointer must satisfy the validity and aliasing rules of the source
    /// reference. If `RESTRICTED` is `true`, accesses must be exclusive.
    pub const unsafe fn from_raw(data: *mut C) -> Self {
        Self { data }
    }

    pub const fn from_mut(data: &mut C) -> Self {
        Self { data }
    }

    pub const fn as_ptr(self) -> *mut C {
        self.data
    }
}

impl<C, const RESTRICTED: bool> CRefMut<C, RESTRICTED> {
    pub(crate) const NICHE: Self = Self {
        data: core::ptr::null_mut(),
    };
}

impl<C: ?Sized, const RESTRICTED: bool> core::fmt::Debug for CRefMut<C, RESTRICTED> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("CRefMut").field(&self.data).finish()
    }
}

impl<C: ?Sized, const RESTRICTED: bool> Clone for CRefMut<C, RESTRICTED> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C: ?Sized, const RESTRICTED: bool> Copy for CRefMut<C, RESTRICTED> {}

impl<C: ?Sized, const RESTRICTED: bool> PartialEq for CRefMut<C, RESTRICTED> {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.data, other.data)
    }
}
impl<C: ?Sized, const RESTRICTED: bool> Eq for CRefMut<C, RESTRICTED> {}
impl<C, const RESTRICTED: bool> PartialOrd for CRefMut<C, RESTRICTED> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<C, const RESTRICTED: bool> Ord for CRefMut<C, RESTRICTED> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.data.cmp(&other.data)
    }
}

unsafe impl<C: ?Sized, const RESTRICTED: bool> Borrow for CRefMut<C, RESTRICTED> {
    type Borrowed<'itm>
        = Self
    where
        Self: 'itm;
    type Owner = ();

    fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self
    }
}

impl<'itm, C: ?Sized, const RESTRICTED: bool> FromBorrow<'itm> for CRefMut<C, RESTRICTED> {
    fn from_borrow(source: Self) -> Self {
        source
    }
}

impl<C: CType + ?Sized, const RESTRICTED: bool> ReprC for CRefMut<C, RESTRICTED> {
    type CType = Self;
}

unsafe impl<C: CType + ?Sized, const RESTRICTED: bool> EncodeOwned for CRefMut<C, RESTRICTED> {
    type Store = ();

    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        self
    }
}

unsafe impl<'d, C: CType + ?Sized, const RESTRICTED: bool> DecodeOwned<'d>
    for CRefMut<C, RESTRICTED>
{
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(source)
    }
}

impl<C: CType + ?Sized, const RESTRICTED: bool> Encode for CRefMut<C, RESTRICTED> {}
impl<'d, C: CType + ?Sized, const RESTRICTED: bool> Decode<'d> for CRefMut<C, RESTRICTED> {}

unsafe impl<C: CType + ?Sized, const RESTRICTED: bool> CheckedTransmute for CRefMut<C, RESTRICTED> {
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}

unsafe impl<C: CType + ?Sized, const RESTRICTED: bool> CType for CRefMut<C, RESTRICTED> {}
unsafe impl<C: CType, const RESTRICTED: bool> CFnArg for CRefMut<C, RESTRICTED> {}
unsafe impl<C: CType, const RESTRICTED: bool> CFnReturn for CRefMut<C, RESTRICTED> {}

#[cfg(test)]
mod tests {
    use core::cell::UnsafeCell;

    use static_assertions::assert_impl_all;

    use super::*;
    use crate::{niche::Niche, slice::CSliceMut};

    #[test]
    fn exclusive_and_shared_mutable_references_have_distinct_carriers() {
        assert_impl_all!(&mut u8: Niche<CType = CRefMut<u8>>);
        assert_impl_all!(&UnsafeCell<u8>: Niche<CType = CRefMut<u8, false>>);
        assert_impl_all!(&mut UnsafeCell<u8>: Niche<CType = CRefMut<u8>>);
        assert_impl_all!(&mut [u8]: Niche<CType = CSliceMut<u8>>);
        assert_impl_all!(&[UnsafeCell<u8>]: Niche<CType = CSliceMut<u8, false>>);
        assert_impl_all!(&mut [UnsafeCell<u8>]: Niche<CType = CSliceMut<u8>>);

        assert_eq!(
            core::mem::size_of::<CRefMut<u8>>(),
            core::mem::size_of::<*mut u8>()
        );
        assert_eq!(
            core::mem::align_of::<CRefMut<u8>>(),
            core::mem::align_of::<*mut u8>()
        );

        let mut cell = UnsafeCell::new(1_u8);
        let shared: CRefMut<u8, false> = crate::encode(&cell);
        let exclusive: CRefMut<u8> = crate::encode(&mut cell);
        assert_eq!(shared.as_ptr(), exclusive.as_ptr());
        unsafe { exclusive.as_ptr().write(2) };
        assert_eq!(cell.into_inner(), 2);
    }
}
