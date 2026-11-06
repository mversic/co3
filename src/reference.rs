//! ABI carriers for pointers derived from references.
use core::cell::UnsafeCell;

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::Borrow,
    stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked},
    transmute::{AbiCompatible, AbiCompatibleValidate},
};

/// Shared carrier operations used by internal conversion helpers.
///
/// # Safety
/// Implementations must preserve the raw pointer ABI and shared access contract.
pub(crate) unsafe trait SharedRefCarrier<R: ?Sized>: CType + Copy {
    fn as_ptr(&self) -> *const R::CType where R: ReprC;
    /// # Safety
    /// The pointer must satisfy the carrier's validity and lifetime contract.
    unsafe fn from_shared_ptr(data: *const R::CType) -> Self where R: ReprC;
    /// # Safety
    /// The result must satisfy the destination carrier's raw constructor contract.
    unsafe fn cast_shared<U: ReprC<CType: Sized> + ?Sized, Target: SharedRefCarrier<U>>(self) -> Target where R: ReprC {
        unsafe { Target::from_shared_ptr(self.as_ptr().cast()) }
    }
}

unsafe impl<R: ReprC + ?Sized> SharedRefCarrier<R> for CSoftRef<R> {
    fn as_ptr(&self) -> *const R::CType { self.data }
    unsafe fn from_shared_ptr(data: *const R::CType) -> Self { unsafe { Self::from_raw(data) } }
}
unsafe impl<'a, R: ReprC + ?Sized> SharedRefCarrier<R> for CRef<'a, R> {
    fn as_ptr(&self) -> *const R::CType { self.data }
    unsafe fn from_shared_ptr(data: *const R::CType) -> Self { unsafe { Self::from_raw(data) } }
}

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
    /// Recover the shared reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// A non-null pointer must refer to a live `C` for the returned lifetime.
    pub const unsafe fn into_ref<'a>(self) -> Option<&'a R::CType> where R: ReprC {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { &*self.data })
    }
}

impl<R: ReprC + ?Sized, const EXCLUSIVE: bool> CSoftRefMut<R, EXCLUSIVE> {}

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

/// A thin shared carrier whose source borrow lasts for `'a`.
///
/// A null pointer represents an absent reference.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CRef<'a, R: ReprC + ?Sized> {
    data: *const R::CType,
    lifetime: core::marker::PhantomData<&'a ()>,
}

/// A thin mutable carrier whose source borrow lasts for `'a`.
///
/// - `EXCLUSIVE = true` represents access from `&mut T`.
/// - `EXCLUSIVE = false` represents access from `&UnsafeCell<T>`.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CRefMut<'a, R: ReprC + ?Sized, const EXCLUSIVE: bool> {
    data: *mut R::CType,
    lifetime: core::marker::PhantomData<&'a ()>,
}

unsafe impl<'a, R: ReprC<CType: Sized> + ?Sized> crate::Zeroable for CRef<'a, R> {}
unsafe impl<'a, R: ReprC<CType: Sized> + ?Sized, const EXCLUSIVE: bool> crate::Zeroable
    for CRefMut<'a, R, EXCLUSIVE>
{
}

impl<'a, R: ReprC<CType: CType + Sized> + ?Sized> CRef<'a, R> {
    /// Construct a carrier with a null data pointer.
    pub const fn null() -> Self {
        Self {
            // TODO: Make it work for ?Sized types also
            data: core::ptr::null(),
            lifetime: core::marker::PhantomData,
        }
    }
}

impl<'a, R: ReprC<CType: CType + Sized> + ?Sized, const EXCLUSIVE: bool> CRefMut<'a, R, EXCLUSIVE> {
    /// Construct a carrier with a null data pointer.
    pub const fn null() -> Self {
        Self {
            // TODO: Make it work for ?Sized types also
            data: core::ptr::null_mut(),
            lifetime: core::marker::PhantomData,
        }
    }
}

impl<'a, R: ReprC<CType: CType> + ?Sized> CRef<'a, R> {
    /// Wrap a pointer that follows the validity and aliasing rules of a shared reference.
    ///
    /// # Safety
    ///
    /// A non-null thin pointer must refer to a live `R::CType` for 'a and satisfy the aliasing requirements.
    pub const unsafe fn from_raw(data: *const R::CType) -> Self {
        assert_thin_pointer::<R::CType>();
        Self {
            data,
            lifetime: core::marker::PhantomData,
        }
    }

    /// Wrap an existing shared reference to lowered data.
    pub const fn from_ref(data: &'a R::CType) -> Self {
        assert_thin_pointer::<R::CType>();
        Self {
            data,
            lifetime: core::marker::PhantomData,
        }
    }
}

impl<'a, R: ReprC<CType: CType> + ?Sized, const EXCLUSIVE: bool> CRefMut<'a, R, EXCLUSIVE> {
    /// Wrap a pointer in a mutable ABI carrier.
    ///
    /// # Safety
    ///
    /// The thin pointer must satisfy the validity and aliasing rules of the source
    /// reference for 'a. If `EXCLUSIVE` is `true`, accesses must be exclusive.
    pub const unsafe fn from_raw(data: *mut R::CType) -> Self {
        assert_thin_pointer::<R::CType>();
        Self {
            data,
            lifetime: core::marker::PhantomData,
        }
    }
}

impl<'a, R: ReprC<CType: CType> + ?Sized, const EXCLUSIVE: bool> CRefMut<'a, R, EXCLUSIVE> {
    /// Wrap an existing mutable reference to lowered data.
    pub const fn from_mut(data: &'a mut R::CType) -> Self {
        assert_thin_pointer::<R::CType>();
        Self {
            data,
            lifetime: core::marker::PhantomData,
        }
    }
}

impl<'a, R: ReprC<CType: CType> + ?Sized> CRefMut<'a, R, false> {
    /// Create a carrier for shared access with interior mutability.
    pub const fn from_ref(cell: &'a UnsafeCell<R::CType>) -> Self {
        assert_thin_pointer::<R::CType>();
        Self {
            data: cell.get(),
            lifetime: core::marker::PhantomData,
        }
    }
}

impl<'a, R: ReprC + ?Sized> CRef<'a, R> {
    /// Recover the shared reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// A non-null pointer must refer to a live `C` for the returned lifetime.
    pub const unsafe fn into_ref(self) -> Option<&'a R::CType> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { &*self.data })
    }
}

impl<'a, R: ReprC + ?Sized, const EXCLUSIVE: bool> CRefMut<'a, R, EXCLUSIVE> {}

impl<'a, R: ReprC + ?Sized> CRefMut<'a, R, true> {
    /// Recover the mutable reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer must refer to a live `C` that is exclusively accessible for the returned lifetime.
    pub const unsafe fn into_mut(self) -> Option<&'a mut R::CType> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { &mut *self.data })
    }
}

impl<'a, R: ReprC + ?Sized, const EXCLUSIVE: bool> CRefMut<'a, R, EXCLUSIVE> {
    /// Recover the interior-mutable reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer must refer to a live `C` that is accessible for the returned lifetime.
    pub const unsafe fn into_ref(self) -> Option<&'a UnsafeCell<R::CType>> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { &*(self.data as *const UnsafeCell<R::CType>) })
    }
}

// The carrier form determines lifetime tracking; pointer operations are identical.
macro_rules! impl_ref_pointer_methods {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*], [$($target:tt)*], [$($suffix:tt)*], $ptr:ident, [$($marker:tt)*]) => {
        impl<$($params)*> $ty<$($args)*> {
            /// Cast the pointee without accessing it.
            ///
            /// # Safety
            /// The result must satisfy the destination's raw constructor contract.
            #[allow(dead_code)]
            pub(crate) const unsafe fn cast<U: ReprC<CType: Sized> + ?Sized>(self) -> $ty<$($target)* U $($suffix)*> {
                $ty { data: self.data.cast(), $($marker)* }
            }

            /// Return the stored pointer.
            pub const fn as_ptr(self) -> *$ptr R::CType { self.data }
        }
    };
}
impl_ref_pointer_methods! { CSoftRef, [R: ReprC + ?Sized], [R], [], [], const, [] }
impl_ref_pointer_methods! { CSoftRefMut, [R: ReprC + ?Sized, const EXCLUSIVE: bool], [R, EXCLUSIVE], [], [, EXCLUSIVE], mut, [] }
impl_ref_pointer_methods! { CRef, ['a, R: ReprC + ?Sized], ['a, R], ['a,], [], const, [lifetime: core::marker::PhantomData,] }
impl_ref_pointer_methods! { CRefMut, ['a, R: ReprC + ?Sized, const EXCLUSIVE: bool], ['a, R, EXCLUSIVE], ['a,], [, EXCLUSIVE], mut, [lifetime: core::marker::PhantomData,] }

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

impl_reference_carrier! { CRef, ['a, R: ReprC + ?Sized], ['a, R] }
impl_reference_carrier! { CRefMut, ['a, R: ReprC + ?Sized, const EXCLUSIVE: bool], ['a, R, EXCLUSIVE] }
impl_reference_identity! { CRef, ['a, R: ReprC<CType: CType> + ?Sized], ['a, R] }
impl_reference_identity! { CRefMut, ['a, R: ReprC<CType: CType> + ?Sized, const EXCLUSIVE: bool], ['a, R, EXCLUSIVE] }

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
    fn lifetime_carriers_preserve_layout_and_borrows() {
        use core::mem::{align_of, size_of};
        assert_eq!(size_of::<CRef<'_, u8>>(), size_of::<CSoftRef<u8>>());
        assert_eq!(align_of::<CRef<'_, u8>>(), align_of::<CSoftRef<u8>>());
        assert_eq!(
            size_of::<CRefMut<'_, u8, true>>(),
            size_of::<CSoftRefMut<u8, true>>()
        );
        let value = 7_u8;
        let shared = CRef::<u8>::from_ref(&value);
        assert_eq!(unsafe { shared.into_ref() }, Some(&7));
        let mut value = 8_u8;
        let exclusive = CRefMut::<u8, true>::from_mut(&mut value);
        *unsafe { exclusive.into_mut().unwrap() } = 9;
        assert_eq!(value, 9);
        let cell = UnsafeCell::new(2_u8);
        let shared_cell = CRefMut::<u8, false>::from_ref(&cell);
        assert_eq!(unsafe { shared_cell.into_ref().unwrap() }.get(), cell.get());
        assert!(unsafe { CRef::<u8>::null().into_ref() }.is_none());
    }

    #[test]
    fn shared_reference_carrier_preserves_pointer_abi() {
        assert_impl_all!(&u8: Niche<CType = CRef<'static, u8>>, Encode, Decode<'static>);
        assert_impl_all!(&CSoftRef<u8>: Niche<CType = CRef<'static, CSoftRef<u8>>>, Encode, Decode<'static>);
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
        assert_eq!(carrier, CRef::from_ref(&value));
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
