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
#[cfg(feature = "alloc")]
use rust_spec::Unspecified;

use rust_spec::{
    RustSpec,
    size::{MetaSized, SliceLike},
};

use crate::{
    CFnArg, CFnReturn, CType, ReprC,
    convert::{Decode, DecodeMove, Encode, EncodeMove, EncodeMoveUnchecked},
    marker::{AbiCompatible, Zeroable},
};

#[cfg(feature = "repr-rust")]
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

macro_rules! impl_non_null_wide {
    ([$($lifetime:lifetime $($mutability:ident)?,)?] $direct:ident => $carrier:ident [$($flags:tt)*]) => {
        impl<$($lifetime,)? R> ReprC for $direct<$($lifetime,)? R>
        where
            R: ReprC<CType: Wide<Header: CType + WideHeader<Data: CType>>> + Wide + ?Sized,
        {
            type CType = abi::$carrier<$($lifetime,)? R::CType>;
        }

        impl<$($lifetime,)? R> crate::niche::Niche for $direct<$($lifetime,)? R>
        where
            R: ReprC<CType: Wide<Header: CType + WideHeader<Data: CType>>> + Wide + ?Sized,
        {
            const NICHE: Self::CType = abi::$carrier::null();

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
            R: CType + ReprC<CType = R> + Wide<Header: CType + WideHeader<Data: CType>> + ?Sized,
            Self: ReprC<CType = abi::$carrier<$($lifetime,)? <R as ReprC>::CType>>,
        {
            type SoftStore = ();

            unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                let value = core::mem::ManuallyDrop::new(self);
                abi::$carrier {
                    data: value.data.as_ptr(),
                    metadata: value.metadata,
                    $(lifetime: PhantomData::<&$lifetime ()>,)?
                }
            }
        }

        unsafe impl<$($lifetime,)? R> EncodeMove for $direct<$($lifetime,)? R>
        where
            R: CType + ReprC<CType = R> + Wide<Header: CType + WideHeader<Data: CType>> + ?Sized,
            Self: ReprC<CType = abi::$carrier<$($lifetime,)? <R as ReprC>::CType>>,
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
            R: CType + ReprC<CType = R> + Wide<Header: CType + WideHeader<Data: CType>> + ?Sized,
            Self: ReprC<CType = abi::$carrier<$($lifetime,)? <R as ReprC>::CType>>,
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

crate::borrow::__impl_borrowed_identity!(['a, R: Wide + ?Sized] WideRef<'a, R>);

impl_non_null_wide! { @codecs ['a,] WideRef => WideRef [] }

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

crate::borrow::__impl_borrowed_identity!(['a, R: Wide + ?Sized] WideRefMut<'a, R>);

impl_non_null_wide! { @codecs ['a mut,] WideRefMut => WideRefMut [] }

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
    type Niche = rust_spec::niche::WithNiche<Unspecified>;
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
impl_non_null_wide! { [] WideBox => WideBox [] }

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


        unsafe impl<$($params)*> $crate::convert::AbiCompatibleValidate for $ty<$($args)*>
where
    Self: $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Specified>,
{
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl<$($params)*> AbiCompatible for $ty<$($args)*>
where
    Self: $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Specified>,
{}
        unsafe impl<$($params)*> CType for $ty<$($args)*> {}
        unsafe impl<$($params)*> CFnArg for $ty<$($args)*> {}
        unsafe impl<$($params)*> CFnReturn for $ty<$($args)*> {}
    };
}

macro_rules! impl_carrier_borrow {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
        $crate::borrow::__impl_borrowed_identity!([$($params)*] $ty<$($args)*>);
    };
}

#[cfg(feature = "alloc")]
pub(crate) use impl_carrier_borrow;
pub(crate) use impl_identity_carrier;

#[doc(hidden)]
pub mod abi {
    use sealed::sealed;

    use super::*;

    /// Raw parts shared by ordinary and soft wide reference carriers.
    ///
    /// # Safety
    /// Implementations must preserve the wide pointer ABI and their access contract.
    #[sealed]
    #[doc(hidden)]
    pub unsafe trait WideRefCarrier: CType + Copy {
        type Pointee: CType + Wide + ?Sized;

        fn metadata(&self) -> <Self::Pointee as Wide>::Metadata;

        /// Read the data pointer without granting mutable access to the pointee.
        fn data(&self) -> *const PointeeData<Self::Pointee>;

        /// # Safety
        /// The parts must satisfy the destination carrier's lifetime and aliasing contract,
        /// including exclusive access when required, regardless of the pointer's constness.
        unsafe fn from_raw_parts(
            data: *const PointeeData<Self::Pointee>,
            metadata: <Self::Pointee as Wide>::Metadata,
        ) -> Self;
    }

    /// Operations shared by ordinary and soft mutable wide carriers.
    ///
    /// # Safety
    /// Implementations must preserve the wide pointer ABI and exclusive access contract.
    #[sealed]
    #[doc(hidden)]
    pub unsafe trait WideRefMutCarrier: CType + Copy {
        type Pointee: CType + Wide + ?Sized;

        fn metadata(&self) -> <Self::Pointee as Wide>::Metadata;

        fn data(&self) -> *mut PointeeData<Self::Pointee>;

        /// # Safety
        /// The parts must satisfy the destination carrier's lifetime and aliasing contract.
        unsafe fn from_raw_parts_mut(
            data: *mut PointeeData<Self::Pointee>,
            metadata: <Self::Pointee as Wide>::Metadata,
        ) -> Self;
    }

    /// Shared reference to a C-compatible wide pointee.
    ///
    /// A null data pointer represents an absent reference.
    #[derive(RustSpec)]
    #[doc(hidden)]
    #[repr(C)]
    pub struct SoftWideRef<R: CType + Wide + ?Sized> {
        pub(super) data: *const PointeeData<R>,
        pub(super) metadata: <R as Wide>::Metadata,
    }

    /// Mutable wide-reference carrier with a defined C ABI layout.
    ///
    /// Represents exclusive access that comes from `&mut T`.
    #[derive(RustSpec)]
    #[doc(hidden)]
    #[repr(C)]
    pub struct SoftWideRefMut<R: CType + Wide + ?Sized> {
        pub(super) data: *mut PointeeData<R>,
        pub(super) metadata: <R as Wide>::Metadata,
    }

    impl<R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> SoftWideRef<R> {
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
            data: *const PointeeData<R>,
            metadata: <R as Wide>::Metadata,
        ) -> Self {
            Self { data, metadata }
        }

        /// Wrap an existing wide reference.
        pub fn from_ref(source: &R) -> Self {
            Self {
                data: R::as_ptr(source).cast(),
                metadata: R::metadata(source),
            }
        }
    }

    impl<R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> SoftWideRefMut<R> {
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
        /// The pointer and metadata must describe a valid, exclusively accessible P.
        pub const unsafe fn from_raw_parts_mut(
            data: *mut PointeeData<R>,
            metadata: <R as Wide>::Metadata,
        ) -> Self {
            Self { data, metadata }
        }

        /// Wrap an existing mutable wide reference.
        pub fn from_mut(source: &mut R) -> Self {
            Self {
                data: R::as_mut_ptr(source).cast(),
                metadata: R::metadata(source),
            }
        }
    }

    impl<R: CType + Wide + ?Sized> SoftWideRef<R> {
        /// Recover a shared wide reference, returning `None` for a null pointer.
        ///
        /// # Safety
        ///
        /// The pointer and metadata must describe a valid `P` for the returned lifetime.
        pub unsafe fn into_ref<'a>(self) -> Option<&'a R> {
            if self.data.is_null() {
                return None;
            }

            let metadata = self.metadata;
            let data = self.data.cast();

            Some(unsafe { R::from_raw_parts(data, metadata) })
        }
    }

    impl<R: CType + Wide + ?Sized> SoftWideRefMut<R> {
        /// Recover a mutable wide reference, returning `None` for a null pointer.
        ///
        /// # Safety
        ///
        /// The pointer and metadata must describe a valid, exclusively accessible `P`
        /// for the returned lifetime.
        pub unsafe fn into_mut<'a>(self) -> Option<&'a mut R> {
            if self.data.is_null() {
                return None;
            }

            let metadata = self.metadata;
            let data = self.data.cast();

            Some(unsafe { R::from_raw_parts_mut(data, metadata) })
        }
    }

    /// Shared reference to a C-compatible wide pointee.
    ///
    /// A null data pointer is the niche. CO3 sources select this carrier directly.
    #[derive(RustSpec)]
    #[repr(C)]
    #[doc(hidden)]
    pub struct WideRef<'a, R: CType + Wide + ?Sized> {
        pub(super) data: *const PointeeData<R>,
        pub(super) metadata: <R as Wide>::Metadata,
        pub(super) lifetime: PhantomData<&'a ()>,
    }

    /// Wide carrier with a writable data pointer.
    ///
    /// Native and CO3 exclusive wide references select this carrier directly.
    #[derive(RustSpec)]
    #[repr(C)]
    #[doc(hidden)]
    pub struct WideRefMut<'a, R: CType + Wide + ?Sized> {
        pub(super) data: *mut PointeeData<R>,
        pub(super) metadata: <R as Wide>::Metadata,
        pub(super) lifetime: PhantomData<&'a ()>,
    }

    impl<'a, R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> WideRefMut<'a, R> {
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
        /// Non-null parts must describe a valid pointee for `'a`. For co3 exclusive references,
        /// access must be exclusive. Otherwise, writes must obey the pointee's
        /// interior-mutability and synchronization rules.
        pub const unsafe fn from_raw_parts_mut(
            data: *mut PointeeData<R>,
            metadata: <R as Wide>::Metadata,
        ) -> Self {
            Self {
                data,
                metadata,
                lifetime: PhantomData,
            }
        }

        /// Wrap an existing mutable wide reference.
        pub fn from_mut(source: &'a mut R) -> Self {
            let source = core::ptr::from_mut(source);
            Self {
                data: R::as_mut_ptr(source).cast(),
                metadata: R::metadata(source),
                lifetime: PhantomData,
            }
        }
    }

    impl<R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> Default for WideRefMut<'_, R> {
        fn default() -> Self {
            Self::null()
        }
    }

    impl<'a, R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> WideRef<'a, R> {
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
            data: *const PointeeData<R>,
            metadata: <R as Wide>::Metadata,
        ) -> Self {
            Self {
                data,
                metadata,
                lifetime: PhantomData,
            }
        }

        /// Wrap an existing wide reference.
        pub fn from_ref(source: &'a R) -> Self {
            Self {
                data: R::as_ptr(source).cast(),
                metadata: R::metadata(source),
                lifetime: PhantomData,
            }
        }
    }

    impl<'a, R: CType + Wide + ?Sized> WideRef<'a, R> {
        #[cfg(feature = "repr-rust")]
        pub(crate) const fn data(&self) -> *const PointeeData<R> {
            self.data
        }
    }

    impl<'a, R: CType + Wide + ?Sized> WideRef<'a, R> {
        /// Recover a shared wide reference, returning `None` for a null pointer.
        ///
        /// # Safety
        ///
        /// The pointer and metadata must describe a valid `P` for the returned lifetime.
        pub unsafe fn into_ref(self) -> Option<&'a R> {
            if self.data.is_null() {
                return None;
            }

            let metadata = self.metadata;
            let data = self.data.cast();

            Some(unsafe { R::from_raw_parts(data, metadata) })
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
            #[allow(dead_code, reason = "cast usage varies between generated wide carriers")]
            pub(crate) const unsafe fn cast<U>(self) -> $ty<$($target)* U $($suffix)*>
            where U: CType + Wide<Metadata = <R as Wide>::Metadata> + ?Sized {
                $ty { data: self.data.cast(), metadata: self.metadata, $($marker)* }
            }
            /// Return the stored data pointer.
            pub const fn as_ptr(&self) -> *$ptr PointeeData<R> { self.data }
            /// Return the unchanged metadata.
            pub const fn metadata(&self) -> <R as Wide>::Metadata { self.metadata }
        }
    };
}

    #[sealed]
    unsafe impl<C: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> WideRefCarrier
        for SoftWideRef<C>
    {
        type Pointee = C;

        fn metadata(&self) -> <C as Wide>::Metadata {
            self.metadata
        }
        fn data(&self) -> *const PointeeData<C> {
            self.data as *const _
        }
        unsafe fn from_raw_parts(
            data: *const PointeeData<C>,
            metadata: <C as Wide>::Metadata,
        ) -> Self {
            unsafe { Self::from_raw_parts(data, metadata) }
        }
    }
    #[sealed]
    unsafe impl<'a, C: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> WideRefCarrier
        for WideRef<'a, C>
    {
        type Pointee = C;

        fn metadata(&self) -> <C as Wide>::Metadata {
            self.metadata
        }
        fn data(&self) -> *const PointeeData<C> {
            self.data as *const _
        }
        unsafe fn from_raw_parts(
            data: *const PointeeData<C>,
            metadata: <C as Wide>::Metadata,
        ) -> Self {
            unsafe { Self::from_raw_parts(data, metadata) }
        }
    }
    #[sealed]
    unsafe impl<C: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> WideRefCarrier
        for SoftWideRefMut<C>
    {
        type Pointee = C;

        fn metadata(&self) -> <C as Wide>::Metadata {
            self.metadata
        }
        fn data(&self) -> *const PointeeData<C> {
            self.data as *const _
        }
        unsafe fn from_raw_parts(
            data: *const PointeeData<C>,
            metadata: <C as Wide>::Metadata,
        ) -> Self {
            unsafe { Self::from_raw_parts_mut(data.cast_mut(), metadata) }
        }
    }

    #[sealed]
    unsafe impl<'a, C: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> WideRefCarrier
        for WideRefMut<'a, C>
    where
        Self: CType,
    {
        type Pointee = C;

        fn metadata(&self) -> <C as Wide>::Metadata {
            self.metadata
        }
        fn data(&self) -> *const PointeeData<C> {
            self.data as *const _
        }
        unsafe fn from_raw_parts(
            data: *const PointeeData<C>,
            metadata: <C as Wide>::Metadata,
        ) -> Self {
            unsafe { Self::from_raw_parts_mut(data.cast_mut(), metadata) }
        }
    }

    #[sealed]
    unsafe impl<C: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> WideRefMutCarrier
        for SoftWideRefMut<C>
    {
        type Pointee = C;

        fn data(&self) -> *mut PointeeData<C> {
            self.data
        }
        fn metadata(&self) -> <C as Wide>::Metadata {
            self.metadata
        }
        unsafe fn from_raw_parts_mut(
            data: *mut PointeeData<C>,
            metadata: <C as Wide>::Metadata,
        ) -> Self {
            unsafe { Self::from_raw_parts_mut(data, metadata) }
        }
    }
    #[sealed]
    unsafe impl<'a, C: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> WideRefMutCarrier
        for WideRefMut<'a, C>
    where
        Self: CType,
    {
        type Pointee = C;

        fn data(&self) -> *mut PointeeData<C> {
            self.data
        }
        fn metadata(&self) -> <C as Wide>::Metadata {
            self.metadata
        }
        unsafe fn from_raw_parts_mut(
            data: *mut PointeeData<C>,
            metadata: <C as Wide>::Metadata,
        ) -> Self {
            unsafe { Self::from_raw_parts_mut(data, metadata) }
        }
    }

    impl_wide_ref_pointer_methods! { SoftWideRef, [R: CType + Wide + ?Sized], [R], [], [], const, [] }

    impl_wide_ref_pointer_methods! { SoftWideRefMut, [R: CType + Wide + ?Sized], [R], [], [], mut, [] }

    impl_wide_ref_pointer_methods! { WideRef, ['a, R: CType + Wide + ?Sized], ['a, R], ['a,], [], const, [lifetime: PhantomData,] }

    impl_wide_ref_pointer_methods! { WideRefMut, ['a, R: CType + Wide + ?Sized], ['a, R], ['a,], [], mut, [lifetime: PhantomData,] }

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

    impl<'a, R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> Default for WideRef<'a, R> {
        fn default() -> Self {
            Self::null()
        }
    }

    unsafe impl<R: CType + Wide + ?Sized> Zeroable for WideRefMut<'_, R> {}

    impl_wide_raw_methods! { WideRef, ['a, R: CType + Wide + ?Sized], ['a, R] }

    impl_wide_raw_methods! { WideRefMut, ['a, R: CType + Wide + ?Sized], ['a, R] }

    impl_carrier_borrow! { WideRef, ['a, R: CType + Wide + ?Sized], ['a, R] }

    impl_carrier_borrow! { WideRefMut, ['a, R: CType + Wide + ?Sized], ['a, R] }

    impl_identity_carrier! { WideRef, ['a, R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized], ['a, R] }

    impl_identity_carrier! { WideRefMut, ['a, R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized], ['a, R] }

    unsafe impl<'a, R: CType + Wide + ?Sized> Zeroable for WideRef<'a, R> {}

    impl_wide_raw_methods! { SoftWideRef, [R: CType + Wide + ?Sized], [R] }

    impl_wide_raw_methods! { SoftWideRefMut, [R: CType + Wide + ?Sized], [R] }

    impl_carrier_borrow! { SoftWideRef, [R: CType + Wide + ?Sized], [R] }

    impl_carrier_borrow! { SoftWideRefMut, [R: CType + Wide + ?Sized], [R] }

    impl_identity_carrier! { SoftWideRef, [R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized], [R] }

    impl_identity_carrier! { SoftWideRefMut, [R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized], [R] }

    impl<R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> Default for SoftWideRef<R> {
        fn default() -> Self {
            Self::null()
        }
    }

    impl<R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> Default for SoftWideRefMut<R> {
        fn default() -> Self {
            Self::null()
        }
    }

    #[cfg(feature = "alloc")]
    /// Owned wide pointee with a defined C ABI layout.
    ///
    /// If the data pointer is set to `null`, the struct represents an absent box.
    /// The direct `WideBox` source selects this carrier directly.
    #[derive(RustSpec)]
    #[repr(C)]
    #[doc(hidden)]
    pub struct WideBox<R: CType + Wide + ?Sized> {
        pub(super) data: *mut PointeeData<R>,
        pub(super) metadata: <R as Wide>::Metadata,
    }

    #[cfg(feature = "alloc")]
    impl<R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> Default for WideBox<R> {
        fn default() -> Self {
            Self::null()
        }
    }

    #[cfg(feature = "alloc")]
    impl<R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> WideBox<R> {
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
            data: NonNull<PointeeData<R>>,
            metadata: <R as Wide>::Metadata,
        ) -> Self {
            Self {
                data: data.as_ptr(),
                metadata,
            }
        }
        /// Create a carrier from a boxed value.
        pub fn from_box(source: Box<R>) -> Self {
            let metadata = R::metadata(&*source);
            let data = R::into_non_null(source).cast();
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
        pub unsafe fn into_box(self) -> Option<Box<R>> {
            let data = NonNull::<<R as Wide>::Header>::new(self.data.cast())?;
            Some(unsafe { R::from_non_null(data, self.metadata) })
        }

        pub(crate) const unsafe fn from_raw_nullable(
            data: *mut PointeeData<R>,
            metadata: <R as Wide>::Metadata,
        ) -> Self {
            Self { data, metadata }
        }
    }

    #[cfg(feature = "alloc")]
    impl<R: CType + Wide + ?Sized> WideBox<R> {
        pub const fn as_ptr(self) -> *mut PointeeData<R> {
            self.data
        }

        pub const fn metadata(&self) -> <R as Wide>::Metadata {
            self.metadata
        }
    }

    #[cfg(feature = "alloc")]
    impl_wide_raw_methods! { WideBox, [R: CType + Wide + ?Sized], [R] }

    #[cfg(feature = "alloc")]
    impl_identity_carrier! { WideBox, [R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized], [R] }

    #[cfg(feature = "alloc")]
    impl_carrier_borrow! { WideBox, [R: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized], [R] }

    unsafe impl<R: CType + Wide + ?Sized> Zeroable for SoftWideRef<R> {}

    unsafe impl<R: CType + Wide + ?Sized> Zeroable for SoftWideRefMut<R> {}

    #[cfg(feature = "alloc")]
    unsafe impl<R: CType + Wide + ?Sized> Zeroable for WideBox<R> {}
}

#[cfg(test)]
pub(crate) mod tests {
    use rust_spec::Unspecified;

    #[cfg(all(feature = "derive", feature = "alloc"))]
    #[test]
    fn box_tail_does_not_generate_wide_headers() {
        // These names would collide if the derive emitted speculative headers.
        struct PacketData;
        struct PacketDataCType;
        #[derive(crate::ReprC, rust_spec::RustSpec)]
        struct Packet {
            tag: u8,
            tail: alloc::boxed::Box<(u32,)>,
        }
        let _ = (PacketData, PacketDataCType);
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
    use alloc::boxed::Box;
    #[cfg(feature = "alloc")]
    use core::ptr::NonNull;
    use core::{
        cell::{Cell, UnsafeCell},
        mem::ManuallyDrop,
    };

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::{Wide, WideRef, WideRefMut, abi};
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
    unsafe impl crate::marker::Zeroable for Metadata {}

    #[repr(transparent)]
    #[derive(crate::rust_spec::RustSpec)]
    pub(crate) struct Custom(pub(crate) [u8]);
    unsafe impl CType for Custom {}
    // Identity lowering preserves the complete pointee ABI.
    unsafe impl crate::marker::AbiCompatible for Custom {}
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
        type Carrier = abi::SoftWideRef<<[(u8,)] as crate::ReprC>::CType>;
        static_assertions::assert_type_eq_all!(<Source as ReprC>::CType, Carrier);
        assert!(Source::is_niche(&abi::SoftWideRef::null()));
    }

    #[test]
    fn mutable_wide_ref_selects_soft_carriers_for_both_indirect_paths() {
        use crate::{ReprC, niche::Niche};

        type Stable = WideRefMut<'static, [bool]>;
        type Unspecified = WideRefMut<'static, [(u8,)]>;
        static_assertions::assert_type_eq_all!(
            <Stable as ReprC>::CType,
            abi::SoftWideRefMut<<[bool] as crate::ReprC>::CType>
        );
        static_assertions::assert_type_eq_all!(
            <Unspecified as ReprC>::CType,
            abi::SoftWideRefMut<<[(u8,)] as crate::ReprC>::CType>
        );
        assert!(Stable::is_niche(&<Stable as Niche>::NICHE));
        assert!(Unspecified::is_niche(&<Unspecified as Niche>::NICHE));
    }

    #[test]
    fn direct_wide_references_preserve_the_pointee_and_metadata() {
        assert_impl_all!(WideRef<'static, Custom>: Copy, crate::ReprC<CType = super::abi::WideRef<'static, <Custom as crate::ReprC>::CType>>);
        assert_not_impl_any!(WideRef<'static, Custom>: CType);
        assert_impl_all!(WideRefMut<'static, Custom>: crate::ReprC<CType = super::abi::WideRefMut<'static, <Custom as crate::ReprC>::CType>>);
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
        let null = super::abi::WideRef::<<Custom as crate::ReprC>::CType>::null();
        assert!(unsafe { crate::decode::<WideRef<'_, Custom>>(null) }.is_none());
        let absent = crate::encode(None::<WideRef<'_, Custom>>);
        assert!(absent.0.0.as_ptr().is_null());
        assert_eq!(
            unsafe { crate::decode::<Option<WideRef<'_, Custom>>>(absent) }
                .map(|value| value.is_none()),
            Some(true)
        );
        let encoded = crate::encode(shared);
        let decoded = unsafe { crate::decode::<WideRef<'_, Custom>>(encoded) }.unwrap();
        assert_eq!(&unsafe { decoded.into_ref() }.0, &values);

        let null = super::abi::WideRefMut::<<Custom as crate::ReprC>::CType>::null();
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

        assert_impl_all!(WideBox<[u8]>: crate::ReprC<CType = abi::WideBox<[u8]>>);
        assert_impl_all!(WideBox<[u8]>: rust_spec::RustSpec<
            Niche = rust_spec::niche::WithNiche<Unspecified>,
        >);
        assert_impl_all!(Option<WideBox<[u8]>>: rust_spec::RustSpec<Layout = Unspecified>);
        assert_not_impl_any!(WideBox<[u8]>: Copy);
        assert_not_impl_any!(WideBox<[bool]>: CType);

        use alloc::{borrow::ToOwned, rc::Rc};

        assert_impl_all!(WideBox<[bool]>: crate::rust_spec::RustSpec<
            Drop = crate::rust_spec::drop::CustomDrop<crate::rust_spec::drop::AutoDrop>,
            __IndirectTrap = crate::rust_spec::layout::NonRobust,
        >);
        let item = Rc::new(());
        let boxed = WideBox::from_box(alloc::vec![Rc::clone(&item)].into_boxed_slice());
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

        let null = abi::WideBox::<[u8]>::null();
        assert!(unsafe { crate::decode::<WideBox<[u8]>>(null) }.is_none());
        let source = alloc::vec![1_u8, 2].into_boxed_slice();
        let address = source.as_ptr();
        let encoded = crate::encode(WideBox::from_box(source));
        let decoded = unsafe { crate::decode::<WideBox<[u8]>>(encoded) }.unwrap();
        let recovered = unsafe { decoded.into_box() };
        assert_eq!(recovered.as_ptr(), address);

        let source = alloc::vec![true, false].into_boxed_slice();
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
            let bytes = alloc::vec![1_u8, 2, 3].into_boxed_slice();
            unsafe { Box::from_raw(Box::into_raw(bytes) as *mut Pointee) }
        }
        let expected = Metadata {
            length: 3,
            marker: 7,
        };
        let carrier = abi::WideBox::<<Pointee as crate::ReprC>::CType>::from_box(boxed());
        assert_eq!(carrier.metadata(), expected);
        assert_eq!(&unsafe { carrier.into_box() }.unwrap().0, &[1, 2, 3]);
        assert_eq!(
            abi::WideBox::<<Pointee as crate::ReprC>::CType>::null().metadata(),
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
        let shared = abi::SoftWideRef::<<Pointee as crate::ReprC>::CType>::from_ref(&*value);
        assert_eq!(
            shared.metadata(),
            Metadata {
                length: 3,
                marker: 7
            }
        );
        let raw = unsafe {
            abi::SoftWideRef::<<Pointee as crate::ReprC>::CType>::from_raw_parts(
                shared.as_ptr(),
                shared.metadata(),
            )
        };
        let decoded = unsafe {
            crate::soft_decode::<abi::SoftWideRef<<Pointee as crate::ReprC>::CType>>(
                raw,
                &mut Default::default(),
            )
        }
        .unwrap();
        assert_eq!(decoded.metadata(), shared.metadata());
        assert_eq!(unsafe { &shared.into_ref().unwrap().0 }, &[3, 5, 8]);

        let mutable = abi::SoftWideRefMut::<<Pointee as crate::ReprC>::CType>::from_mut(value);
        assert_eq!(
            mutable.metadata(),
            Metadata {
                length: 3,
                marker: 7
            }
        );
        unsafe { mutable.into_mut().unwrap().0[1] = 9 };
        assert_eq!(source, [3, 9, 8]);

        let null = abi::SoftWideRef::<<Pointee as crate::ReprC>::CType>::null();
        assert!(null.as_ptr().is_null());
        assert_eq!(
            null.metadata(),
            Metadata {
                length: 0,
                marker: 0
            }
        );
        let decoded = unsafe {
            crate::soft_decode::<abi::SoftWideRef<<Pointee as crate::ReprC>::CType>>(
                null,
                &mut Default::default(),
            )
        }
        .unwrap();
        assert!(unsafe { decoded.into_ref() }.is_none());
        static_assertions::assert_impl_all!(abi::SoftWideRef<<Pointee as crate::ReprC>::CType>: CType, Copy);
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

        let one = unsafe {
            abi::SoftWideRef::<<Pointee as crate::ReprC>::CType>::from_raw_parts(first, a)
        };
        let same_pointer = unsafe {
            abi::SoftWideRef::<<Pointee as crate::ReprC>::CType>::from_raw_parts(first, b)
        };
        let different_pointer = unsafe {
            abi::SoftWideRef::<<Pointee as crate::ReprC>::CType>::from_raw_parts(second, a)
        };
        assert!(one < same_pointer);
        assert_ne!(one, different_pointer);
        assert_eq!(
            one.partial_cmp(&different_pointer),
            Some(first.cmp(&second))
        );

        let one = unsafe {
            abi::SoftWideRefMut::<<Pointee as crate::ReprC>::CType>::from_raw_parts_mut(first, a)
        };
        let same_pointer = unsafe {
            abi::SoftWideRefMut::<<Pointee as crate::ReprC>::CType>::from_raw_parts_mut(first, b)
        };
        let different_pointer = unsafe {
            abi::SoftWideRefMut::<<Pointee as crate::ReprC>::CType>::from_raw_parts_mut(second, a)
        };
        assert!(one < same_pointer);
        assert_ne!(one, different_pointer);
        assert_eq!(
            one.partial_cmp(&different_pointer),
            Some(first.cmp(&second))
        );

        let null = abi::SoftWideRef::<<Pointee as crate::ReprC>::CType>::null();
        let other_null = unsafe {
            abi::SoftWideRef::<<Pointee as crate::ReprC>::CType>::from_raw_parts(
                core::ptr::null(),
                a,
            )
        };
        assert_ne!(null, other_null);
    }
    #[test]
    fn lifetime_wide_carriers_preserve_layout_and_metadata() {
        use core::mem::{align_of, size_of};

        assert_eq!(
            size_of::<abi::WideRef<'_, [u8]>>(),
            size_of::<abi::SoftWideRef<[u8]>>()
        );
        assert_eq!(
            align_of::<abi::WideRef<'_, [u8]>>(),
            align_of::<abi::SoftWideRef<[u8]>>()
        );
        assert_eq!(
            size_of::<abi::WideRefMut<'_, [u8]>>(),
            size_of::<abi::SoftWideRefMut<[u8]>>()
        );
        let values = [1_u8, 2, 3];
        let shared = abi::WideRef::<[u8]>::from_ref(&values);
        assert_eq!(shared.metadata(), 3);
        assert_eq!(unsafe { shared.into_ref() }, Some(&values[..]));
        let mut values = [4_u8, 5];
        let exclusive: abi::WideRefMut<'_, [u8]> = crate::encode(&mut values[..]);
        unsafe { crate::decode::<&mut [u8]>(exclusive) }.unwrap()[1] = 6;
        assert_eq!(values, [4, 6]);
        assert!(unsafe { abi::WideRef::<[u8]>::null().into_ref() }.is_none());
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
