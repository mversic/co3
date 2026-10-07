//! Conversions for types in [`core::ffi`].

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, ffi::CString};
#[cfg(feature = "alloc")]
use core::ptr::NonNull;
use core::{
    ffi::{CStr, c_char, c_void},
    mem::ManuallyDrop,
};

use crate::{
    CType, ReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
};
#[cfg(feature = "alloc")]
use crate::{
    Decode, Encode,
    boxed::CBox,
    niche::Niche,
    stored::{DecodeOwned, EncodeOwned},
};

/// A nul-terminated value represented across the ABI by a pointer to its data.
///
/// # Safety
///
/// Implementations must identify the same valid nul-terminated data through every method.
pub unsafe trait NulTerminatedBuf {
    /// Data of a nul-terminated pointer.
    type Data;

    /// Returns a raw pointer to the underlying data.
    fn as_ptr(ptr: *const Self) -> *const Self::Data;

    /// Forms a nul-terminated reference from a data pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must be non-null and properly aligned. The initialized data through the first nul
    /// must be readable within one allocation, fit in `isize::MAX` bytes, and remain unchanged for
    /// `'a`.
    unsafe fn from_raw<'a>(ptr: *const Self::Data) -> &'a Self;

    /// Consumes the `Box`, returning a wrapped `NonNull` pointer.
    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Data>;

    /// Constructs a box from a `NonNull` pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must come from `Self::into_non_null`, retain its allocation provenance,
    /// and still own the complete allocation.
    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(ptr: NonNull<Self::Data>) -> Box<Self>;
}

unsafe impl<R: NulTerminatedBuf + ?Sized> NulTerminatedBuf for ManuallyDrop<R> {
    type Data = R::Data;

    fn as_ptr(ptr: *const Self) -> *const Self::Data {
        R::as_ptr(ptr as *const R)
    }

    unsafe fn from_raw<'a>(ptr: *const Self::Data) -> &'a Self {
        let inner = unsafe { R::from_raw(ptr) };
        unsafe { &*(inner as *const R as *const Self) }
    }

    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Data> {
        let inner = unsafe { Box::from_raw(Box::into_raw(self) as *mut R) };
        R::into_non_null(inner)
    }

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(ptr: NonNull<Self::Data>) -> Box<Self> {
        let inner = unsafe { R::from_non_null(ptr) };
        unsafe { Box::from_raw(Box::into_raw(inner) as *mut Self) }
    }
}

impl ReprC for CStr {
    type CType = c_char;
}

unsafe impl NulTerminatedBuf for CStr {
    type Data = c_char;

    fn as_ptr(ptr: *const Self) -> *const Self::Data {
        ptr.cast()
    }

    unsafe fn from_raw<'a>(ptr: *const Self::Data) -> &'a Self {
        unsafe { CStr::from_ptr(ptr) }
    }

    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Data> {
        let raw = self.into_c_string().into_raw();
        unsafe { NonNull::new_unchecked(raw) }
    }

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(ptr: NonNull<Self::Data>) -> Box<Self> {
        unsafe { CString::from_raw(ptr.as_ptr()) }.into_boxed_c_str()
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
unsafe impl<'d> DecodeOwned<'d> for CString {
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        unsafe { Box::<CStr>::soft_decode(source, &mut ()) }.map(Into::into)
    }
}

#[cfg(feature = "alloc")]
impl Encode for CString {}
#[cfg(feature = "alloc")]
impl Decode<'_> for CString {}

#[cfg(feature = "alloc")]
impl Niche for CString {
    const NICHE: Self::CType = CBox::NICHE;
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

#[cfg(test)]
mod tests {
    use core::cell::{Cell, UnsafeCell};

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    use crate::Decode;

    #[test]
    fn nul_terminated_buf_wrapper_impls() {
        assert_impl_all!(CStr: NulTerminatedBuf);
        assert_not_impl_any!(UnsafeCell<CStr>: NulTerminatedBuf);
        assert_not_impl_any!(Cell<CStr>: NulTerminatedBuf);
        assert_impl_all!(ManuallyDrop<CStr>: NulTerminatedBuf);
        assert_impl_all!(&ManuallyDrop<CStr>: Encode, Decode<'static>);
        assert_not_impl_any!(&mut CStr: ReprC, Encode, Decode<'static>);
        #[cfg(feature = "alloc")]
        {
            assert_impl_all!(Box<ManuallyDrop<CStr>>: EncodeOwned, crate::stored::DecodeOwned<'static>);
            assert_impl_all!(CString: Encode);
        }
    }

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

    #[cfg(feature = "alloc")]
    #[test]
    fn cstr_box_round_trip() {
        let original = CString::new("owned").unwrap().into_boxed_c_str();
        let ptr = original.as_ptr();
        let encoded = crate::stored::encode_owned(original);
        assert_eq!(encoded.data.cast_const(), ptr);
        let decoded: Box<CStr> = unsafe { crate::stored::decode_owned(encoded) }.unwrap();
        assert_eq!(decoded.as_ptr(), ptr);
        assert_eq!(&*decoded, c"owned");
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn cstring_round_trip() {
        let original = CString::new("owned").unwrap();
        let encoded = crate::encode(original.clone());
        let decoded: CString = unsafe { crate::decode(encoded) }.unwrap();
        assert_eq!(decoded, original);
    }
}
