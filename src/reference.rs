//! ABI carriers for pointers derived from references.

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    stored::{DecodeOwned, EncodeOwned},
};

/// A shared reference pointer carrier with the same ABI as `*const C`.
///
/// A null pointer represents an absent reference.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CRef<C: ?Sized> {
    data: *const C,
}

impl<C: ?Sized> CRef<C> {
    /// Wrap a pointer that follows the validity and aliasing rules of a shared reference.
    ///
    /// # Safety
    /// A non-null pointer must refer to a live `C` for any later dereference.
    pub const unsafe fn from_raw(data: *const C) -> Self {
        Self { data }
    }

    /// Wrap an existing shared reference to lowered data.
    pub const fn from_ref(data: &C) -> Self {
        Self { data }
    }

    pub const fn as_ptr(self) -> *const C {
        self.data
    }
}

impl<C> CRef<C> {
    /// Construct a carrier with a null data pointer.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null(),
        }
    }
}

impl<C: ?Sized> core::fmt::Debug for CRef<C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("CRef").field(&self.data).finish()
    }
}
impl<C: ?Sized> Clone for CRef<C> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C: ?Sized> Copy for CRef<C> {}
impl<C: ?Sized> PartialEq for CRef<C> {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.data, other.data)
    }
}
impl<C: ?Sized> Eq for CRef<C> {}
impl<C> PartialOrd for CRef<C> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<C> Ord for CRef<C> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.data.cmp(&other.data)
    }
}

unsafe impl<C: ?Sized> Borrow for CRef<C> {
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
impl<'itm, C: ?Sized> FromBorrow<'itm> for CRef<C> {
    fn from_borrow(source: Self) -> Self {
        source
    }
}
impl<C: CType + ?Sized> ReprC for CRef<C> {
    type CType = Self;
}
unsafe impl<C: CType + ?Sized> EncodeOwned for CRef<C> {
    type Store = ();

    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        self
    }
}
unsafe impl<'d, C: CType + ?Sized> DecodeOwned<'d> for CRef<C> {
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(source)
    }
}
impl<C: CType + ?Sized> Encode for CRef<C> {}
impl<'d, C: CType + ?Sized> Decode<'d> for CRef<C> {}
unsafe impl<C: CType + ?Sized> CType for CRef<C> {}
unsafe impl<C: CType> CFnArg for CRef<C> {}
unsafe impl<C: CType> CFnReturn for CRef<C> {}
unsafe impl<C: CType + ?Sized> BorrowCast for CRef<C> {
    type AsConst = Self;
}
unsafe impl<C: CType + ?Sized> BorrowCastMut for CRef<C> {
    type AsMut = Self;
}

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

    /// Wrap an existing mutable reference to lowered data.
    pub const fn from_mut(data: &mut C) -> Self {
        Self { data }
    }

    pub const fn as_ptr(self) -> *mut C {
        self.data
    }
}

impl<C, const RESTRICTED: bool> CRefMut<C, RESTRICTED> {
    /// Construct a carrier with a null data pointer.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null_mut(),
        }
    }
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
    fn shared_reference_carrier_preserves_pointer_abi() {
        assert_impl_all!(&u8: Niche<CType = CRef<u8>>, Encode, Decode<'static>);
        assert_impl_all!(&CRef<u8>: Niche<CType = CRef<CRef<u8>>>, Encode, Decode<'static>);
        assert_eq!(
            core::mem::size_of::<CRef<u8>>(),
            core::mem::size_of::<*const u8>()
        );
        assert_eq!(
            core::mem::align_of::<CRef<u8>>(),
            core::mem::align_of::<*const u8>()
        );

        let value = 7_u8;
        let carrier = crate::encode(&value);
        assert_eq!(carrier, CRef::from_ref(&value));
        assert_eq!(unsafe { crate::decode::<&u8>(carrier) }, Some(&value));
    }

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
