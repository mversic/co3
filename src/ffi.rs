//! Conversions for types in [`core::ffi`].

#[cfg(feature = "alloc")]
use alloc::ffi::CString;
use core::ffi::{CStr, c_char, c_void};

use crate::{
    CType, ReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
};
#[cfg(feature = "alloc")]
use crate::{Encode, boxed::CBox, niche::Niche, stored::EncodeOwned};

/// Pointer conversion for a nul-terminated pointee.
///
/// # Safety
///
/// `as_c_ptr` **MUST** return a pointer to the value's valid nul-terminated data.
pub unsafe trait NulTerminatedRef: ReprC<CType: Sized> {
    /// Returns a pointer to this value's nul-terminated data.
    ///
    /// The pointer must point to this value's valid data, including its terminating nul.
    fn as_c_ptr(&self) -> *const Self::CType;

    /// Borrows a nul-terminated value from a C pointer.
    ///
    /// # Safety
    ///
    /// `ptr` **MUST** be non-null and properly aligned. The initialized data through the first nul
    /// must be readable within one allocation, fit in `isize::MAX` bytes, and remain unchanged for
    /// `'a`.
    unsafe fn from_c_ptr<'a>(ptr: *const Self::CType) -> &'a Self;
}

unsafe impl Borrow for c_void {
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
impl<'itm> FromBorrow<'itm> for c_void {
    fn from_borrow(source: Self) -> Self {
        source
    }
}
impl ReprC for c_void {
    type CType = Self;
}
unsafe impl CType for c_void {}
unsafe impl BorrowCast for c_void {
    type AsConst = Self;
}
unsafe impl BorrowCastMut for c_void {
    type AsMut = Self;
}

impl ReprC for CStr {
    type CType = c_char;
}

unsafe impl NulTerminatedRef for CStr {
    fn as_c_ptr(&self) -> *const Self::CType {
        self.as_ptr()
    }

    unsafe fn from_c_ptr<'a>(ptr: *const Self::CType) -> &'a Self {
        unsafe { CStr::from_ptr(ptr) }
    }
}

#[cfg(feature = "alloc")]
impl ReprC for CString {
    type CType = CBox<c_char>;
}
#[cfg(feature = "alloc")]
unsafe impl EncodeOwned for CString {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        self.into_boxed_c_str().soft_encode(&mut ())
    }
}

#[cfg(feature = "alloc")]
impl Encode for CString {}

#[cfg(feature = "alloc")]
impl Niche for CString {
    const NICHE_VALUE: Self::CType = CBox::NICHE_VALUE;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cstr_reference_round_trip() {
        let original = c"hello";
        let encoded = crate::encode(original);
        assert_eq!(encoded, original.as_ptr());

        let decoded: &CStr = unsafe { crate::decode(encoded) }.unwrap();
        assert_eq!(decoded, original);

        let none: Option<&CStr> = unsafe { crate::decode(crate::encode(None::<&CStr>)) }.unwrap();
        assert!(none.is_none());
    }
}
