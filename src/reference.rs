//! ABI carriers for pointers derived from references.
use core::cell::UnsafeCell;

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::Borrow,
    stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked},
    transmute::{AbiCompatible, AbiCompatibleValidate},
};

/// Constructs the carrier selected by `ReprC` for a shared reference without reading it.
pub(crate) trait FromRawRef<C: ?Sized>: CType + Sized {
    /// # Safety
    ///
    /// The thin pointer must satisfy the validity and aliasing requirements of the
    /// selected carrier. Interior-mutability access must remain synchronized.
    unsafe fn from_raw_reference(data: *const C) -> Self;
}

impl<C: CType + ?Sized> FromRawRef<C> for CRef<C> {
    unsafe fn from_raw_reference(data: *const C) -> Self {
        unsafe { Self::from_raw(data) }
    }
}

impl<C: CType + ?Sized> FromRawRef<C> for CRefMut<C, false> {
    unsafe fn from_raw_reference(data: *const C) -> Self {
        unsafe { Self::from_raw(data.cast_mut()) }
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

/// A thin mutable pointer carrier with a defined C ABI layout.
///
/// - `EXCLUSIVE = true` represents exclusive access that comes from `&mut T`.
/// - `EXCLUSIVE = false` represents access that comes from `&UnsafeCell<T>`.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CRefMut<C: ?Sized, const EXCLUSIVE: bool> {
    data: *mut C,
}

unsafe impl<C: Sized> crate::Zeroable for CRef<C> {}
unsafe impl<C: Sized, const EXCLUSIVE: bool> crate::Zeroable for CRefMut<C, EXCLUSIVE> {}

impl<C: CType> CRef<C> {
    /// Construct a carrier with a null data pointer.
    pub const fn null() -> Self {
        Self {
            // TODO: Make it work for ?Sized types also
            data: core::ptr::null(),
        }
    }
}

impl<C: CType, const EXCLUSIVE: bool> CRefMut<C, EXCLUSIVE> {
    /// Construct a carrier with a null data pointer.
    pub const fn null() -> Self {
        Self {
            // TODO: Make it work for ?Sized types also
            data: core::ptr::null_mut(),
        }
    }
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

impl<C: CType + ?Sized, const EXCLUSIVE: bool> CRefMut<C, EXCLUSIVE> {
    /// Wrap an existing mutable reference to lowered data.
    pub const fn from_mut(data: &mut C) -> Self {
        assert_thin_pointer::<C>();
        Self { data }
    }
}

impl<C: CType + ?Sized> CRefMut<C, false> {
    /// Create a carrier for shared access with interior mutability.
    pub const fn from_ref(cell: &UnsafeCell<C>) -> Self {
        assert_thin_pointer::<C>();
        Self { data: cell.get() }
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
    pub const unsafe fn into_ref<'a>(self) -> Option<&'a C> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { &*self.data })
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
    /// The pointer must refer to a live `C` that is exclusively accessible for the returned lifetime.
    pub const unsafe fn into_mut<'a>(self) -> Option<&'a mut C> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { &mut *self.data })
    }
}

impl<C: ?Sized, const EXCLUSIVE: bool> CRefMut<C, EXCLUSIVE> {
    /// Recover the interior-mutable reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer must refer to a live `C` that is accessible for the returned lifetime.
    pub const unsafe fn into_ref<'a>(self) -> Option<&'a UnsafeCell<C>> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { &*(self.data as *const UnsafeCell<C>) })
    }
}

macro_rules! impl_thin_pointer_traits {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
        impl<$($params)*> Copy for $ty<$($args)*> {}
        impl<$($params)*> Clone for $ty<$($args)*> {
            fn clone(&self) -> Self { *self }
        }

        impl<$($params)*> Eq for $ty<$($args)*> {}
        impl<$($params)*> PartialEq for $ty<$($args)*> {
            fn eq(&self, other: &Self) -> bool { core::ptr::eq(self.data, other.data) }
        }
        impl<$($params)*> Ord for $ty<$($args)*> {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                #[allow(ambiguous_wide_pointer_comparisons)]
                self.data.cmp(&other.data)
            }
        }
        impl<$($params)*> PartialOrd for $ty<$($args)*> {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
    };
}

macro_rules! impl_reference_carrier {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
        impl_thin_pointer_traits! { $ty, [$($params)*], [$($args)*] }

        impl<$($params)*> core::fmt::Debug for $ty<$($args)*> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_tuple(stringify!($ty)).field(&self.data).finish()
            }
        }

        unsafe impl<$($params)*> Borrow for $ty<$($args)*> {
            type Borrowed = Self;

            fn borrow(self) -> Self::Borrowed { self }
            fn from_borrow(source: Self::Borrowed) -> Self { source }
        }

    };
}

macro_rules! impl_reference_identity {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
        impl<$($params)*> ReprC for $ty<$($args)*> { type CType = Self; }
        unsafe impl<$($params)*> EncodeMove for $ty<$($args)*> {
            type Store = ();
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
                unsafe { self.soft_encode_unchecked(&mut ()) }
            }
        }
        unsafe impl<$($params)*> EncodeMoveUnchecked for $ty<$($args)*> {
            type SoftStore = ();
            unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm { self }
        }
        unsafe impl<'d, $($params)*> DecodeMove<'d> for $ty<$($args)*> {
            type Store = ();
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> { Some(source) }
        }
        impl<$($params)*> Encode for $ty<$($args)*> {}
        impl<$($params)*> crate::EncodeUnchecked for $ty<$($args)*> {}
        impl<'d, $($params)*> Decode<'d> for $ty<$($args)*> {}
        unsafe impl<$($params)*> CType for $ty<$($args)*> {}
        unsafe impl<$($params)*> CFnArg for $ty<$($args)*> {}
        unsafe impl<$($params)*> CFnReturn for $ty<$($args)*> {}
        unsafe impl<$($params)*> AbiCompatibleValidate for $ty<$($args)*> {
            unsafe fn is_valid(_: &Self::CType) -> bool { true }
        }
unsafe impl<$($params)*> AbiCompatible for $ty<$($args)*> {}
    };
}

impl_reference_carrier! { CRef, [C: ?Sized], [C] }
impl_reference_carrier! { CRefMut, [C: ?Sized, const EXCLUSIVE: bool], [C, EXCLUSIVE] }
impl_reference_identity! { CRef, [C: CType + ?Sized], [C] }
impl_reference_identity! { CRefMut, [C: CType + ?Sized, const EXCLUSIVE: bool], [C, EXCLUSIVE] }

const fn assert_thin_pointer<C: ?Sized>() {
    const {
        assert!(
            core::mem::size_of::<*const C>() == core::mem::size_of::<*const ()>(),
            "reference carrier requires a thin pointer"
        );
    }
}

pub(crate) use impl_thin_pointer_traits;

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
