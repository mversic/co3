//! ABI carriers for pointers derived from references.

use core::cell::UnsafeCell;
use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    stored::{DecodeOwned, DirectEncodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
};

const fn assert_thin_pointer<C: ?Sized>() {
    const {
        assert!(
            core::mem::size_of::<*const C>() == core::mem::size_of::<*const ()>(),
            "reference carrier requires a thin pointer"
        );
    }
}

/// A thin shared reference pointer carrier with the same ABI as `*const C`.
///
/// A null pointer represents an absent reference.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CRef<C: ?Sized> {
    data: *const C,
}

impl<C: CType + ?Sized> CRef<C> {
    /// Wrap a pointer that follows the validity and aliasing rules of a shared reference.
    ///
    /// # Safety
    ///
    /// A non-null thin pointer must refer to a live `C` for any later dereference.
    pub const unsafe fn from_raw(data: *const C) -> Self {
        assert_thin_pointer::<C>();
        Self { data }
    }

    /// Wrap an existing shared reference to lowered data.
    pub const fn from_ref(data: &C) -> Self {
        assert_thin_pointer::<C>();
        Self { data }
    }
}

impl<C: ?Sized> CRef<C> {
    pub const fn as_ptr(self) -> *const C {
        self.data
    }

    /// Recover the shared reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// A non-null pointer must refer to a live `C` for the returned lifetime.
    pub unsafe fn into_ref<'a>(self) -> Option<&'a C> {
        if self.data.is_null() {
            None
        } else {
            Some(unsafe { &*self.data })
        }
    }
}

impl<C: CType> CRef<C> {
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
impl<C: CType> PartialOrd for CRef<C> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<C: CType> Ord for CRef<C> {
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
        unsafe { self.soft_encode_unchecked(&mut ()) }
    }
}
unsafe impl<C: CType + ?Sized> DirectEncodeOwned for CRef<C> {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
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
impl<C: CType + ?Sized> crate::DirectEncode for CRef<C> {}
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

/// A thin mutable pointer carrier with a defined C ABI layout.
///
/// - `EXCLUSIVE = true` represents exclusive access that comes from `&mut T`.
/// - `EXCLUSIVE = false` represents access that comes from `&UnsafeCell<T>`.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CRefMut<C: ?Sized, const EXCLUSIVE: bool> {
    data: *mut C,
}

impl<C: CType + ?Sized, const EXCLUSIVE: bool> CRefMut<C, EXCLUSIVE> {
    /// Wrap a pointer in a mutable ABI carrier.
    ///
    /// # Safety
    ///
    /// The thin pointer must satisfy the validity and aliasing rules of the source
    /// reference. If `EXCLUSIVE` is `true`, accesses must be exclusive.
    pub const unsafe fn from_raw(data: *mut C) -> Self {
        assert_thin_pointer::<C>();
        Self { data }
    }
}

impl<C: CType + ?Sized> CRefMut<C, true> {
    /// Wrap an existing mutable reference to lowered data.
    pub const fn from_mut(data: &mut C) -> Self {
        assert_thin_pointer::<C>();
        Self { data }
    }
}

impl<C: ?Sized, const EXCLUSIVE: bool> CRefMut<C, EXCLUSIVE> {
    pub const fn as_ptr(self) -> *mut C {
        self.data
    }
}

impl<C: ?Sized> CRefMut<C, true> {
    /// Recover the mutable reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer must refer to a live `C` that is exclusively accessible
    /// for the returned lifetime.
    pub unsafe fn into_mut<'a>(self) -> Option<&'a mut C> {
        if self.data.is_null() {
            None
        } else {
            Some(unsafe { &mut *self.data })
        }
    }
}

impl<C, const EXCLUSIVE: bool> CRefMut<C, EXCLUSIVE> {
    /// Construct a carrier with a null data pointer.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null_mut(),
        }
    }
}

impl<C: CType + ?Sized> CRefMut<C, false> {
    /// Create a carrier for shared access with interior mutability.
    pub const fn from_ref(cell: &UnsafeCell<C>) -> Self {
        assert_thin_pointer::<C>();
        Self { data: cell.get() }
    }
}

impl<C: ?Sized, const EXCLUSIVE: bool> core::fmt::Debug for CRefMut<C, EXCLUSIVE> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("CRefMut").field(&self.data).finish()
    }
}

impl<C: ?Sized, const EXCLUSIVE: bool> Clone for CRefMut<C, EXCLUSIVE> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C: ?Sized, const EXCLUSIVE: bool> Copy for CRefMut<C, EXCLUSIVE> {}

impl<C: ?Sized, const EXCLUSIVE: bool> PartialEq for CRefMut<C, EXCLUSIVE> {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.data, other.data)
    }
}
impl<C: ?Sized, const EXCLUSIVE: bool> Eq for CRefMut<C, EXCLUSIVE> {}
impl<C: CType, const EXCLUSIVE: bool> PartialOrd for CRefMut<C, EXCLUSIVE> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<C: CType, const EXCLUSIVE: bool> Ord for CRefMut<C, EXCLUSIVE> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.data.cmp(&other.data)
    }
}
unsafe impl<C: ?Sized, const EXCLUSIVE: bool> Borrow for CRefMut<C, EXCLUSIVE> {
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
impl<'itm, C: ?Sized, const EXCLUSIVE: bool> FromBorrow<'itm> for CRefMut<C, EXCLUSIVE> {
    fn from_borrow(source: Self) -> Self {
        source
    }
}

impl<C: CType + ?Sized, const EXCLUSIVE: bool> ReprC for CRefMut<C, EXCLUSIVE> {
    type CType = Self;
}

unsafe impl<C: CType + ?Sized, const EXCLUSIVE: bool> EncodeOwned for CRefMut<C, EXCLUSIVE> {
    type Store = ();

    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { self.soft_encode_unchecked(&mut ()) }
    }
}
unsafe impl<C: CType + ?Sized, const EXCLUSIVE: bool> DirectEncodeOwned for CRefMut<C, EXCLUSIVE> {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
        self
    }
}

unsafe impl<'d, C: CType + ?Sized, const EXCLUSIVE: bool> DecodeOwned<'d>
    for CRefMut<C, EXCLUSIVE>
{
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(source)
    }
}

impl<C: CType + ?Sized, const EXCLUSIVE: bool> Encode for CRefMut<C, EXCLUSIVE> {}
impl<C: CType + ?Sized, const EXCLUSIVE: bool> crate::DirectEncode for CRefMut<C, EXCLUSIVE> {}
impl<'d, C: CType + ?Sized, const EXCLUSIVE: bool> Decode<'d> for CRefMut<C, EXCLUSIVE> {}

unsafe impl<C: CType + ?Sized, const EXCLUSIVE: bool> CType for CRefMut<C, EXCLUSIVE> {}
unsafe impl<C: CType, const EXCLUSIVE: bool> CFnArg for CRefMut<C, EXCLUSIVE> {}
unsafe impl<C: CType, const EXCLUSIVE: bool> CFnReturn for CRefMut<C, EXCLUSIVE> {}
unsafe impl<C: CType + ?Sized, const EXCLUSIVE: bool> BorrowCast for CRefMut<C, EXCLUSIVE> {
    type AsConst = Self;
}
unsafe impl<C: CType + ?Sized, const EXCLUSIVE: bool> BorrowCastMut for CRefMut<C, EXCLUSIVE> {
    type AsMut = Self;
}

unsafe impl<C: CType + ?Sized> CheckedTransmute for CRef<C> {
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}

unsafe impl<C: CType + ?Sized, const EXCLUSIVE: bool> CheckedTransmute for CRefMut<C, EXCLUSIVE> {
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use core::cell::UnsafeCell;

    use static_assertions::assert_impl_all;

    use super::*;
    use crate::{niche::Niche, slice::CWideRefMut};

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
        assert_impl_all!(&mut u8: Niche<CType = CRefMut<u8, true>>);
        assert_impl_all!(&UnsafeCell<u8>: Niche<CType = CRefMut<UnsafeCell<u8>, false>>);
        assert_impl_all!(&mut UnsafeCell<u8>: Niche<CType = CRefMut<UnsafeCell<u8>, true>>);
        assert_impl_all!(&mut [u8]: Niche<CType = CWideRefMut<[u8], true>>);
        assert_impl_all!(&[UnsafeCell<u8>]: Niche<CType = CWideRefMut<[UnsafeCell<u8>], false>>);
        assert_impl_all!(&mut [UnsafeCell<u8>]: Niche<CType = CWideRefMut<[UnsafeCell<u8>], true>>);

        assert_eq!(
            core::mem::size_of::<CRefMut<u8, true>>(),
            core::mem::size_of::<*mut u8>()
        );
        assert_eq!(
            core::mem::align_of::<CRefMut<u8, true>>(),
            core::mem::align_of::<*mut u8>()
        );

        let mut cell = UnsafeCell::new(1_u8);
        let exclusive: CRefMut<UnsafeCell<u8>, true> = crate::encode(&mut cell);
        assert_eq!(exclusive.as_ptr(), cell.get().cast::<UnsafeCell<u8>>());
        unsafe { exclusive.as_ptr().write(UnsafeCell::new(2)) };
        assert_eq!(cell.into_inner(), 2);
    }
}
