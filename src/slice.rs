//! C-ABI borrowed wide pointer carriers.
use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC, Zeroable,
    stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked},
    transmute::AbiCompatible,
    wide::{PointeeData, Wide, WideHeader},
};

/// Constructs a shared-reference carrier from its data address and metadata without reading it.
pub(crate) trait FromRawWideRef<P: Wide<Metadata = usize> + ?Sized>: CType + Sized {
    /// # Safety
    ///
    /// The pointer and length must satisfy the validity and aliasing requirements
    /// of the selected carrier. Interior-mutability access must remain synchronized.
    unsafe fn from_raw_reference(data: *const PointeeData<P>, metadata: usize) -> Self;
}

impl<R, P> FromRawWideRef<P> for CWideRef<R>
where
    R: ReprC<CType = P> + ?Sized,
    P: Wide<Header: WideHeader<Data: CType>, Metadata = usize> + ?Sized,
{
    unsafe fn from_raw_reference(data: *const PointeeData<P>, metadata: usize) -> Self {
        unsafe { Self::from_raw_parts(data, metadata) }
    }
}

impl<R, P> FromRawWideRef<P> for CWideRefMut<R, false>
where
    R: ReprC<CType = P> + ?Sized,
    P: Wide<Header: WideHeader<Data: CType>, Metadata = usize> + ?Sized,
{
    unsafe fn from_raw_reference(data: *const PointeeData<P>, metadata: usize) -> Self {
        unsafe { Self::from_raw_parts_mut(data.cast_mut(), metadata) }
    }
}

/// Shared reference to a C-compatible wide pointee.
///
/// A null data pointer represents an absent reference.
#[derive(RustSpec)]
#[repr(C)]
pub struct CWideRef<R: ReprC<CType: Wide> + ?Sized> {
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
pub struct CWideRefMut<R: ReprC<CType: Wide> + ?Sized, const EXCLUSIVE: bool> {
    data: *mut PointeeData<R::CType>,
    metadata: <R::CType as Wide>::Metadata,
}

unsafe impl<R: ReprC<CType: Wide> + ?Sized> Zeroable for CWideRef<R> {}
unsafe impl<R: ReprC<CType: Wide> + ?Sized, const EXCLUSIVE: bool> Zeroable
    for CWideRefMut<R, EXCLUSIVE>
{
}

impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> CWideRef<R> {
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
    CWideRefMut<R, EXCLUSIVE>
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

impl<R: ReprC<CType: Wide> + ?Sized> CWideRef<R> {
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

impl<R: ReprC<CType: Wide> + ?Sized, const EXCLUSIVE: bool> CWideRefMut<R, EXCLUSIVE> {
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

impl<R: ReprC<CType: Wide> + ?Sized> CWideRef<R> {
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

impl<R: ReprC<CType: Wide> + ?Sized> CWideRefMut<R, true> {
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

impl_wide_raw_methods! { CWideRef, [R: ReprC<CType: Wide> + ?Sized], [R] }
impl_wide_raw_methods! { CWideRefMut, [R: ReprC<CType: Wide> + ?Sized, const EXCLUSIVE: bool], [R, EXCLUSIVE] }

impl_carrier_borrow! { CWideRef, [R: ReprC<CType: Wide> + ?Sized], [R] }
impl_carrier_borrow! { CWideRefMut, [R: ReprC<CType: Wide> + ?Sized, const EXCLUSIVE: bool], [R, EXCLUSIVE] }

impl_identity_carrier! { CWideRef, [R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized], [R] }
impl_identity_carrier! { CWideRefMut, [R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const EXCLUSIVE: bool], [R, EXCLUSIVE] }

impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> Default for CWideRef<R> {
    fn default() -> Self {
        Self::null()
    }
}
impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const EXCLUSIVE: bool> Default
    for CWideRefMut<R, EXCLUSIVE>
{
    fn default() -> Self {
        Self::null()
    }
}

#[cfg(feature = "alloc")]
pub(crate) use impl_carrier_borrow;
pub(crate) use impl_identity_carrier;
#[cfg(feature = "alloc")]
pub(crate) use impl_wide_raw_methods;
