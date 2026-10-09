#[cfg(feature = "alloc")]
use alloc::boxed::Box;
#[cfg(feature = "alloc")]
use core::ptr::NonNull;
use core::{
    cell::{Cell, UnsafeCell},
    mem::ManuallyDrop,
};
use rust_spec::{
    RustSpec,
    size::{MetaSized, SliceLike},
};

use crate::{CType, transmute::CheckedTransmute};

pub(crate) type WideData<R> = <<R as Wide>::Header as WideHeader>::Data;

/// Describes the data addressed by a dynamically sized type's fixed header.
pub trait WideHeader {
    type Data;
}

/// A dynamically sized value with data and metadata (also called a `fat` pointer).
///
/// This includes slices, trait objects, and DSTs whose last field is one of the
/// aforementioned. This is an advanced trait used to form ABI slice and wide
/// representations; [`crate::ffi!`] handles ordinary slices automatically.
///
/// # Safety
///
/// - The header pointer must be aligned for `Header::Data` and point to its first element, if any.
/// - `metadata`, `as_ptr`, and `as_mut_ptr` **MUST** describe the same value and its actual layout.
/// - Ownership methods must transfer and recover the original allocation.
/// - Constructors must point to that value when given valid raw parts.
pub unsafe trait Wide {
    /// Fixed-layout prefix at the data address of a wide pointer.
    type Header: WideHeader;

    /// Metadata component of a pointer.
    type Metadata;

    /// Extracts the metadata component of a pointer.
    fn metadata(ptr: *const Self) -> Self::Metadata;

    /// Returns a raw pointer to the underlying data.
    fn as_ptr(ptr: *const Self) -> *const Self::Header;

    /// Returns a mutable raw pointer to the underlying data.
    fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Header;

    /// Forms a wide reference from a data pointer and metadata.
    ///
    /// # Safety
    ///
    /// `data` and `metadata` must describe a valid `Self` that can be shared for `'a`.
    unsafe fn from_raw_parts<'a>(data: *const Self::Header, metadata: Self::Metadata) -> &'a Self;

    /// Performs the same functionality as [`Self::from_raw_parts`], except that a mutable reference is returned.
    ///
    /// # Safety
    ///
    /// `data` and `metadata` must describe a valid `Self` that can be exclusively borrowed for `'a`.
    unsafe fn from_raw_parts_mut<'a>(
        data: *mut Self::Header,
        metadata: Self::Metadata,
    ) -> &'a mut Self;

    /// Consumes the `Box`, returning a wrapped `NonNull` pointer.
    ///
    /// See [`Box::into_non_null`]
    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Header>;

    /// Constructs a box from a `NonNull` pointer.
    ///
    /// # Safety
    ///
    /// `data` and `metadata` must identify the allocation transferred by `Self::into_non_null`.
    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(data: NonNull<Self::Header>, metadata: Self::Metadata) -> Box<Self>;
}

macro_rules! impl_wide_for_transparent_wrapper {
    ($($wrapper:ident),+ $(,)?) => {$(
        unsafe impl<R: Wide + ?Sized> Wide for $wrapper<R> {
            type Header = R::Header;
            type Metadata = R::Metadata;

            fn metadata(ptr: *const Self) -> Self::Metadata {
                R::metadata(ptr as *const R)
            }

            fn as_ptr(ptr: *const Self) -> *const Self::Header {
                R::as_ptr(ptr as *const R)
            }

            fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Header {
                R::as_mut_ptr(ptr as *mut R)
            }

            unsafe fn from_raw_parts<'a>(
                data: *const Self::Header,
                metadata: Self::Metadata,
            ) -> &'a Self {
                let inner = unsafe { R::from_raw_parts(data, metadata) };
                unsafe { &*(inner as *const R as *const Self) }
            }

            unsafe fn from_raw_parts_mut<'a>(
                data: *mut Self::Header,
                metadata: Self::Metadata,
            ) -> &'a mut Self {
                let inner = unsafe { R::from_raw_parts_mut(data, metadata) };
                unsafe { &mut *(inner as *mut R as *mut Self) }
            }

            #[cfg(feature = "alloc")]
            fn into_non_null(self: Box<Self>) -> NonNull<Self::Header> {
                Box::into_non_null(self).cast()
            }

            #[cfg(feature = "alloc")]
            unsafe fn from_non_null(
                data: NonNull<Self::Header>,
                metadata: Self::Metadata,
            ) -> Box<Self> {
                let inner = unsafe { R::from_raw_parts_mut(data.as_ptr(), metadata) };
                unsafe { Box::from_raw(inner as *mut R as *mut Self) }
            }
        }
    )+};
}

impl_wide_for_transparent_wrapper!(ManuallyDrop);

macro_rules! impl_wide_for_cell {
    ($($wrapper:ident),+ $(,)?) => {$(
        // SAFETY: CheckedTransmute equates T's layout with [C]. SliceLike gives
        // both pointer types the same length metadata; Cell and UnsafeCell
        // preserve the layout of T.
        unsafe impl<T: ?Sized, C: CType> Wide for $wrapper<T>
        where
            T: CheckedTransmute<CType = [C]> + RustSpec<Size = MetaSized<SliceLike>>,
        {
            type Header = [C; 0];
            type Metadata = usize;

            fn metadata(ptr: *const Self) -> Self::Metadata {
                let slice = unsafe { core::mem::transmute_copy::<*const Self, *const [C]>(&ptr) };
                slice.len()
            }

            fn as_ptr(ptr: *const Self) -> *const Self::Header {
                ptr.cast()
            }

            fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Header {
                ptr.cast()
            }

            unsafe fn from_raw_parts<'a>(data: *const Self::Header, len: usize) -> &'a Self {
                let slice = core::ptr::slice_from_raw_parts(data.cast::<C>(), len);
                let ptr = unsafe { core::mem::transmute_copy::<*const [C], *const Self>(&slice) };
                unsafe { &*ptr }
            }

            unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Header, len: usize) -> &'a mut Self {
                let slice = core::ptr::slice_from_raw_parts_mut(data.cast::<C>(), len);
                let ptr = unsafe { core::mem::transmute_copy::<*mut [C], *mut Self>(&slice) };
                unsafe { &mut *ptr }
            }

            #[cfg(feature = "alloc")]
            fn into_non_null(self: Box<Self>) -> NonNull<Self::Header> {
                Box::into_non_null(self).cast()
            }

            #[cfg(feature = "alloc")]
            unsafe fn from_non_null(data: NonNull<Self::Header>, len: usize) -> Box<Self> {
                let slice = NonNull::slice_from_raw_parts(data.cast::<C>(), len).as_ptr();
                let ptr = unsafe { core::mem::transmute_copy::<*mut [C], *mut Self>(&slice) };
                unsafe { Box::from_raw(ptr) }
            }
        }
    )+};
}

impl_wide_for_cell!(UnsafeCell, Cell);

unsafe impl<R> Wide for [R] {
    type Header = [R; 0];
    type Metadata = usize;

    fn metadata(ptr: *const Self) -> Self::Metadata {
        ptr.len()
    }

    fn as_ptr(ptr: *const Self) -> *const Self::Header {
        ptr as *const Self::Header
    }

    fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Header {
        ptr as *mut Self::Header
    }

    unsafe fn from_raw_parts<'a>(data: *const Self::Header, len: Self::Metadata) -> &'a Self {
        unsafe { core::slice::from_raw_parts(data.cast::<R>(), len) }
    }

    unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Header, len: Self::Metadata) -> &'a mut Self {
        unsafe { core::slice::from_raw_parts_mut(data.cast::<R>(), len) }
    }

    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Header> {
        Box::into_non_null(self).cast()
    }

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(data: NonNull<Self::Header>, len: Self::Metadata) -> Box<Self> {
        let slice = NonNull::slice_from_raw_parts(data.cast::<R>(), len);
        unsafe { Box::from_non_null(slice) }
    }
}

unsafe impl Wide for str {
    type Header = [u8; 0];
    type Metadata = usize;

    fn metadata(ptr: *const Self) -> Self::Metadata {
        (ptr as *const [u8]).len()
    }

    fn as_ptr(ptr: *const Self) -> *const Self::Header {
        ptr as *const Self::Header
    }

    fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Header {
        ptr as *mut Self::Header
    }

    unsafe fn from_raw_parts<'a>(data: *const Self::Header, len: Self::Metadata) -> &'a Self {
        let slice = unsafe { <[u8]>::from_raw_parts(data.cast(), len) };
        unsafe { core::str::from_utf8_unchecked(slice) }
    }

    unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Header, len: Self::Metadata) -> &'a mut Self {
        let slice = unsafe { <[u8]>::from_raw_parts_mut(data.cast(), len) };
        unsafe { core::str::from_utf8_unchecked_mut(slice) }
    }

    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Header> {
        self.into_boxed_bytes().into_non_null().cast()
    }

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(data: NonNull<Self::Header>, len: Self::Metadata) -> Box<Self> {
        let str = unsafe { Self::from_raw_parts_mut(data.as_ptr(), len) };

        let str = NonNull::from(str);
        unsafe { Box::from_non_null(str) }
    }
}

impl<T> WideHeader for [T; 0] {
    type Data = T;
}

#[cfg(test)]
mod tests {
    use core::{
        cell::{Cell, UnsafeCell},
        mem::ManuallyDrop,
    };

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::Wide;

    #[test]
    fn wide_is_implemented_for_builtin_wide_types() {
        assert_impl_all!([u8]: Wide<Header = [u8; 0], Metadata = usize>);
        assert_impl_all!(str: Wide<Header = [u8; 0], Metadata = usize>);
        assert_impl_all!(UnsafeCell<[u8]>: Wide<Header = [u8; 0], Metadata = usize>);
        assert_impl_all!(UnsafeCell<str>: Wide<Header = [u8; 0], Metadata = usize>);
        assert_impl_all!(Cell<[u8]>: Wide<Header = [u8; 0], Metadata = usize>);
        assert_impl_all!(Cell<str>: Wide<Header = [u8; 0], Metadata = usize>);
        assert_impl_all!(ManuallyDrop<[u8]>: Wide<Header = [u8; 0], Metadata = usize>);
        assert_impl_all!(ManuallyDrop<str>: Wide<Header = [u8; 0], Metadata = usize>);
    }

    #[test]
    fn wide_is_not_implemented_for_thin_types() {
        assert_not_impl_any!(u8: Wide);
        assert_not_impl_any!([u8; 4]: Wide);
        assert_not_impl_any!(&[u8]: Wide);
    }
}
