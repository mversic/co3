//! ABI carriers for exclusive mutable pointers.

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::{Borrow, FromBorrow},
    stored::{DecodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
};

/// A mutable pointer whose referent is exclusively accessible for the borrow.
///
/// Header generators can render this as `C *restrict` where the qualifier has
/// meaning, while preserving its pointer ABI.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CRestrict<C: ?Sized> {
    data: *mut C,
}

impl<C: ?Sized> CRestrict<C> {
    /// Wrap a pointer for an exclusive mutable ABI borrow.
    ///
    /// # Safety
    ///
    /// If the pointer is used to access memory during the borrow, those accesses
    /// must satisfy the exclusivity promised by the corresponding `&mut` value.
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

impl<C> CRestrict<C> {
    pub(crate) const NICHE: Self = Self {
        data: core::ptr::null_mut(),
    };
}

impl<C: ?Sized> core::fmt::Debug for CRestrict<C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("CRestrict").field(&self.data).finish()
    }
}

impl<C: ?Sized> Clone for CRestrict<C> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C: ?Sized> Copy for CRestrict<C> {}

impl<C: ?Sized> PartialEq for CRestrict<C> {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.data, other.data)
    }
}
impl<C: ?Sized> Eq for CRestrict<C> {}
impl<C> PartialOrd for CRestrict<C> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<C> Ord for CRestrict<C> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.data.cmp(&other.data)
    }
}

unsafe impl<C: ?Sized> Borrow for CRestrict<C> {
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

impl<'itm, C: ?Sized> FromBorrow<'itm> for CRestrict<C> {
    fn from_borrow(source: Self) -> Self {
        source
    }
}

impl<C: CType + ?Sized> ReprC for CRestrict<C> {
    type CType = Self;
}

unsafe impl<C: CType + ?Sized> EncodeOwned for CRestrict<C> {
    type Store = ();

    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        self
    }
}

unsafe impl<'d, C: CType + ?Sized> DecodeOwned<'d> for CRestrict<C> {
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(source)
    }
}

impl<C: CType + ?Sized> Encode for CRestrict<C> {}
impl<'d, C: CType + ?Sized> Decode<'d> for CRestrict<C> {}

unsafe impl<C: CType + ?Sized> CheckedTransmute for CRestrict<C> {
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}

unsafe impl<C: CType + ?Sized> CType for CRestrict<C> {}
unsafe impl<C: CType> CFnArg for CRestrict<C> {}
unsafe impl<C: CType> CFnReturn for CRestrict<C> {}

#[cfg(test)]
mod tests {
    use core::cell::UnsafeCell;

    use static_assertions::assert_impl_all;

    use super::*;
    use crate::{
        niche::Niche,
        slice::{CSliceMut, CSliceRestrict},
    };

    #[test]
    fn exclusive_and_shared_mutable_references_have_distinct_carriers() {
        assert_impl_all!(&mut u8: Niche<CType = CRestrict<u8>>);
        assert_impl_all!(&UnsafeCell<u8>: Niche<CType = *mut u8>);
        assert_impl_all!(&mut UnsafeCell<u8>: Niche<CType = CRestrict<u8>>);
        assert_impl_all!(&mut [u8]: Niche<CType = CSliceRestrict<u8>>);
        assert_impl_all!(&[UnsafeCell<u8>]: Niche<CType = CSliceMut<u8>>);
        assert_impl_all!(&mut [UnsafeCell<u8>]: Niche<CType = CSliceRestrict<u8>>);

        assert_eq!(
            core::mem::size_of::<CRestrict<u8>>(),
            core::mem::size_of::<*mut u8>()
        );
        assert_eq!(
            core::mem::align_of::<CRestrict<u8>>(),
            core::mem::align_of::<*mut u8>()
        );

        let mut cell = UnsafeCell::new(1_u8);
        let shared: *mut u8 = crate::encode(&cell);
        let exclusive: CRestrict<u8> = crate::encode(&mut cell);
        assert_eq!(shared, exclusive.as_ptr());
        unsafe { exclusive.as_ptr().write(2) };
        assert_eq!(cell.into_inner(), 2);
    }
}
