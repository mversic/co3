//! Wide pointee metadata and C-ABI reference and owning carriers.

#[cfg(feature = "alloc")]
use alloc::{borrow::ToOwned, boxed::Box};
use core::{
    cell::{Cell, UnsafeCell},
    fmt::Debug,
    hash::Hash,
    marker::PhantomData,
    ptr::NonNull,
};

use rust_spec::{
    RustSpec,
    size::{MetaSized, SliceLike},
};

use crate::{
    CFnArg, CFnReturn, CType, ReprC, Zeroable,
    convert::{Decode, DecodeMove, Encode, EncodeMove, EncodeMoveUnchecked},
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

/// A shared wide reference represented as a non-null data pointer and metadata.
///
/// The layout of the pointee is determined by its [`Wide`] implementation.
#[derive(RustSpec)]
#[repr(C)]
pub struct WideRef<'a, R: Wide + ?Sized> {
    data: NonNull<PointeeData<R>>,
    metadata: R::Metadata,
    lifetime: PhantomData<&'a ()>,
}

impl<'a, R: Wide + ?Sized> WideRef<'a, R> {
    /// Wrap a wide reference without changing its pointee or metadata.
    pub fn from_ref(source: &'a R) -> Self {
        let data = NonNull::new(R::as_ptr(source).cast_mut().cast())
            .expect("Wide::as_ptr returned null for a reference");
        unsafe { Self::from_raw_parts(data, R::metadata(source)) }
    }

    /// Construct a wide reference from its raw parts.
    ///
    /// # Safety
    /// The parts must describe a valid `R` that can be shared for `'a`.
    pub const unsafe fn from_raw_parts(
        data: NonNull<PointeeData<R>>,
        metadata: R::Metadata,
    ) -> Self {
        Self {
            data,
            metadata,
            lifetime: PhantomData,
        }
    }

    pub const fn as_ptr(&self) -> *const PointeeData<R> {
        self.data.as_ptr()
    }

    pub const fn metadata(&self) -> R::Metadata {
        self.metadata
    }

    /// Recover the shared reference.
    ///
    /// # Safety
    /// The parts must describe a valid `R` that can be shared for `'a`.
    pub unsafe fn into_ref(self) -> &'a R {
        unsafe { R::from_raw_parts(self.data.as_ptr().cast(), self.metadata) }
    }
}

impl<R: Wide + ?Sized> Copy for WideRef<'_, R> {}
impl<R: Wide + ?Sized> Clone for WideRef<'_, R> {
    fn clone(&self) -> Self {
        *self
    }
}

/// An exclusive wide reference represented as a data pointer and metadata.
///
/// The data pointer is non-null.
#[derive(RustSpec)]
#[repr(C)]
pub struct WideRefMut<'a, R: Wide + ?Sized> {
    data: NonNull<PointeeData<R>>,
    metadata: R::Metadata,
    lifetime: PhantomData<&'a mut ()>,
}

impl<'a, R: Wide + ?Sized> WideRefMut<'a, R> {
    /// Wrap an exclusive wide reference without changing its pointee or metadata.
    pub fn from_mut(source: &'a mut R) -> Self {
        let source = core::ptr::from_mut(source);
        let data = NonNull::new(R::as_mut_ptr(source).cast())
            .expect("Wide::as_mut_ptr returned null for a reference");
        unsafe { Self::from_raw_parts_mut(data, R::metadata(source)) }
    }

    /// Construct an exclusive wide reference from its raw parts.
    ///
    /// # Safety
    /// The parts must describe a valid `R` that can be exclusively borrowed for `'a`.
    pub const unsafe fn from_raw_parts_mut(
        data: NonNull<PointeeData<R>>,
        metadata: R::Metadata,
    ) -> Self {
        Self {
            data,
            metadata,
            lifetime: PhantomData,
        }
    }

    pub const fn as_ptr(&self) -> *mut PointeeData<R> {
        self.data.as_ptr()
    }

    pub const fn metadata(&self) -> R::Metadata {
        self.metadata
    }

    /// Recover the exclusive reference.
    ///
    /// # Safety
    /// The parts must describe a valid `R` that can be exclusively borrowed for `'a`.
    pub unsafe fn into_mut(self) -> &'a mut R {
        unsafe { R::from_raw_parts_mut(self.data.cast().as_ptr(), self.metadata) }
    }
}

/// An owned wide pointer represented as a data pointer and metadata.
///
/// The data pointer is non-null. Dropping this value drops the owned allocation.
/// Decoding a non-null carrier requires the caller to transfer unique ownership
/// of an allocation that can be recovered with [`Wide::from_non_null`].
#[cfg(feature = "alloc")]
#[repr(C)]
pub struct WideBox<R: Wide + ?Sized> {
    data: NonNull<PointeeData<R>>,
    metadata: R::Metadata,
}

// The header fields determine alignment, but the full pointee determines
// the properties reachable through the data pointer.
#[cfg(feature = "alloc")]
unsafe impl<R> RustSpec for WideBox<R>
where
    R: Wide + RustSpec<Mutability: rust_spec::mutability::PointerMutability> + ?Sized,
    R::Metadata: RustSpec,
    R::Layout: core::ops::Add<<R::Metadata as RustSpec>::Layout>,
    <usize as RustSpec>::__Alignment: rust_spec::Max<<R::Metadata as RustSpec>::__Alignment>,
    <R::Mutability as rust_spec::mutability::PointerMutability>::Output:
        core::ops::Add<<R::Metadata as RustSpec>::Mutability>,
    R::Trap: core::ops::Add<<R::Metadata as RustSpec>::__IndirectTrap>,
{
    type Layout = <R::Layout as core::ops::Add<<R::Metadata as RustSpec>::Layout>>::Output;
    type Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>;
    type __Alignment = rust_spec::Maximum<
        <usize as RustSpec>::__Alignment,
        <R::Metadata as RustSpec>::__Alignment,
    >;
    type Trap = rust_spec::layout::NonRobust;
    type Niche = rust_spec::niche::WithNiche<rust_spec::Stable>;
    type Drop = rust_spec::drop::CustomDrop<rust_spec::drop::AutoDrop>;
    type Mutability =
        <<R::Mutability as rust_spec::mutability::PointerMutability>::Output as core::ops::Add<
            <R::Metadata as RustSpec>::Mutability,
        >>::Output;
    type Opacity = rust_spec::NonOpaque;
    type __IndirectTrap =
        <R::Trap as core::ops::Add<<R::Metadata as RustSpec>::__IndirectTrap>>::Output;
}

#[cfg(feature = "alloc")]
impl<R: Wide + ?Sized> WideBox<R> {
    /// Transfer a box without reallocating it or changing its metadata.
    pub fn from_box(source: Box<R>) -> Self {
        let metadata = R::metadata(&*source);
        let data = R::into_non_null(source).cast();
        unsafe { Self::from_raw_parts(data, metadata) }
    }

    /// Construct an owned wide pointer from its raw parts.
    ///
    /// # Safety
    /// The parts must represent unique ownership of a live allocation that
    /// can be recovered with [`Wide::from_non_null`].
    pub const unsafe fn from_raw_parts(
        data: NonNull<PointeeData<R>>,
        metadata: R::Metadata,
    ) -> Self {
        Self { data, metadata }
    }

    pub const fn as_ptr(&self) -> *mut PointeeData<R> {
        self.data.as_ptr()
    }

    pub const fn metadata(&self) -> R::Metadata {
        self.metadata
    }

    /// Recover the box.
    ///
    /// # Safety
    /// The parts must represent unique ownership of a live allocation that
    /// can be recovered with [`Wide::from_non_null`].
    pub unsafe fn into_box(self) -> Box<R> {
        let value = core::mem::ManuallyDrop::new(self);
        unsafe { R::from_non_null(value.data.cast(), value.metadata) }
    }
}

#[cfg(feature = "alloc")]
impl<R: Wide + ?Sized> Drop for WideBox<R> {
    fn drop(&mut self) {
        // The handle owns the allocation until it is consumed or dropped.
        drop(unsafe { R::from_non_null(self.data.cast(), self.metadata) });
    }
}

#[cfg(feature = "alloc")]
impl<R: Wide + ToOwned<Owned: Into<Box<R>>> + ?Sized> ToOwned for WideBox<R> {
    type Owned = Self;

    fn to_owned(&self) -> Self::Owned {
        // The owned allocation remains live and shared for the duration of the copy.
        let pointee = unsafe { R::from_raw_parts(self.data.cast().as_ptr(), self.metadata) };
        Self::from_box(pointee.to_owned().into())
    }
}

/// Shared reference to a C-compatible wide pointee.
///
/// A null data pointer represents an absent reference.
#[derive(RustSpec)]
#[repr(C)]
#[doc(hidden)]
pub struct CSoftWideRef<R: ReprC<CType: Wide> + ?Sized, const STABLE: bool = false> {
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
#[doc(hidden)]
pub struct CSoftWideRefMut<
    R: ReprC<CType: Wide> + ?Sized,
    const EXCLUSIVE: bool,
    const STABLE: bool = false,
> {
    data: *mut PointeeData<R::CType>,
    metadata: <R::CType as Wide>::Metadata,
}

impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const STABLE: bool>
    CSoftWideRef<R, STABLE>
{
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

impl<
    R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized,
    const EXCLUSIVE: bool,
    const STABLE: bool,
> CSoftWideRefMut<R, EXCLUSIVE, STABLE>
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

impl<R: ReprC<CType: Wide> + ?Sized, const STABLE: bool> CSoftWideRef<R, STABLE> {
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

impl<R: ReprC<CType: Wide> + ?Sized, const STABLE: bool> CSoftWideRefMut<R, true, STABLE> {
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

/// Shared reference to a C-compatible wide pointee.
///
/// A null data pointer is the niche. `STABLE` distinguishes `WideRef`
/// (`true`) from native Rust references (`false`).
#[derive(RustSpec)]
#[repr(C)]
#[doc(hidden)]
pub struct CWideRef<'a, R: ReprC<CType: Wide> + ?Sized, const STABLE: bool = false> {
    data: *const PointeeData<R::CType>,
    metadata: <R::CType as Wide>::Metadata,
    lifetime: PhantomData<&'a ()>,
}

/// Wide carrier with a writable data pointer.
///
/// `STABLE = true` identifies an exclusive `WideRefMut`. The native shared
/// interior-mutable path uses `STABLE = false`.
#[derive(RustSpec)]
#[repr(C)]
#[doc(hidden)]
pub struct CWideRefMut<'a, R: ReprC<CType: Wide> + ?Sized, const STABLE: bool = false> {
    data: *mut PointeeData<R::CType>,
    metadata: <R::CType as Wide>::Metadata,
    lifetime: PhantomData<&'a ()>,
}

impl<R: ReprC<CType: Wide> + ?Sized, const STABLE: bool> CWideRefMut<'_, R, STABLE> {
    pub(crate) const fn data(&self) -> *mut PointeeData<R::CType> {
        self.data
    }

    pub(crate) const fn is_null(&self) -> bool {
        self.data.is_null()
    }
}

impl<'a, R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const STABLE: bool>
    CWideRefMut<'a, R, STABLE>
{
    /// Construct an absent carrier with initialized metadata.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null_mut(),
            metadata: unsafe { core::mem::zeroed() },
            lifetime: PhantomData,
        }
    }

    /// Construct the carrier from raw parts.
    ///
    /// # Safety
    /// Non-null parts must describe a valid pointee for `'a`. With `STABLE = true`,
    /// access must be exclusive. Otherwise, writes must obey the pointee's
    /// interior-mutability and synchronization rules.
    pub const unsafe fn from_raw_parts_mut(
        data: *mut PointeeData<R::CType>,
        metadata: <R::CType as Wide>::Metadata,
    ) -> Self {
        Self {
            data,
            metadata,
            lifetime: PhantomData,
        }
    }

    /// Wrap an existing mutable wide reference.
    pub fn from_mut(source: &'a mut R::CType) -> Self {
        let source = core::ptr::from_mut(source);
        Self {
            data: R::CType::as_mut_ptr(source).cast(),
            metadata: R::CType::metadata(source),
            lifetime: PhantomData,
        }
    }
}

impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const STABLE: bool> Default
    for CWideRefMut<'_, R, STABLE>
{
    fn default() -> Self {
        Self::null()
    }
}

impl<'a, R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const STABLE: bool>
    CWideRef<'a, R, STABLE>
{
    /// Construct an absent carrier with initialized metadata.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null(),
            // CType requires every bit pattern to be valid.
            metadata: unsafe { core::mem::zeroed() },
            lifetime: PhantomData,
        }
    }

    /// Create a carrier from a data pointer and metadata.
    ///
    /// # Safety
    /// A non-null pointer and metadata must describe a valid pointee that can be shared for `'a`.
    pub const unsafe fn from_raw_parts(
        data: *const PointeeData<R::CType>,
        metadata: <R::CType as Wide>::Metadata,
    ) -> Self {
        Self {
            data,
            metadata,
            lifetime: PhantomData,
        }
    }

    /// Wrap an existing wide reference.
    pub fn from_ref(source: &'a R::CType) -> Self {
        Self {
            data: R::CType::as_ptr(source).cast(),
            metadata: R::CType::metadata(source),
            lifetime: PhantomData,
        }
    }
}

impl<'a, R: ReprC<CType: Wide> + ?Sized, const STABLE: bool> CWideRef<'a, R, STABLE> {
    #[allow(dead_code)]
    pub(crate) const fn data(&self) -> *const PointeeData<R::CType> {
        self.data
    }
}

impl<'a, R: ReprC<CType: Wide> + ?Sized, const STABLE: bool> CWideRef<'a, R, STABLE> {
    /// Recover a shared wide reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer and metadata must describe a valid `P` for the returned lifetime.
    pub unsafe fn into_ref(self) -> Option<&'a R::CType> {
        if self.data.is_null() {
            return None;
        }

        let metadata = self.metadata;
        let data = self.data.cast();

        Some(unsafe { R::CType::from_raw_parts(data, metadata) })
    }
}

// All wide reference carriers preserve the same raw pointer and metadata.
macro_rules! impl_wide_ref_pointer_methods {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*], [$($target:tt)*], [$($suffix:tt)*], $ptr:ident, [$($marker:tt)*]) => {
        impl<$($params)*> $ty<$($args)*> {
            /// Cast the pointee while preserving its address and metadata.
            ///
            /// # Safety
            /// The result must satisfy the destination's raw constructor contract.
            #[allow(dead_code)]
            pub(crate) const unsafe fn cast<U>(self) -> $ty<$($target)* U $($suffix)*>
            where U: ReprC<CType: Wide<Metadata = <R::CType as Wide>::Metadata>> + ?Sized {
                $ty { data: self.data.cast(), metadata: self.metadata, $($marker)* }
            }
            /// Return the stored data pointer.
            pub const fn as_ptr(&self) -> *$ptr PointeeData<R::CType> { self.data }
            /// Return the unchanged metadata.
            pub const fn metadata(&self) -> <R::CType as Wide>::Metadata { self.metadata }
        }
    };
}
impl_wide_ref_pointer_methods! { CSoftWideRef, [R: ReprC<CType: Wide> + ?Sized, const STABLE: bool], [R, STABLE], [], [, STABLE], const, [] }
impl_wide_ref_pointer_methods! { CSoftWideRefMut, [R: ReprC<CType: Wide> + ?Sized, const EXCLUSIVE: bool, const STABLE: bool], [R, EXCLUSIVE, STABLE], [], [, EXCLUSIVE, STABLE], mut, [] }
impl_wide_ref_pointer_methods! { CWideRef, ['a, R: ReprC<CType: Wide> + ?Sized, const STABLE: bool], ['a, R, STABLE], ['a,], [, STABLE], const, [lifetime: PhantomData,] }
impl_wide_ref_pointer_methods! { CWideRefMut, ['a, R: ReprC<CType: Wide> + ?Sized, const STABLE: bool], ['a, R, STABLE], ['a,], [, STABLE], mut, [lifetime: PhantomData,] }

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
                self
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
        impl<$($params)*> crate::convert::EncodeUnchecked for $ty<$($args)*> {}
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
        $crate::__impl_borrowed_identity!([$($params)*] $ty<$($args)*>);
    };
}

/// Selects the C carrier for a direct shared wide reference.
#[doc(hidden)]
pub trait WideRefLayout<'a, R: Wide + ?Sized>: rust_spec::layout::LayoutKind {
    type Carrier: CType + Copy;
    const NICHE: Self::Carrier;
    fn is_niche(value: &Self::Carrier) -> bool;
}

impl<'a, R> WideRefLayout<'a, R> for rust_spec::Stable
where
    R: ReprC<CType: Wide<Header: CType + WideHeader<Data: CType>>> + Wide + ?Sized,
{
    type Carrier = CWideRef<'a, R, true>;
    const NICHE: Self::Carrier = CWideRef::null();

    fn is_niche(value: &Self::Carrier) -> bool {
        value.data.is_null()
    }
}

impl<'a, R> WideRefLayout<'a, R> for rust_spec::Unstable
where
    R: ReprC<CType: Wide<Header: CType + WideHeader<Data: CType>>> + Wide + ?Sized,
{
    type Carrier = CSoftWideRef<R, true>;
    const NICHE: Self::Carrier = CSoftWideRef::null();

    fn is_niche(value: &Self::Carrier) -> bool {
        value.data.is_null()
    }
}

impl<'a, R> ReprC for WideRef<'a, R>
where
    R: Wide + RustSpec<Layout: WideRefLayout<'a, R>> + ?Sized,
{
    type CType = <R::Layout as WideRefLayout<'a, R>>::Carrier;
}

impl<'a, R> crate::niche::Niche for WideRef<'a, R>
where
    R: Wide + RustSpec<Layout: WideRefLayout<'a, R>> + ?Sized,
{
    const NICHE: Self::CType = R::Layout::NICHE;

    fn is_niche(value: &Self::CType) -> bool {
        R::Layout::is_niche(value)
    }
}

/// Selects the C carrier for a direct exclusive wide reference.
#[doc(hidden)]
pub trait WideRefMutLayout<'a, R: Wide + ?Sized> {
    type Carrier: CType + Copy;
    const NICHE: Self::Carrier;
    fn is_niche(value: &Self::Carrier) -> bool;
}

impl<'a, R> WideRefMutLayout<'a, R> for (rust_spec::Stable, rust_spec::layout::Robust)
where
    R: ReprC<CType: Wide<Header: CType + WideHeader<Data: CType>>> + Wide + ?Sized,
{
    type Carrier = CWideRefMut<'a, R, true>;
    const NICHE: Self::Carrier = CWideRefMut::null();

    fn is_niche(value: &Self::Carrier) -> bool {
        value.data.is_null()
    }
}

impl<'a, R> WideRefMutLayout<'a, R> for (rust_spec::Stable, rust_spec::layout::NonRobust)
where
    R: ReprC<CType: Wide<Header: CType + WideHeader<Data: CType>>> + Wide + ?Sized,
{
    type Carrier = CSoftWideRefMut<R, true, true>;
    const NICHE: Self::Carrier = CSoftWideRefMut::null();

    fn is_niche(value: &Self::Carrier) -> bool {
        value.data.is_null()
    }
}

impl<'a, R, Trap> WideRefMutLayout<'a, R> for (rust_spec::Unstable, Trap)
where
    R: ReprC<CType: Wide<Header: CType + WideHeader<Data: CType>>> + Wide + ?Sized,
{
    type Carrier = CSoftWideRefMut<R, true, true>;
    const NICHE: Self::Carrier = CSoftWideRefMut::null();

    fn is_niche(value: &Self::Carrier) -> bool {
        value.data.is_null()
    }
}

impl<'a, R> ReprC for WideRefMut<'a, R>
where
    R: Wide + RustSpec + ?Sized,
    (R::Layout, R::Trap): WideRefMutLayout<'a, R>,
{
    type CType = <(R::Layout, R::Trap) as WideRefMutLayout<'a, R>>::Carrier;
}

impl<'a, R> crate::niche::Niche for WideRefMut<'a, R>
where
    R: Wide + RustSpec + ?Sized,
    (R::Layout, R::Trap): WideRefMutLayout<'a, R>,
{
    const NICHE: Self::CType = <(R::Layout, R::Trap)>::NICHE;

    fn is_niche(value: &Self::CType) -> bool {
        <(R::Layout, R::Trap)>::is_niche(value)
    }
}

macro_rules! impl_non_null_wide {
    ([$($lifetime:lifetime $($mutability:ident)?,)?] $direct:ident => $carrier:ident [$($flags:tt)*]) => {
        impl<$($lifetime,)? R> ReprC for $direct<$($lifetime,)? R>
        where
            R: ReprC<CType: Wide<Header: CType + WideHeader<Data: CType>>> + Wide + ?Sized,
        {
            type CType = $carrier<$($lifetime,)? R, $($flags)* true>;
        }

        impl<$($lifetime,)? R> crate::niche::Niche for $direct<$($lifetime,)? R>
        where
            R: ReprC<CType: Wide<Header: CType + WideHeader<Data: CType>>> + Wide + ?Sized,
        {
            const NICHE: Self::CType = $carrier::null();

            fn is_niche(value: &Self::CType) -> bool {
                value.data.is_null()
            }
        }

        impl_non_null_wide! { @codecs [$($lifetime $($mutability)?,)?] $direct => $carrier [$($flags)*] }
    };
    (@codecs [$($lifetime:lifetime $($mutability:ident)?,)?] $direct:ident => $carrier:ident [$($flags:tt)*]) => {
        // Identity pointees preserve the address and the meaning of the metadata.
        unsafe impl<$($lifetime,)? R> EncodeMoveUnchecked for $direct<$($lifetime,)? R>
        where
            R: ReprC<CType = R> + Wide<Header: CType + WideHeader<Data: CType>> + ?Sized,
            Self: ReprC<CType = $carrier<$($lifetime,)? R, $($flags)* true>>,
        {
            type SoftStore = ();

            unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                let value = core::mem::ManuallyDrop::new(self);
                $carrier {
                    data: value.data.as_ptr(),
                    metadata: value.metadata,
                    $(lifetime: PhantomData::<&$lifetime ()>,)?
                }
            }
        }

        unsafe impl<$($lifetime,)? R> EncodeMove for $direct<$($lifetime,)? R>
        where
            R: ReprC<CType = R> + Wide<Header: CType + WideHeader<Data: CType>> + ?Sized,
            Self: ReprC<CType = $carrier<$($lifetime,)? R, $($flags)* true>>,
        {
            type Store = ();

            fn soft_encode<'itm>(self, store: &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                unsafe { self.soft_encode_unchecked(store) }
            }
        }

        unsafe impl<'d, $($lifetime,)? R> DecodeMove<'d> for $direct<$($lifetime,)? R>
        where
            R: ReprC<CType = R> + Wide<Header: CType + WideHeader<Data: CType>> + ?Sized,
            Self: ReprC<CType = $carrier<$($lifetime,)? R, $($flags)* true>>,
        {
            type Store = ();

            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                let data = NonNull::new(source.data as *mut PointeeData<R>)?;
                Some(Self {
                    data,
                    metadata: source.metadata,
                    $(lifetime: PhantomData::<&$lifetime $($mutability)? ()>,)?
                })
            }
        }

        impl<$($lifetime,)? R: Wide + ?Sized> Encode for $direct<$($lifetime,)? R>
        where Self: EncodeMove {}
        impl<$($lifetime,)? R: Wide + ?Sized> crate::convert::EncodeUnchecked for $direct<$($lifetime,)? R>
        where Self: EncodeMoveUnchecked {}
        impl<'d, $($lifetime,)? R: Wide + ?Sized> Decode<'d> for $direct<$($lifetime,)? R>
        where Self: DecodeMove<'d> {}

    };
}

crate::__impl_borrowed_identity!(['a, R: Wide + ?Sized] WideRef<'a, R>);
crate::__impl_borrowed_identity!(['a, R: Wide + ?Sized] WideRefMut<'a, R>);

impl_non_null_wide! { @codecs ['a,] WideRef => CWideRef [] }
impl_non_null_wide! { @codecs ['a mut,] WideRefMut => CWideRefMut [] }
#[cfg(feature = "alloc")]
impl_non_null_wide! { [] WideBox => CWideBox [] }

impl<'a, R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const STABLE: bool>
    Default for CWideRef<'a, R, STABLE>
{
    fn default() -> Self {
        Self::null()
    }
}

unsafe impl<R: ReprC<CType: Wide> + ?Sized, const STABLE: bool> Zeroable
    for CWideRefMut<'_, R, STABLE>
{
}

impl_wide_raw_methods! { CWideRef, ['a, R: ReprC<CType: Wide> + ?Sized, const STABLE: bool], ['a, R, STABLE] }
impl_wide_raw_methods! { CWideRefMut, ['a, R: ReprC<CType: Wide> + ?Sized, const STABLE: bool], ['a, R, STABLE] }
impl_carrier_borrow! { CWideRef, ['a, R: ReprC<CType: Wide> + ?Sized, const STABLE: bool], ['a, R, STABLE] }
impl_carrier_borrow! { CWideRefMut, ['a, R: ReprC<CType: Wide> + ?Sized, const STABLE: bool], ['a, R, STABLE] }
impl_identity_carrier! { CWideRef, ['a, R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const STABLE: bool], ['a, R, STABLE] }
impl_identity_carrier! { CWideRefMut, ['a, R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const STABLE: bool], ['a, R, STABLE] }
unsafe impl<'a, R: ReprC<CType: Wide> + ?Sized, const STABLE: bool> Zeroable
    for CWideRef<'a, R, STABLE>
{
}

impl_wide_raw_methods! { CSoftWideRef, [R: ReprC<CType: Wide> + ?Sized, const STABLE: bool], [R, STABLE] }
impl_wide_raw_methods! { CSoftWideRefMut, [R: ReprC<CType: Wide> + ?Sized, const EXCLUSIVE: bool, const STABLE: bool], [R, EXCLUSIVE, STABLE] }

impl_carrier_borrow! { CSoftWideRef, [R: ReprC<CType: Wide> + ?Sized, const STABLE: bool], [R, STABLE] }
impl_carrier_borrow! { CSoftWideRefMut, [R: ReprC<CType: Wide> + ?Sized, const EXCLUSIVE: bool, const STABLE: bool], [R, EXCLUSIVE, STABLE] }

impl_identity_carrier! { CSoftWideRef, [R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const STABLE: bool], [R, STABLE] }
impl_identity_carrier! { CSoftWideRefMut, [R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const EXCLUSIVE: bool, const STABLE: bool], [R, EXCLUSIVE, STABLE] }

impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const STABLE: bool> Default
    for CSoftWideRef<R, STABLE>
{
    fn default() -> Self {
        Self::null()
    }
}
impl<
    R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized,
    const EXCLUSIVE: bool,
    const STABLE: bool,
> Default for CSoftWideRefMut<R, EXCLUSIVE, STABLE>
{
    fn default() -> Self {
        Self::null()
    }
}

#[cfg(feature = "alloc")]
/// Owned wide pointee with a defined C ABI layout.
///
/// If the data pointer is set to `null`, the struct represents an absent box.
/// `STABLE` selects the direct `WideBox` source when true.
#[derive(RustSpec)]
#[repr(C)]
#[doc(hidden)]
pub struct CWideBox<R: ReprC<CType: Wide> + ?Sized, const STABLE: bool = false> {
    data: *mut PointeeData<R::CType>,
    metadata: <R::CType as Wide>::Metadata,
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const STABLE: bool> Default
    for CWideBox<R, STABLE>
{
    fn default() -> Self {
        Self::null()
    }
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const STABLE: bool>
    CWideBox<R, STABLE>
{
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
impl<R: ReprC<CType: Wide> + ?Sized, const STABLE: bool> CWideBox<R, STABLE> {
    pub const fn as_ptr(self) -> *mut PointeeData<R::CType> {
        self.data
    }

    pub const fn metadata(&self) -> <R::CType as Wide>::Metadata {
        self.metadata
    }
}

#[cfg(feature = "alloc")]
impl_wide_raw_methods! { CWideBox, [R: ReprC<CType: Wide> + ?Sized, const STABLE: bool], [R, STABLE] }
#[cfg(feature = "alloc")]
impl_identity_carrier! { CWideBox, [R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const STABLE: bool], [R, STABLE] }
#[cfg(feature = "alloc")]
impl_carrier_borrow! { CWideBox, [R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const STABLE: bool], [R, STABLE] }

unsafe impl<R: ReprC<CType: Wide> + ?Sized, const STABLE: bool> Zeroable
    for CSoftWideRef<R, STABLE>
{
}
unsafe impl<R: ReprC<CType: Wide> + ?Sized, const EXCLUSIVE: bool, const STABLE: bool> Zeroable
    for CSoftWideRefMut<R, EXCLUSIVE, STABLE>
{
}
#[cfg(feature = "alloc")]
unsafe impl<R: ReprC<CType: Wide> + ?Sized, const STABLE: bool> Zeroable for CWideBox<R, STABLE> {}

#[cfg(feature = "alloc")]
pub(crate) use impl_carrier_borrow;
pub(crate) use impl_identity_carrier;

/// Raw parts shared by ordinary and soft wide reference carriers.
///
/// # Safety
/// Implementations must preserve the wide pointer ABI and their access contract.
pub(crate) unsafe trait WideRefCarrier<R: ReprC<CType: Wide> + ?Sized>:
    CType + Copy
{
    type Pointer: CType + Copy;
    fn as_ptr(&self) -> Self::Pointer;
    fn metadata(&self) -> <R::CType as Wide>::Metadata;
    /// # Safety
    /// The parts must satisfy the destination carrier's lifetime and aliasing contract.
    unsafe fn from_parts(data: Self::Pointer, metadata: <R::CType as Wide>::Metadata) -> Self;
}
unsafe impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> WideRefCarrier<R>
    for CSoftWideRef<R>
{
    type Pointer = *const PointeeData<R::CType>;
    fn as_ptr(&self) -> Self::Pointer {
        self.data
    }
    fn metadata(&self) -> <R::CType as Wide>::Metadata {
        self.metadata
    }
    unsafe fn from_parts(data: Self::Pointer, metadata: <R::CType as Wide>::Metadata) -> Self {
        unsafe { Self::from_raw_parts(data, metadata) }
    }
}
unsafe impl<'a, R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> WideRefCarrier<R>
    for CWideRef<'a, R>
{
    type Pointer = *const PointeeData<R::CType>;
    fn as_ptr(&self) -> Self::Pointer {
        self.data
    }
    fn metadata(&self) -> <R::CType as Wide>::Metadata {
        self.metadata
    }
    unsafe fn from_parts(data: Self::Pointer, metadata: <R::CType as Wide>::Metadata) -> Self {
        unsafe { Self::from_raw_parts(data, metadata) }
    }
}
unsafe impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const E: bool>
    WideRefCarrier<R> for CSoftWideRefMut<R, E>
{
    type Pointer = *mut PointeeData<R::CType>;
    fn as_ptr(&self) -> Self::Pointer {
        self.data
    }
    fn metadata(&self) -> <R::CType as Wide>::Metadata {
        self.metadata
    }
    unsafe fn from_parts(data: Self::Pointer, metadata: <R::CType as Wide>::Metadata) -> Self {
        unsafe { Self::from_raw_parts_mut(data, metadata) }
    }
}

unsafe impl<'a, R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const STABLE: bool>
    WideRefCarrier<R> for CWideRefMut<'a, R, STABLE>
where
    Self: CType,
{
    type Pointer = *mut PointeeData<R::CType>;
    fn as_ptr(&self) -> Self::Pointer {
        self.data
    }
    fn metadata(&self) -> <R::CType as Wide>::Metadata {
        self.metadata
    }
    unsafe fn from_parts(data: Self::Pointer, metadata: <R::CType as Wide>::Metadata) -> Self {
        unsafe { Self::from_raw_parts_mut(data, metadata) }
    }
}

/// Operations shared by ordinary and soft mutable wide carriers.
///
/// # Safety
/// Implementations must preserve the wide pointer ABI and exclusive access contract.
pub(crate) unsafe trait MutWideRefCarrier<R: ReprC<CType: Wide> + ?Sized>:
    CType + Copy
{
    fn data(&self) -> *mut PointeeData<R::CType>;
    fn metadata(&self) -> <R::CType as Wide>::Metadata;
    fn is_null(&self) -> bool {
        self.data().is_null()
    }
    /// # Safety
    /// The parts must satisfy the destination carrier's lifetime and aliasing contract.
    unsafe fn from_raw_parts_mut(
        data: *mut PointeeData<R::CType>,
        metadata: <R::CType as Wide>::Metadata,
    ) -> Self;
}
unsafe impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> MutWideRefCarrier<R>
    for CSoftWideRefMut<R, true>
{
    fn data(&self) -> *mut PointeeData<R::CType> {
        self.data
    }
    fn metadata(&self) -> <R::CType as Wide>::Metadata {
        self.metadata
    }
    unsafe fn from_raw_parts_mut(
        data: *mut PointeeData<R::CType>,
        metadata: <R::CType as Wide>::Metadata,
    ) -> Self {
        unsafe { Self::from_raw_parts_mut(data, metadata) }
    }
}
unsafe impl<'a, R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized>
    MutWideRefCarrier<R> for CWideRef<'a, R>
where
    Self: CType,
{
    fn data(&self) -> *mut PointeeData<R::CType> {
        self.data.cast_mut()
    }
    fn metadata(&self) -> <R::CType as Wide>::Metadata {
        self.metadata
    }
    unsafe fn from_raw_parts_mut(
        data: *mut PointeeData<R::CType>,
        metadata: <R::CType as Wide>::Metadata,
    ) -> Self {
        unsafe { Self::from_raw_parts(data.cast_const(), metadata) }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(all(feature = "derive", feature = "alloc"))]
    #[test]
    fn box_tail_does_not_generate_wide_headers() {
        // These names would collide if the derive emitted speculative headers.
        struct PacketData;
        struct CPacketData;
        #[derive(crate::ReprC, rust_spec::RustSpec)]
        struct Packet {
            tag: u8,
            tail: alloc::boxed::Box<(u32,)>,
        }
        let _ = (PacketData, CPacketData);
        static_assertions::assert_not_impl_any!(Packet: super::Wide);
        let source = Packet {
            tag: 7,
            tail: alloc::boxed::Box::new((42,)),
        };
        let mut encode_store = Default::default();
        let encoded = crate::convert::EncodeMove::soft_encode(source, &mut encode_store);
        let mut decode_store = Default::default();
        let decoded = unsafe {
            <Packet as crate::convert::DecodeMove>::soft_decode(encoded, &mut decode_store)
        }
        .unwrap();
        assert_eq!((decoded.tag, decoded.tail.0), (7, 42));
    }

    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, vec};
    #[cfg(feature = "alloc")]
    use core::ptr::NonNull;
    use core::{
        cell::{Cell, UnsafeCell},
        mem::ManuallyDrop,
    };

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    #[cfg(feature = "alloc")]
    use super::CWideBox;
    use super::{CSoftWideRef, CSoftWideRefMut, Wide, WideRef, WideRefMut};
    use crate::CType;

    #[repr(C)]
    #[derive(
        Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, crate::rust_spec::RustSpec,
    )]
    pub(crate) struct Metadata {
        pub(crate) length: usize,
        pub(crate) marker: u32,
    }
    unsafe impl CType for Metadata {}
    unsafe impl crate::Zeroable for Metadata {}

    #[repr(transparent)]
    #[derive(crate::rust_spec::RustSpec)]
    pub(crate) struct Custom(pub(crate) [u8]);
    unsafe impl CType for Custom {}
    // Identity lowering preserves the complete pointee ABI.
    unsafe impl crate::transmute::AbiCompatible for Custom {}
    impl crate::ReprC for Custom {
        type CType = Self;
    }

    unsafe impl Wide for Custom {
        type Header = [u8; 0];
        type Metadata = Metadata;

        fn metadata(ptr: *const Self) -> Metadata {
            Metadata {
                length: (ptr as *const [u8]).len(),
                marker: 7,
            }
        }
        fn as_ptr(ptr: *const Self) -> *const Self::Header {
            ptr.cast()
        }
        fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Header {
            ptr.cast()
        }
        unsafe fn from_raw_parts<'a>(data: *const Self::Header, metadata: Metadata) -> &'a Self {
            let slice = core::ptr::slice_from_raw_parts(data.cast::<u8>(), metadata.length);
            unsafe { &*(slice as *const Self) }
        }
        unsafe fn from_raw_parts_mut<'a>(
            data: *mut Self::Header,
            metadata: Metadata,
        ) -> &'a mut Self {
            let slice = core::ptr::slice_from_raw_parts_mut(data.cast::<u8>(), metadata.length);
            unsafe { &mut *(slice as *mut Self) }
        }
        #[cfg(feature = "alloc")]
        fn into_non_null(self: Box<Self>) -> NonNull<Self::Header> {
            Box::into_non_null(self).cast()
        }
        #[cfg(feature = "alloc")]
        unsafe fn from_non_null(data: NonNull<Self::Header>, metadata: Metadata) -> Box<Self> {
            let value = unsafe { Self::from_raw_parts_mut(data.as_ptr(), metadata) };
            unsafe { Box::from_raw(value as *mut Self) }
        }
    }

    #[test]
    fn unstable_wide_ref_selects_a_soft_carrier() {
        use crate::{ReprC, niche::Niche};

        type Source = WideRef<'static, [(u8,)]>;
        type Carrier = CSoftWideRef<[(u8,)], true>;
        static_assertions::assert_type_eq_all!(<Source as ReprC>::CType, Carrier);
        assert!(Source::is_niche(&Carrier::null()));
    }

    #[test]
    fn mutable_wide_ref_selects_soft_carriers_for_both_indirect_paths() {
        use crate::{ReprC, niche::Niche};

        type Stable = WideRefMut<'static, [bool]>;
        type Unstable = WideRefMut<'static, [(u8,)]>;
        static_assertions::assert_type_eq_all!(
            <Stable as ReprC>::CType,
            CSoftWideRefMut<[bool], true, true>
        );
        static_assertions::assert_type_eq_all!(
            <Unstable as ReprC>::CType,
            CSoftWideRefMut<[(u8,)], true, true>
        );
        assert!(Stable::is_niche(&<Stable as Niche>::NICHE));
        assert!(Unstable::is_niche(&<Unstable as Niche>::NICHE));
    }

    #[test]
    fn direct_wide_references_preserve_the_pointee_and_metadata() {
        assert_impl_all!(WideRef<'static, Custom>: Copy, crate::ReprC<CType = super::CWideRef<'static, Custom, true>>);
        assert_not_impl_any!(WideRef<'static, Custom>: CType);
        assert_impl_all!(WideRefMut<'static, Custom>: crate::ReprC<CType = super::CWideRefMut<'static, Custom, true>>);
        assert_not_impl_any!(WideRefMut<'static, Custom>: Copy);
        assert_not_impl_any!(WideRef<'static, [bool]>: CType);

        let mut values = [1_u8, 2, 3];
        let shared =
            WideRef::<Custom>::from_ref(unsafe { &*(&values[..] as *const [u8] as *const Custom) });
        assert_eq!(shared.as_ptr(), values.as_ptr());
        assert_eq!(
            shared.metadata(),
            Metadata {
                length: 3,
                marker: 7
            }
        );
        let null = super::CWideRef::<Custom, true>::null();
        assert!(unsafe { crate::decode::<WideRef<'_, Custom>>(null) }.is_none());
        let absent = crate::encode(None::<WideRef<'_, Custom>>);
        assert!(absent.0.as_ptr().is_null());
        assert_eq!(
            unsafe { crate::decode::<Option<WideRef<'_, Custom>>>(absent) }
                .map(|value| value.is_none()),
            Some(true)
        );
        let encoded = crate::encode(shared);
        let decoded = unsafe { crate::decode::<WideRef<'_, Custom>>(encoded) }.unwrap();
        assert_eq!(&unsafe { decoded.into_ref() }.0, &values);

        let null = super::CWideRefMut::<Custom, true>::null();
        assert!(unsafe { crate::decode::<WideRefMut<'_, Custom>>(null) }.is_none());

        let exclusive = WideRefMut::<Custom>::from_mut(unsafe {
            &mut *(&mut values[..] as *mut [u8] as *mut Custom)
        });
        assert_eq!(
            exclusive.metadata(),
            Metadata {
                length: 3,
                marker: 7
            }
        );
        let exclusive =
            unsafe { crate::decode::<WideRefMut<'_, Custom>>(crate::encode(exclusive)) }.unwrap();
        unsafe { exclusive.into_mut() }.0[0] = 4;
        assert_eq!(values, [4, 2, 3]);

        let rust_only = WideRef::<[bool]>::from_ref(&[true, false]);
        assert_eq!(unsafe { rust_only.into_ref() }, &[true, false]);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn direct_wide_box_transfers_the_original_allocation() {
        use super::WideBox;

        assert_impl_all!(WideBox<[u8]>: crate::ReprC<CType = CWideBox<[u8], true>>);
        assert_not_impl_any!(WideBox<[u8]>: Copy);
        assert_not_impl_any!(WideBox<[bool]>: CType);

        use alloc::{borrow::ToOwned, rc::Rc};

        assert_impl_all!(WideBox<[bool]>: crate::rust_spec::RustSpec<
            Drop = crate::rust_spec::drop::CustomDrop<crate::rust_spec::drop::AutoDrop>,
            __IndirectTrap = crate::rust_spec::layout::NonRobust,
        >);
        let item = Rc::new(());
        let boxed = WideBox::from_box(vec![Rc::clone(&item)].into_boxed_slice());
        assert_eq!(Rc::strong_count(&item), 2);
        let cloned = boxed.to_owned();
        assert_ne!(boxed.as_ptr(), cloned.as_ptr());
        assert_eq!(Rc::strong_count(&item), 3);
        drop(boxed);
        assert_eq!(Rc::strong_count(&item), 2);
        drop(cloned);
        assert_eq!(Rc::strong_count(&item), 1);

        let text = WideBox::from_box(Box::<str>::from("wide"));
        let owned = text.to_owned();
        assert_ne!(text.as_ptr(), owned.as_ptr());
        assert_eq!(&*unsafe { owned.into_box() }, "wide");

        let null = CWideBox::<[u8], true>::null();
        assert!(unsafe { crate::decode::<WideBox<[u8]>>(null) }.is_none());
        let source = vec![1_u8, 2].into_boxed_slice();
        let address = source.as_ptr();
        let encoded = crate::encode(WideBox::from_box(source));
        let decoded = unsafe { crate::decode::<WideBox<[u8]>>(encoded) }.unwrap();
        let recovered = unsafe { decoded.into_box() };
        assert_eq!(recovered.as_ptr(), address);

        let source = vec![true, false].into_boxed_slice();
        let address = source.as_ptr();
        let carrier = WideBox::from_box(source);
        assert_eq!(carrier.as_ptr().cast_const(), address);
        assert_eq!(carrier.metadata(), 2);
        let recovered = unsafe { carrier.into_box() };
        assert_eq!(recovered.as_ptr(), address);
        assert_eq!(&*recovered, &[true, false]);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn wide_boxes_preserve_custom_metadata() {
        type Pointee = Custom;
        fn boxed() -> Box<Pointee> {
            let bytes = vec![1_u8, 2, 3].into_boxed_slice();
            unsafe { Box::from_raw(Box::into_raw(bytes) as *mut Pointee) }
        }
        let expected = Metadata {
            length: 3,
            marker: 7,
        };
        let carrier = CWideBox::<Pointee>::from_box(boxed());
        assert_eq!(carrier.metadata(), expected);
        assert_eq!(&unsafe { carrier.into_box() }.unwrap().0, &[1, 2, 3]);
        assert_eq!(
            CWideBox::<Pointee>::null().metadata(),
            Metadata {
                length: 0,
                marker: 0
            }
        );
    }

    #[test]
    fn carriers_preserve_non_slice_metadata() {
        type Pointee = Custom;
        let mut source = [3_u8, 5, 8];
        let value = unsafe { &mut *(source.as_mut_slice() as *mut [u8] as *mut Pointee) };
        let shared = CSoftWideRef::<Pointee>::from_ref(&*value);
        assert_eq!(
            shared.metadata(),
            Metadata {
                length: 3,
                marker: 7
            }
        );
        let raw =
            unsafe { CSoftWideRef::<Pointee>::from_raw_parts(shared.as_ptr(), shared.metadata()) };
        let decoded =
            unsafe { crate::soft_decode::<CSoftWideRef<Pointee>>(raw, &mut Default::default()) }
                .unwrap();
        assert_eq!(decoded.metadata(), shared.metadata());
        assert_eq!(unsafe { &shared.into_ref().unwrap().0 }, &[3, 5, 8]);

        let mutable = CSoftWideRefMut::<Pointee, true>::from_mut(value);
        assert_eq!(
            mutable.metadata(),
            Metadata {
                length: 3,
                marker: 7
            }
        );
        unsafe { mutable.into_mut().unwrap().0[1] = 9 };
        assert_eq!(source, [3, 9, 8]);

        let null = CSoftWideRef::<Pointee>::null();
        assert!(null.as_ptr().is_null());
        assert_eq!(
            null.metadata(),
            Metadata {
                length: 0,
                marker: 0
            }
        );
        let decoded =
            unsafe { crate::soft_decode::<CSoftWideRef<Pointee>>(null, &mut Default::default()) }
                .unwrap();
        assert!(unsafe { decoded.into_ref() }.is_none());
        static_assertions::assert_impl_all!(CSoftWideRef<Pointee>: CType, Copy);
    }

    #[test]
    fn comparisons_use_pointer_then_arbitrary_metadata() {
        type Pointee = Custom;
        let mut bytes = [0_u8; 2];
        let first = bytes.as_mut_ptr();
        let second = unsafe { first.add(1) };
        let a = Metadata {
            length: 0,
            marker: 1,
        };
        let b = Metadata {
            length: 0,
            marker: 2,
        };

        let one = unsafe { CSoftWideRef::<Pointee>::from_raw_parts(first, a) };
        let same_pointer = unsafe { CSoftWideRef::<Pointee>::from_raw_parts(first, b) };
        let different_pointer = unsafe { CSoftWideRef::<Pointee>::from_raw_parts(second, a) };
        assert!(one < same_pointer);
        assert_ne!(one, different_pointer);
        assert_eq!(
            one.partial_cmp(&different_pointer),
            Some(first.cmp(&second))
        );

        let one = unsafe { CSoftWideRefMut::<Pointee, true>::from_raw_parts_mut(first, a) };
        let same_pointer =
            unsafe { CSoftWideRefMut::<Pointee, true>::from_raw_parts_mut(first, b) };
        let different_pointer =
            unsafe { CSoftWideRefMut::<Pointee, true>::from_raw_parts_mut(second, a) };
        assert!(one < same_pointer);
        assert_ne!(one, different_pointer);
        assert_eq!(
            one.partial_cmp(&different_pointer),
            Some(first.cmp(&second))
        );

        let null = CSoftWideRef::<Pointee>::null();
        let other_null = unsafe { CSoftWideRef::<Pointee>::from_raw_parts(core::ptr::null(), a) };
        assert_ne!(null, other_null);
    }
    #[test]
    fn lifetime_wide_carriers_preserve_layout_and_metadata() {
        use core::mem::{align_of, size_of};

        use super::{CSoftWideRef, CSoftWideRefMut, CWideRef, CWideRefMut};
        assert_eq!(
            size_of::<CWideRef<'_, [u8]>>(),
            size_of::<CSoftWideRef<[u8]>>()
        );
        assert_eq!(
            align_of::<CWideRef<'_, [u8]>>(),
            align_of::<CSoftWideRef<[u8]>>()
        );
        assert_eq!(
            size_of::<CWideRefMut<'_, [u8]>>(),
            size_of::<CSoftWideRefMut<[u8], true>>()
        );
        let values = [1_u8, 2, 3];
        let shared = CWideRef::<[u8]>::from_ref(&values);
        assert_eq!(shared.metadata(), 3);
        assert_eq!(unsafe { shared.into_ref() }, Some(&values[..]));
        let mut values = [4_u8, 5];
        let exclusive: CWideRef<'_, [u8]> = crate::encode(&mut values[..]);
        unsafe { crate::decode::<&mut [u8]>(exclusive) }.unwrap()[1] = 6;
        assert_eq!(values, [4, 6]);
        assert!(unsafe { CWideRef::<[u8]>::null().into_ref() }.is_none());
    }

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
