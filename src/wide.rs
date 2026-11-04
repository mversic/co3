//! Wide pointee metadata and C-ABI reference and owning carriers.

#[cfg(feature = "alloc")]
use alloc::boxed::Box;
#[cfg(feature = "alloc")]
use core::ptr::NonNull;
use core::{
    cell::{Cell, UnsafeCell},
    fmt::Debug,
    hash::Hash,
    mem::ManuallyDrop,
};

use rust_spec::{
    RustSpec,
    size::{MetaSized, SliceLike},
};

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC, Zeroable,
    stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked},
    transmute::AbiCompatible,
};

pub(crate) type WideData<R> = <<R as Wide>::Header as WideHeader>::Data;

/// Describes the data addressed by a dynamically sized type's fixed header.
pub trait WideHeader {
    type Data;
}

/// Element at the data address of a C-compatible wide pointee.
pub(crate) type PointeeData<P> = <<P as Wide>::Header as WideHeader>::Data;

/// A dynamically sized value with data and metadata (also called a `fat` pointer).
///
/// This includes slices, trait objects, and DSTs whose last field is one of the
/// aforementioned. This is an advanced trait used to form ABI slice and wide
/// representations; [`crate::ffi!`] handles ordinary slices automatically.
///
/// # Safety
///
/// - Implementor must be a DST; a sized type, including a ZST, must not implement this trait.
/// - The header pointer must be aligned for `Header::Data` and point to its first element, if any.
/// - `metadata`, `as_ptr`, and `as_mut_ptr` **MUST** describe the same value and its actual layout.
/// - Ownership methods must transfer and recover the original allocation.
/// - Constructors must point to that value when given valid raw parts.
pub unsafe trait Wide {
    /// The fixed header at the start of the allocation.
    type Header: WideHeader;

    /// Metadata component of a pointer.
    type Metadata: Debug + Copy + Send + Sync + Ord + Hash + Unpin + CType + Zeroable;

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
                let inner = unsafe { R::from_non_null(data, metadata) };
                let inner = Box::into_raw(inner) as *mut Self;
                unsafe { Box::from_raw(inner) }
            }
        }
    )+};
}

impl_wide_for_transparent_wrapper!(ManuallyDrop);

macro_rules! impl_wide_for_cell {
    ($($wrapper:ident),+ $(,)?) => {$(
        unsafe impl<R: Wide<Metadata = usize> + ?Sized> Wide for $wrapper<R>
        where
            R: rust_spec::RustSpec<Size = MetaSized<SliceLike>>,
        {
            type Header = R::Header;
            type Metadata = usize;

            fn metadata(ptr: *const Self) -> Self::Metadata {
                R::metadata(ptr as *const R)
            }

            fn as_ptr(ptr: *const Self) -> *const Self::Header {
                ptr.cast()
            }

            fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Header {
                ptr.cast()
            }

            unsafe fn from_raw_parts<'a>(data: *const Self::Header, len: usize) -> &'a Self {
                let slice = core::ptr::slice_from_raw_parts(data.cast::<u8>(), len);
                let ptr = unsafe { core::mem::transmute_copy::<*const [u8], *const Self>(&slice) };
                unsafe { &*ptr }
            }

            unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Header, len: usize) -> &'a mut Self {
                let slice = core::ptr::slice_from_raw_parts_mut(data.cast::<u8>(), len);
                let ptr = unsafe { core::mem::transmute_copy::<*mut [u8], *mut Self>(&slice) };
                unsafe { &mut *ptr }
            }

            #[cfg(feature = "alloc")]
            fn into_non_null(self: Box<Self>) -> NonNull<Self::Header> {
                Box::into_non_null(self).cast()
            }

            #[cfg(feature = "alloc")]
            unsafe fn from_non_null(data: NonNull<Self::Header>, len: usize) -> Box<Self> {
                let slice = NonNull::slice_from_raw_parts(data.cast::<u8>(), len).as_ptr();
                let ptr = unsafe { core::mem::transmute_copy::<*mut [u8], *mut Self>(&slice) };
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
        let raw = core::ptr::slice_from_raw_parts_mut(data.as_ptr().cast::<u8>(), len);
        unsafe { Box::from_raw(raw as *mut str) }
    }
}

impl<T> WideHeader for [T; 0] {
    type Data = T;
}

/// Shared reference to a C-compatible wide pointee.
///
/// A null data pointer represents an absent reference.
#[derive(RustSpec)]
#[repr(C)]
pub struct CSoftWideRef<R: ReprC<CType: Wide> + ?Sized> {
    data: *const PointeeData<R::CType>,
    metadata: <R::CType as Wide>::Metadata,
}

/// Mutable wide-reference carrier with a defined C ABI layout.
///
/// - `EXCLUSIVE = true` represents exclusive access that comes from `&mut T`.
/// - `EXCLUSIVE = false` represents access that comes from `&UnsafeCell<T>`.
///
/// `EXCLUSIVE == false` should be read as unknown. For all is known to this wrapper the access
/// is exclusive. You have to use `UnsafeCell` in `P` to assert non-exclusive access. It is the
/// user's responsibility to ensure any access of the pointer is well synchronized.
#[derive(RustSpec)]
#[repr(C)]
pub struct CSoftWideRefMut<R: ReprC<CType: Wide> + ?Sized, const EXCLUSIVE: bool> {
    data: *mut PointeeData<R::CType>,
    metadata: <R::CType as Wide>::Metadata,
}

impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> CSoftWideRef<R> {
    /// Construct an absent carrier with initialized metadata.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null(),
            // CType requires every bit pattern to be valid.
            metadata: unsafe { core::mem::zeroed() },
        }
    }

    /// Create a carrier from a data pointer and metadata.
    ///
    /// # Safety
    /// A non-null pointer and metadata must describe a valid P for any access.
    pub const unsafe fn from_raw_parts(
        data: *const PointeeData<R::CType>,
        metadata: <R::CType as Wide>::Metadata,
    ) -> Self {
        Self { data, metadata }
    }

    /// Wrap an existing wide reference.
    pub fn from_ref(source: &R::CType) -> Self {
        Self {
            data: R::CType::as_ptr(source).cast(),
            metadata: R::CType::metadata(source),
        }
    }
}

impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const EXCLUSIVE: bool>
    CSoftWideRefMut<R, EXCLUSIVE>
{
    /// Construct an absent carrier with initialized metadata.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null_mut(),
            // CType requires every bit pattern to be valid.
            metadata: unsafe { core::mem::zeroed() },
        }
    }

    /// Create a mutable carrier from a data pointer and metadata.
    ///
    /// # Safety
    /// The pointer and metadata must describe a valid P; when EXCLUSIVE is true,
    /// accesses must satisfy exclusive access.
    pub const unsafe fn from_raw_parts_mut(
        data: *mut PointeeData<R::CType>,
        metadata: <R::CType as Wide>::Metadata,
    ) -> Self {
        Self { data, metadata }
    }

    /// Wrap an existing mutable wide reference.
    pub fn from_mut(source: &mut R::CType) -> Self {
        Self {
            data: R::CType::as_mut_ptr(source).cast(),
            metadata: R::CType::metadata(source),
        }
    }
}

impl<R: ReprC<CType: Wide> + ?Sized> CSoftWideRef<R> {
    /// Cast the pointee type, preserving the data address and metadata unchanged.
    ///
    /// # Safety
    ///
    /// The resulting carrier must satisfy the destination's raw constructor contract
    pub(crate) const unsafe fn cast<U>(self) -> CSoftWideRef<U>
    where
        U: ReprC<CType: Wide<Metadata = <R::CType as Wide>::Metadata>> + ?Sized,
    {
        CSoftWideRef {
            data: self.data.cast(),
            metadata: self.metadata,
        }
    }

    pub const fn as_ptr(&self) -> *const PointeeData<R::CType> {
        self.data
    }
    pub(crate) const fn data(&self) -> *const PointeeData<R::CType> {
        self.data
    }

    /// Extracts the metadata component of a pointer.
    pub const fn metadata(&self) -> <R::CType as Wide>::Metadata {
        self.metadata
    }
}

impl<R: ReprC<CType: Wide> + ?Sized, const EXCLUSIVE: bool> CSoftWideRefMut<R, EXCLUSIVE> {
    /// Cast the pointee type, preserving the data address and metadata unchanged.
    ///
    /// # Safety
    ///
    /// The resulting carrier must satisfy the destination's raw constructor contract
    #[allow(dead_code)]
    pub(crate) const unsafe fn cast<U>(self) -> CSoftWideRefMut<U, EXCLUSIVE>
    where
        U: ReprC<CType: Wide<Metadata = <R::CType as Wide>::Metadata>> + ?Sized,
    {
        CSoftWideRefMut {
            data: self.data.cast(),
            metadata: self.metadata,
        }
    }

    pub const fn as_ptr(&self) -> *mut PointeeData<R::CType> {
        self.data
    }
    pub(crate) const fn data(&self) -> *mut PointeeData<R::CType> {
        self.data
    }
    pub(crate) const fn is_null(&self) -> bool {
        self.data.is_null()
    }

    /// Extracts the metadata component of a pointer.
    pub const fn metadata(&self) -> <R::CType as Wide>::Metadata {
        self.metadata
    }
}

impl<R: ReprC<CType: Wide> + ?Sized> CSoftWideRef<R> {
    /// Recover a shared wide reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer and metadata must describe a valid `P` for the returned lifetime.
    pub unsafe fn into_ref<'a>(self) -> Option<&'a R::CType> {
        if self.data.is_null() {
            return None;
        }

        let metadata = self.metadata;
        let data = self.data.cast();

        Some(unsafe { R::CType::from_raw_parts(data, metadata) })
    }
}

impl<R: ReprC<CType: Wide> + ?Sized> CSoftWideRefMut<R, true> {
    /// Recover a mutable wide reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer and metadata must describe a valid, exclusively accessible `P`
    /// for the returned lifetime.
    pub unsafe fn into_mut<'a>(self) -> Option<&'a mut R::CType> {
        if self.data.is_null() {
            return None;
        }

        let metadata = self.metadata;
        let data = self.data.cast();

        Some(unsafe { R::CType::from_raw_parts_mut(data, metadata) })
    }
}

macro_rules! impl_wide_raw_methods {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
        impl<$($params)*> core::fmt::Debug for $ty<$($args)*> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_struct(stringify!($ty)).field("data", &self.data)
                    .field("metadata", &self.metadata).finish()
            }
        }

        impl<$($params)*> PartialEq for $ty<$($args)*> {
            fn eq(&self, other: &Self) -> bool {
                !self.data.is_null() && self.data == other.data && self.metadata == other.metadata
            }
        }

        impl<$($params)*> PartialOrd for $ty<$($args)*> {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                if self.data.is_null() || other.data.is_null() {
                    None
                } else {
                    Some(self.data.cmp(&other.data).then_with(|| self.metadata.cmp(&other.metadata)))
                }
            }
        }

        impl<$($params)*> core::hash::Hash for $ty<$($args)*> {
            fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                core::hash::Hash::hash(&self.data, state);
                core::hash::Hash::hash(&self.metadata, state);
            }
        }

        impl<$($params)*> Copy for $ty<$($args)*> {}
        impl<$($params)*> Clone for $ty<$($args)*> {
            fn clone(&self) -> Self { *self }
        }
    };
}

macro_rules! impl_identity_carrier {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
        impl<$($params)*> ReprC for $ty<$($args)*> {
            type CType = Self;
        }
        unsafe impl<$($params)*> EncodeMove for $ty<$($args)*> {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                unsafe { self.soft_encode_unchecked(&mut ()) }
            }
        }
        unsafe impl<$($params)*> EncodeMoveUnchecked for $ty<$($args)*> {
            type SoftStore = ();

            unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm {
                self
            }
        }
        unsafe impl<'d, $($params)*> DecodeMove<'d> for $ty<$($args)*> {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> core::option::Option<Self> {
                Some(source)
            }
        }

        impl<$($params)*> Encode for $ty<$($args)*> {}
        impl<$($params)*> crate::EncodeUnchecked for $ty<$($args)*> {}
        impl<'d, $($params)*> Decode<'d> for $ty<$($args)*> {}


        unsafe impl<$($params)*> $crate::transmute::AbiCompatibleValidate for $ty<$($args)*> {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl<$($params)*> AbiCompatible for $ty<$($args)*> {}
        unsafe impl<$($params)*> CType for $ty<$($args)*> {}
        unsafe impl<$($params)*> CFnArg for $ty<$($args)*> {}
        unsafe impl<$($params)*> CFnReturn for $ty<$($args)*> {}
    };
}

macro_rules! impl_carrier_borrow {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
        unsafe impl<$($params)*> $crate::borrow::Borrow for $ty<$($args)*> {
            type Borrowed = Self;

            fn borrow(self) -> Self::Borrowed {
                self
            }

            fn from_borrow(source: Self) -> Self {
                source
            }
        }
    };
}

impl_wide_raw_methods! { CSoftWideRef, [R: ReprC<CType: Wide> + ?Sized], [R] }
impl_wide_raw_methods! { CSoftWideRefMut, [R: ReprC<CType: Wide> + ?Sized, const EXCLUSIVE: bool], [R, EXCLUSIVE] }

impl_carrier_borrow! { CSoftWideRef, [R: ReprC<CType: Wide> + ?Sized], [R] }
impl_carrier_borrow! { CSoftWideRefMut, [R: ReprC<CType: Wide> + ?Sized, const EXCLUSIVE: bool], [R, EXCLUSIVE] }

impl_identity_carrier! { CSoftWideRef, [R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized], [R] }
impl_identity_carrier! { CSoftWideRefMut, [R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const EXCLUSIVE: bool], [R, EXCLUSIVE] }

impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> Default
    for CSoftWideRef<R>
{
    fn default() -> Self {
        Self::null()
    }
}
impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const EXCLUSIVE: bool> Default
    for CSoftWideRefMut<R, EXCLUSIVE>
{
    fn default() -> Self {
        Self::null()
    }
}

#[cfg(feature = "alloc")]
/// Owned wide pointee with a defined C ABI layout.
///
/// If the data pointer is set to `null`, the struct represents an absent box.
#[derive(RustSpec)]
#[repr(C)]
pub struct CWideBox<R: ReprC<CType: Wide> + ?Sized> {
    data: *mut PointeeData<R::CType>,
    metadata: <R::CType as Wide>::Metadata,
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> Default for CWideBox<R> {
    fn default() -> Self {
        Self::null()
    }
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> CWideBox<R> {
    /// Construct an absent carrier with initialized metadata.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null_mut(),
            metadata: unsafe { core::mem::zeroed() },
        }
    }
    /// Create a carrier from owned raw parts.
    ///
    /// # Safety
    ///
    /// The parts must retain unique ownership of a live allocation compatible with `P`.
    pub const unsafe fn from_raw_parts(
        data: NonNull<PointeeData<R::CType>>,
        metadata: <R::CType as Wide>::Metadata,
    ) -> Self {
        Self {
            data: data.as_ptr(),
            metadata,
        }
    }
    /// Create a carrier from a boxed value.
    pub fn from_box(source: Box<R::CType>) -> Self {
        let metadata = R::CType::metadata(&*source);
        let data = R::CType::into_non_null(source).cast();
        Self {
            data: data.as_ptr(),
            metadata,
        }
    }
    /// Recover the owned value, or `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The parts must retain unique ownership of a live allocation compatible with `P`.
    pub unsafe fn into_box(self) -> Option<Box<R::CType>> {
        let data = NonNull::<<R::CType as Wide>::Header>::new(self.data.cast())?;
        Some(unsafe { R::CType::from_non_null(data, self.metadata) })
    }

    pub(crate) const unsafe fn from_raw_nullable(
        data: *mut PointeeData<R::CType>,
        metadata: <R::CType as Wide>::Metadata,
    ) -> Self {
        Self { data, metadata }
    }
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Wide> + ?Sized> CWideBox<R> {
    pub const fn as_ptr(self) -> *mut PointeeData<R::CType> {
        self.data
    }

    pub const fn metadata(&self) -> <R::CType as Wide>::Metadata {
        self.metadata
    }
}

#[cfg(feature = "alloc")]
impl_wide_raw_methods! { CWideBox, [R: ReprC<CType: Wide> + ?Sized], [R] }
#[cfg(feature = "alloc")]
impl_identity_carrier! { CWideBox, [R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized], [R] }
#[cfg(feature = "alloc")]
impl_carrier_borrow! { CWideBox, [R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized], [R] }

unsafe impl<R: ReprC<CType: Wide> + ?Sized> Zeroable for CSoftWideRef<R> {}
unsafe impl<R: ReprC<CType: Wide> + ?Sized, const EXCLUSIVE: bool> Zeroable
    for CSoftWideRefMut<R, EXCLUSIVE>
{
}
#[cfg(feature = "alloc")]
unsafe impl<R: ReprC<CType: Wide> + ?Sized> Zeroable for CWideBox<R> {}

#[cfg(feature = "alloc")]
pub(crate) use impl_carrier_borrow;
pub(crate) use impl_identity_carrier;

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

    #[cfg(feature = "derive")]
    #[repr(C)]
    #[derive(rust_spec::RustSpec, crate::ReprC)]
    struct Packet<T: ?Sized> {
        header: u32,
        tail: T,
    }

    #[cfg(feature = "derive")]
    #[test]
    fn cells_with_custom_dst_preserve_raw_parts() {
        // A live mutable borrow of the contents must survive reconstruction of
        // a shared cell reference. Miri catches an intermediate &Packet here.
        let cell = UnsafeCell::new(Packet {
            header: 7,
            tail: [1u8, 2, 3],
        });
        let ptr: *const UnsafeCell<Packet<[u8]>> = &cell;
        let inner = unsafe { &mut *cell.get() };
        let data = <UnsafeCell<Packet<[u8]>> as Wide>::as_ptr(ptr);
        let len = <UnsafeCell<Packet<[u8]>> as Wide>::metadata(ptr);
        let rebuilt = unsafe { <UnsafeCell<Packet<[u8]>> as Wide>::from_raw_parts(data, len) };
        assert!(core::ptr::eq(ptr, rebuilt));
        inner.tail[1] = 9;
        assert_eq!(inner.header, 7);
        assert_eq!(inner.tail, [1, 9, 3]);

        let mut cell = Cell::new(Packet {
            header: 8,
            tail: [4u8, 5],
        });
        let ptr: *mut Cell<Packet<[u8]>> = &mut cell;
        let data = <Cell<Packet<[u8]>> as Wide>::as_mut_ptr(ptr);
        let len = <Cell<Packet<[u8]>> as Wide>::metadata(ptr);
        let rebuilt = unsafe { <Cell<Packet<[u8]>> as Wide>::from_raw_parts_mut(data, len) };
        rebuilt.get_mut().tail[0] = 6;
        assert_eq!(cell.into_inner().tail, [6, 5]);

        #[cfg(feature = "alloc")]
        {
            let value: alloc::boxed::Box<UnsafeCell<Packet<[u8]>>> =
                alloc::boxed::Box::new(UnsafeCell::new(Packet {
                    header: 11,
                    tail: [8u8, 9],
                }));
            let len = <UnsafeCell<Packet<[u8]>> as Wide>::metadata(&*value);
            let data = <UnsafeCell<Packet<[u8]>> as Wide>::into_non_null(value);
            let mut value = unsafe { <UnsafeCell<Packet<[u8]>> as Wide>::from_non_null(data, len) };
            assert_eq!(value.get_mut().header, 11);
            assert_eq!(&value.get_mut().tail, &[8, 9]);
        }
    }

    #[test]
    fn wide_is_not_implemented_for_thin_types() {
        assert_not_impl_any!(u8: Wide);
        assert_not_impl_any!([u8; 4]: Wide);
        assert_not_impl_any!(&[u8]: Wide);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn boxed_manually_drop_str_preserves_allocation() {
        use alloc::boxed::Box;

        for text in ["", "héllo"] {
            let raw = Box::into_raw(Box::<str>::from(text));
            let value = unsafe { Box::from_raw(raw as *mut ManuallyDrop<str>) };
            let len = <ManuallyDrop<str> as Wide>::metadata(&*value);
            let data = <ManuallyDrop<str> as Wide>::into_non_null(value);
            let value = unsafe { <ManuallyDrop<str> as Wide>::from_non_null(data, len) };
            let restored = Box::into_raw(value) as *mut str;
            assert!(core::ptr::eq(raw, restored));
            let value = unsafe { Box::from_raw(restored) };
            assert_eq!(&*value, text);
        }
    }
}
