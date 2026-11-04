//! ABI carriers for pointers derived from references.
use core::cell::UnsafeCell;

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::Borrow,
    stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked},
    transmute::{AbiCompatible, AbiCompatibleValidate},
};

/// A thin shared reference pointer carrier with the same ABI as `*const R::CType`.
///
/// A null pointer represents an absent reference.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CSoftRef<R: ReprC + ?Sized> {
    data: *const R::CType,
}

/// A thin mutable pointer carrier with a defined C ABI layout.
///
/// - `EXCLUSIVE = true` represents exclusive access that comes from `&mut T`.
/// - `EXCLUSIVE = false` represents access that comes from `&UnsafeCell<T>`.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CSoftRefMut<R: ReprC + ?Sized, const EXCLUSIVE: bool> {
    data: *mut R::CType,
}

unsafe impl<R: ReprC<CType: Sized> + ?Sized> crate::Zeroable for CSoftRef<R> {}
unsafe impl<R: ReprC<CType: Sized> + ?Sized, const EXCLUSIVE: bool> crate::Zeroable
    for CSoftRefMut<R, EXCLUSIVE>
{
}

impl<R: ReprC<CType: CType + Sized> + ?Sized> CSoftRef<R> {
    /// Construct a carrier with a null data pointer.
    pub const fn null() -> Self {
        Self {
            // TODO: Make it work for ?Sized types also
            data: core::ptr::null(),
        }
    }
}

impl<R: ReprC<CType: CType + Sized> + ?Sized, const EXCLUSIVE: bool> CSoftRefMut<R, EXCLUSIVE> {
    /// Construct a carrier with a null data pointer.
    pub const fn null() -> Self {
        Self {
            // TODO: Make it work for ?Sized types also
            data: core::ptr::null_mut(),
        }
    }
}

impl<R: ReprC<CType: CType> + ?Sized> CSoftRef<R> {
    /// Wrap a pointer that follows the validity and aliasing rules of a shared reference.
    ///
    /// # Safety
    ///
    /// A non-null thin pointer must refer to a live `R::CType` for any later dereference.
    pub const unsafe fn from_raw(data: *const R::CType) -> Self {
        assert_thin_pointer::<R::CType>();
        Self { data }
    }

    /// Wrap an existing shared reference to lowered data.
    pub const fn from_ref(data: &R::CType) -> Self {
        assert_thin_pointer::<R::CType>();
        Self { data }
    }
}

impl<R: ReprC<CType: CType> + ?Sized, const EXCLUSIVE: bool> CSoftRefMut<R, EXCLUSIVE> {
    /// Wrap a pointer in a mutable ABI carrier.
    ///
    /// # Safety
    ///
    /// The thin pointer must satisfy the validity and aliasing rules of the source
    /// reference. If `EXCLUSIVE` is `true`, accesses must be exclusive.
    pub const unsafe fn from_raw(data: *mut R::CType) -> Self {
        assert_thin_pointer::<R::CType>();
        Self { data }
    }
}

impl<R: ReprC<CType: CType> + ?Sized, const EXCLUSIVE: bool> CSoftRefMut<R, EXCLUSIVE> {
    /// Wrap an existing mutable reference to lowered data.
    pub const fn from_mut(data: &mut R::CType) -> Self {
        assert_thin_pointer::<R::CType>();
        Self { data }
    }
}

impl<R: ReprC<CType: CType> + ?Sized> CSoftRefMut<R, false> {
    /// Create a carrier for shared access with interior mutability.
    pub const fn from_ref(cell: &UnsafeCell<R::CType>) -> Self {
        assert_thin_pointer::<R::CType>();
        Self { data: cell.get() }
    }
}

impl<R: ReprC + ?Sized> CSoftRef<R> {
    /// Cast the pointee type without accessing or validating the pointee.
    ///
    /// # Safety
    ///
    /// The resulting carrier must satisfy the destination's raw constructor contract
    pub(crate) const unsafe fn cast<U: ReprC<CType: Sized> + ?Sized>(self) -> CSoftRef<U> {
        CSoftRef {
            data: self.data.cast(),
        }
    }

    pub const fn as_ptr(self) -> *const R::CType {
        self.data
    }

    /// Recover the shared reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// A non-null pointer must refer to a live `C` for the returned lifetime.
    pub const unsafe fn into_ref<'a>(self) -> Option<&'a R::CType> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { &*self.data })
    }
}

impl<R: ReprC + ?Sized, const EXCLUSIVE: bool> CSoftRefMut<R, EXCLUSIVE> {
    /// Cast the pointee type without accessing or validating the pointee.
    ///
    /// # Safety
    ///
    /// The resulting carrier must satisfy the destination's raw constructor contract
    pub(crate) const unsafe fn cast<U: ReprC<CType: Sized> + ?Sized>(
        self,
    ) -> CSoftRefMut<U, EXCLUSIVE> {
        CSoftRefMut {
            data: self.data.cast(),
        }
    }

    pub const fn as_ptr(self) -> *mut R::CType {
        self.data
    }
}

impl<R: ReprC + ?Sized> CSoftRefMut<R, true> {
    /// Recover the mutable reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer must refer to a live `C` that is exclusively accessible for the returned lifetime.
    pub const unsafe fn into_mut<'a>(self) -> Option<&'a mut R::CType> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { &mut *self.data })
    }
}

impl<R: ReprC + ?Sized, const EXCLUSIVE: bool> CSoftRefMut<R, EXCLUSIVE> {
    /// Recover the interior-mutable reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer must refer to a live `C` that is accessible for the returned lifetime.
    pub const unsafe fn into_ref<'a>(self) -> Option<&'a UnsafeCell<R::CType>> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { &*(self.data as *const UnsafeCell<R::CType>) })
    }
}

macro_rules! impl_thin_pointer_traits {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
        impl<$($params)*> Copy for $ty<$($args)*> {}
        impl<$($params)*> Clone for $ty<$($args)*> {
            fn clone(&self) -> Self { *self }
        }

        impl<$($params)*> PartialEq for $ty<$($args)*> {
            fn eq(&self, other: &Self) -> bool {
                !self.data.is_null() && core::ptr::eq(self.data, other.data)
            }
        }
        impl<$($params)*> PartialOrd for $ty<$($args)*> {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                if self.data.is_null() || other.data.is_null() {
                    None
                } else {
                    #[allow(ambiguous_wide_pointer_comparisons)]
                    self.data.partial_cmp(&other.data)
                }
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

impl_reference_carrier! { CSoftRef, [R: ReprC + ?Sized], [R] }
impl_reference_carrier! { CSoftRefMut, [R: ReprC + ?Sized, const EXCLUSIVE: bool], [R, EXCLUSIVE] }
impl_reference_identity! { CSoftRef, [R: ReprC<CType: CType> + ?Sized], [R] }
impl_reference_identity! { CSoftRefMut, [R: ReprC<CType: CType> + ?Sized, const EXCLUSIVE: bool], [R, EXCLUSIVE] }

const fn assert_thin_pointer<C: ?Sized>() {
    const {
        assert!(
            core::mem::size_of::<*const C>() == core::mem::size_of::<*const ()>(),
            "reference carrier requires a thin pointer"
        );
    }
}

#[cfg(feature = "alloc")]
pub(crate) use impl_thin_pointer_traits;

#[cfg(test)]
mod tests {
    use core::cell::UnsafeCell;

    use static_assertions::assert_impl_all;

    use super::*;
    use crate::{niche::Niche, wide::CSoftWideRefMut};

    #[test]
    fn shared_reference_carrier_preserves_pointer_abi() {
        assert_impl_all!(&u8: Niche<CType = CSoftRef<u8>>, Encode, Decode<'static>);
        assert_impl_all!(&CSoftRef<u8>: Niche<CType = CSoftRef<CSoftRef<u8>>>, Encode, Decode<'static>);
        assert_eq!(
            core::mem::size_of::<CSoftRef<u8>>(),
            core::mem::size_of::<*const u8>()
        );
        assert_eq!(
            core::mem::align_of::<CSoftRef<u8>>(),
            core::mem::align_of::<*const u8>()
        );

        let value = 7_u8;
        let carrier = crate::encode(&value);
        assert_eq!(carrier, CSoftRef::from_ref(&value));
        assert_eq!(unsafe { crate::decode::<&u8>(carrier) }, Some(&value));
    }

    #[test]
    fn exclusive_and_shared_mutable_references_have_distinct_carriers() {
        assert_impl_all!(&mut u8: Niche<CType = CSoftRefMut<u8, true>>);
        assert_impl_all!(&UnsafeCell<u8>: Niche<CType = CSoftRefMut<UnsafeCell<u8>, false>>);
        assert_impl_all!(&mut UnsafeCell<u8>: Niche<CType = CSoftRefMut<UnsafeCell<u8>, true>>);
        assert_impl_all!(&mut [u8]: Niche<CType = CSoftWideRefMut<[u8], true>>);
        assert_impl_all!(&[UnsafeCell<u8>]: Niche<CType = CSoftWideRefMut<[UnsafeCell<u8>], false>>);
        assert_impl_all!(&mut [UnsafeCell<u8>]: Niche<CType = CSoftWideRefMut<[UnsafeCell<u8>], true>>);

        assert_eq!(
            core::mem::size_of::<CSoftRefMut<u8, true>>(),
            core::mem::size_of::<*mut u8>()
        );
        assert_eq!(
            core::mem::align_of::<CSoftRefMut<u8, true>>(),
            core::mem::align_of::<*mut u8>()
        );

        let mut cell = UnsafeCell::new(1_u8);
        let exclusive: CSoftRefMut<UnsafeCell<u8>, true> = crate::encode(&mut cell);
        assert_eq!(exclusive.as_ptr(), cell.get().cast::<UnsafeCell<u8>>());
        unsafe { exclusive.as_ptr().write(UnsafeCell::new(2)) };
        assert_eq!(cell.into_inner(), 2);
    }
}
