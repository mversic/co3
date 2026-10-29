//! C-ABI borrowed wide pointer carriers.
use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
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

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata = usize> + ?Sized> FromRawWideRef<P>
    for CWideRef<P>
{
    unsafe fn from_raw_reference(data: *const PointeeData<P>, metadata: usize) -> Self {
        unsafe { Self::from_raw_parts(data, metadata) }
    }
}

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata = usize> + ?Sized> FromRawWideRef<P>
    for CWideRefMut<P, false>
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
pub struct CWideRef<P: Wide + ?Sized> {
    data: *const PointeeData<P>,
    metadata: P::Metadata,
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
pub struct CWideRefMut<P: Wide + ?Sized, const EXCLUSIVE: bool> {
    data: *mut PointeeData<P>,
    metadata: P::Metadata,
}

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized> CWideRef<P> {
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
    pub const unsafe fn from_raw_parts(data: *const PointeeData<P>, metadata: P::Metadata) -> Self {
        Self { data, metadata }
    }

    /// Wrap an existing wide reference.
    pub fn from_ref(source: &P) -> Self {
        Self {
            data: P::as_ptr(source).cast(),
            metadata: P::metadata(source),
        }
    }
}

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized, const EXCLUSIVE: bool>
    CWideRefMut<P, EXCLUSIVE>
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
        data: *mut PointeeData<P>,
        metadata: P::Metadata,
    ) -> Self {
        Self { data, metadata }
    }

    /// Wrap an existing mutable wide reference.
    pub fn from_mut(source: &mut P) -> Self {
        Self {
            data: P::as_mut_ptr(source).cast(),
            metadata: P::metadata(source),
        }
    }
}

impl<P: Wide + ?Sized> CWideRef<P> {
    pub const fn as_ptr(&self) -> *const PointeeData<P> {
        self.data
    }
    pub(crate) const fn data(&self) -> *const PointeeData<P> {
        self.data
    }

    /// Extracts the metadata component of a pointer.
    pub const fn metadata(&self) -> P::Metadata {
        self.metadata
    }
}

impl<P: Wide + ?Sized, const EXCLUSIVE: bool> CWideRefMut<P, EXCLUSIVE> {
    pub const fn as_ptr(&self) -> *mut PointeeData<P> {
        self.data
    }
    pub(crate) const fn data(&self) -> *mut PointeeData<P> {
        self.data
    }
    pub(crate) const fn is_null(&self) -> bool {
        self.data.is_null()
    }

    /// Extracts the metadata component of a pointer.
    pub const fn metadata(&self) -> P::Metadata {
        self.metadata
    }
}

impl<P: Wide + ?Sized> CWideRef<P> {
    /// Recover a shared wide reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer and metadata must describe a valid `P` for the returned lifetime.
    pub unsafe fn into_ref<'a>(self) -> Option<&'a P> {
        if self.data.is_null() {
            return None;
        }

        let metadata = self.metadata;
        let data = self.data.cast();

        Some(unsafe { P::from_raw_parts(data, metadata) })
    }
}

impl<P: Wide + ?Sized> CWideRefMut<P, true> {
    /// Recover a mutable wide reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer and metadata must describe a valid, exclusively accessible `P`
    /// for the returned lifetime.
    pub unsafe fn into_mut<'a>(self) -> Option<&'a mut P> {
        if self.data.is_null() {
            return None;
        }

        let metadata = self.metadata;
        let data = self.data.cast();

        Some(unsafe { P::from_raw_parts_mut(data, metadata) })
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

        impl<$($params)*> Eq for $ty<$($args)*> {}
        impl<$($params)*> PartialEq for $ty<$($args)*> {
            fn eq(&self, other: &Self) -> bool {
                self.data == other.data && self.metadata == other.metadata
            }
        }

        impl<$($params)*> Ord for $ty<$($args)*> {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                self.data.cmp(&other.data).then_with(|| self.metadata.cmp(&other.metadata))
            }
        }
        impl<$($params)*> PartialOrd for $ty<$($args)*> {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
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

impl_wide_raw_methods! { CWideRef, [P: Wide + ?Sized], [P] }
impl_wide_raw_methods! { CWideRefMut, [P: Wide + ?Sized, const EXCLUSIVE: bool], [P, EXCLUSIVE] }

impl_carrier_borrow! { CWideRef, [P: Wide + ?Sized], [P] }
impl_carrier_borrow! { CWideRefMut, [P: Wide + ?Sized, const EXCLUSIVE: bool], [P, EXCLUSIVE] }

impl_identity_carrier! { CWideRef, [P: Wide<Header: WideHeader<Data: CType>> + ?Sized], [P] }
impl_identity_carrier! { CWideRefMut, [P: Wide<Header: WideHeader<Data: CType>> + ?Sized, const EXCLUSIVE: bool], [P, EXCLUSIVE] }

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized> Default for CWideRef<P> {
    fn default() -> Self {
        Self::null()
    }
}
impl<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized, const EXCLUSIVE: bool>
    Default for CWideRefMut<P, EXCLUSIVE>
{
    fn default() -> Self {
        Self::null()
    }
}

pub(crate) use impl_carrier_borrow;
pub(crate) use impl_identity_carrier;
#[cfg(feature = "alloc")]
pub(crate) use impl_wide_raw_methods;
