#[cfg(feature = "alloc")]
use alloc::boxed::Box;
#[cfg(feature = "alloc")]
use core::ptr::NonNull;
use core::{
    cell::{Cell, UnsafeCell},
    mem::ManuallyDrop,
};

/// A dynamically sized value with data and metadata (also called a `fat` pointer).
///
/// This includes slices, trait objects, and DSTs whose last field is one of the
/// aforementioned. This is an advanced trait used to form ABI slice and wide
/// representations; [`crate::ffi!`] handles ordinary slices automatically.
///
/// # Safety
///
/// - `metadata`, `as_ptr`, and `as_mut_ptr` **MUST** describe the same value and its actual layout
/// - Ownership methods must transfer and recover the original allocation.
/// - Constructors must point to that value when given valid raw parts.
pub unsafe trait Wide {
    /// Data component of a wide pointer.
    ///
    /// # Warning
    ///
    /// Constructing a `&T::Data` is only valid if conceptually `Self::Metadata > 0`
    type Data;

    /// Metadata component of a pointer.
    type Metadata;

    /// Extracts the metadata component of a pointer.
    fn metadata(ptr: *const Self) -> Self::Metadata;

    /// Returns a raw pointer to the underlying data.
    fn as_ptr(ptr: *const Self) -> *const Self::Data;

    /// Returns a mutable raw pointer to the underlying data.
    fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Data;

    /// Forms a wide reference from a data pointer and metadata.
    ///
    /// # Safety
    ///
    /// `data` and `metadata` must describe a valid `Self` that can be shared for `'a`.
    unsafe fn from_raw_parts<'a>(data: *const Self::Data, metadata: Self::Metadata) -> &'a Self;

    /// Performs the same functionality as [`Self::from_raw_parts`], except that a mutable reference is returned.
    ///
    /// # Safety
    ///
    /// `data` and `metadata` must describe a valid `Self` that can be exclusively borrowed for `'a`.
    unsafe fn from_raw_parts_mut<'a>(
        data: *mut Self::Data,
        metadata: Self::Metadata,
    ) -> &'a mut Self;

    /// Consumes the `Box`, returning a wrapped `NonNull` pointer.
    ///
    /// See [`Box::into_non_null`]
    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Data>;

    /// Constructs a box from a `NonNull` pointer.
    ///
    /// # Safety
    ///
    /// `data` and `metadata` must identify the allocation transferred by `Self::into_non_null`.
    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(data: NonNull<Self::Data>, metadata: Self::Metadata) -> Box<Self>;
}

macro_rules! impl_wide_for_transparent_wrapper {
    ($($wrapper:ident),+ $(,)?) => {$(
        // TODO: It's super weird that we require Wide::Data: ReprC here
        unsafe impl<R: Wide<Data: crate::ReprC> + ?Sized> Wide for $wrapper<R> {
            type Data = R::Data;
            type Metadata = R::Metadata;

            fn metadata(ptr: *const Self) -> Self::Metadata {
                R::metadata(ptr as *const R)
            }

            fn as_ptr(ptr: *const Self) -> *const Self::Data {
                R::as_ptr(ptr as *const R)
            }

            fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Data {
                R::as_mut_ptr(ptr as *mut R)
            }

            unsafe fn from_raw_parts<'a>(
                data: *const Self::Data,
                metadata: Self::Metadata,
            ) -> &'a Self {
                let inner = unsafe { R::from_raw_parts(data, metadata) };
                unsafe { &*(inner as *const R as *const Self) }
            }

            unsafe fn from_raw_parts_mut<'a>(
                data: *mut Self::Data,
                metadata: Self::Metadata,
            ) -> &'a mut Self {
                let inner = unsafe { R::from_raw_parts_mut(data, metadata) };
                unsafe { &mut *(inner as *mut R as *mut Self) }
            }

            #[cfg(feature = "alloc")]
            fn into_non_null(self: Box<Self>) -> NonNull<Self::Data> {
                Box::into_non_null(self).cast()
            }

            #[cfg(feature = "alloc")]
            unsafe fn from_non_null(
                data: NonNull<Self::Data>,
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
        unsafe impl<R: crate::ReprC> Wide for $wrapper<[R]> {
            type Data = R;
            type Metadata = usize;

            fn metadata(ptr: *const Self) -> Self::Metadata {
                (ptr as *const [R]).len()
            }

            fn as_ptr(ptr: *const Self) -> *const Self::Data {
                ptr.cast()
            }

            fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Data {
                ptr.cast()
            }

            unsafe fn from_raw_parts<'a>(data: *const R, len: usize) -> &'a Self {
                let ptr = core::ptr::slice_from_raw_parts(data, len) as *const Self;
                unsafe { &*ptr }
            }

            unsafe fn from_raw_parts_mut<'a>(data: *mut R, len: usize) -> &'a mut Self {
                let ptr = core::ptr::slice_from_raw_parts_mut(data, len) as *mut Self;
                unsafe { &mut *ptr }
            }

            #[cfg(feature = "alloc")]
            fn into_non_null(self: Box<Self>) -> NonNull<Self::Data> {
                Box::into_non_null(self).cast()
            }

            #[cfg(feature = "alloc")]
            unsafe fn from_non_null(data: NonNull<R>, len: usize) -> Box<Self> {
                let ptr = NonNull::slice_from_raw_parts(data, len).as_ptr() as *mut Self;
                unsafe { Box::from_raw(ptr) }
            }
        }

        unsafe impl Wide for $wrapper<str> {
            type Data = u8;
            type Metadata = usize;

            fn metadata(ptr: *const Self) -> Self::Metadata {
                (ptr as *const [u8]).len()
            }

            fn as_ptr(ptr: *const Self) -> *const Self::Data {
                ptr.cast()
            }

            fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Data {
                ptr.cast()
            }

            unsafe fn from_raw_parts<'a>(data: *const u8, len: usize) -> &'a Self {
                let ptr = core::ptr::slice_from_raw_parts(data, len) as *const Self;
                unsafe { &*ptr }
            }

            unsafe fn from_raw_parts_mut<'a>(data: *mut u8, len: usize) -> &'a mut Self {
                let ptr = core::ptr::slice_from_raw_parts_mut(data, len) as *mut Self;
                unsafe { &mut *ptr }
            }

            #[cfg(feature = "alloc")]
            fn into_non_null(self: Box<Self>) -> NonNull<Self::Data> {
                Box::into_non_null(self).cast()
            }

            #[cfg(feature = "alloc")]
            unsafe fn from_non_null(data: NonNull<u8>, len: usize) -> Box<Self> {
                let ptr = NonNull::slice_from_raw_parts(data, len).as_ptr() as *mut Self;
                unsafe { Box::from_raw(ptr) }
            }
        }
    )+};
}

impl_wide_for_cell!(UnsafeCell, Cell);

unsafe impl<R> Wide for [R] {
    type Data = R;
    type Metadata = usize;

    fn metadata(ptr: *const Self) -> Self::Metadata {
        ptr.len()
    }

    fn as_ptr(ptr: *const Self) -> *const Self::Data {
        ptr as *const R
    }

    fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Data {
        ptr as *mut R
    }

    unsafe fn from_raw_parts<'a>(data: *const Self::Data, len: Self::Metadata) -> &'a Self {
        unsafe { core::slice::from_raw_parts(data, len) }
    }

    unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Data, len: Self::Metadata) -> &'a mut Self {
        unsafe { core::slice::from_raw_parts_mut(data, len) }
    }

    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Data> {
        Box::into_non_null(self).cast()
    }

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(data: NonNull<Self::Data>, len: Self::Metadata) -> Box<Self> {
        let slice = NonNull::slice_from_raw_parts(data, len);
        unsafe { Box::from_non_null(slice) }
    }
}

unsafe impl Wide for str {
    type Data = u8;
    type Metadata = usize;

    fn metadata(ptr: *const Self) -> Self::Metadata {
        (ptr as *const [u8]).len()
    }

    fn as_ptr(ptr: *const Self) -> *const Self::Data {
        ptr as *const u8
    }

    fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Data {
        ptr as *mut u8
    }

    unsafe fn from_raw_parts<'a>(data: *const Self::Data, len: Self::Metadata) -> &'a Self {
        let slice = unsafe { <[u8]>::from_raw_parts(data, len) };
        unsafe { core::str::from_utf8_unchecked(slice) }
    }

    unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Data, len: Self::Metadata) -> &'a mut Self {
        let slice = unsafe { <[u8]>::from_raw_parts_mut(data, len) };
        unsafe { core::str::from_utf8_unchecked_mut(slice) }
    }

    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Data> {
        self.into_boxed_bytes().into_non_null()
    }

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(data: NonNull<Self::Data>, len: Self::Metadata) -> Box<Self> {
        let str = unsafe { Self::from_raw_parts_mut(data.as_ptr(), len) };

        let str = NonNull::from(str);
        unsafe { Box::from_non_null(str) }
    }
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
        assert_impl_all!([u8]: Wide<Data = u8, Metadata = usize>);
        assert_impl_all!(str: Wide<Data = u8, Metadata = usize>);
        assert_impl_all!(UnsafeCell<[u8]>: Wide<Data = u8, Metadata = usize>);
        assert_impl_all!(UnsafeCell<str>: Wide<Data = u8, Metadata = usize>);
        assert_impl_all!(Cell<[u8]>: Wide<Data = u8, Metadata = usize>);
        assert_impl_all!(Cell<str>: Wide<Data = u8, Metadata = usize>);
        assert_impl_all!(ManuallyDrop<[u8]>: Wide<Data = u8, Metadata = usize>);
        assert_impl_all!(ManuallyDrop<str>: Wide<Data = u8, Metadata = usize>);
    }

    #[test]
    fn wide_is_not_implemented_for_thin_types() {
        assert_not_impl_any!(u8: Wide);
        assert_not_impl_any!([u8; 4]: Wide);
        assert_not_impl_any!(&[u8]: Wide);
    }
}
