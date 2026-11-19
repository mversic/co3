//! ABI carriers and conversion traits for Rust references.
use crate::{CType, ReprC};

/// Shared carrier operations used by internal conversion helpers.
///
/// # Safety
///
/// Implementations must preserve the raw pointer ABI and shared access contract.
pub(crate) unsafe trait RefCarrier<R: ?Sized>: CType + Copy {
    fn as_ptr(&self) -> *const R::CType
    where
        R: ReprC;
    /// # Safety
    /// The pointer must satisfy the carrier's validity and lifetime contract.
    unsafe fn from_shared_ptr(data: *const R::CType) -> Self
    where
        R: ReprC;
    /// # Safety
    /// The result must satisfy the destination carrier's raw constructor contract.
    unsafe fn cast_shared<U: ReprC<CType: Sized> + ?Sized, Target: RefCarrier<U>>(self) -> Target
    where
        R: ReprC,
    {
        unsafe { Target::from_shared_ptr(self.as_ptr().cast()) }
    }
}

/// Operations shared by ordinary and soft thin mutable carriers.
///
/// # Safety
/// Implementations must preserve the pointer ABI and the indicated access contract.
pub(crate) unsafe trait RefMutCarrier<R: ReprC + ?Sized>: CType + Copy {
    fn as_ptr(&self) -> *mut R::CType;
    /// # Safety
    /// The pointer must satisfy the destination carrier's lifetime and aliasing contract.
    unsafe fn from_mut_ptr(data: *mut R::CType) -> Self;
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

#[cfg(feature = "alloc")]
pub(crate) use impl_thin_pointer_traits;

#[doc(hidden)]
pub mod abi {
    use core::cell::UnsafeCell;

    use rust_spec::RustSpec;

    use super::{RefCarrier, RefMutCarrier};
    use crate::{
        CFnArg, CFnReturn, CType, ReprC,
        convert::{
            AbiCompatibleValidate, Decode, DecodeMove, Encode, EncodeMove, EncodeMoveUnchecked,
        },
        marker::AbiCompatible,
    };

    /// A thin shared reference pointer carrier with the same ABI as `*const T`.
    ///
    /// A null pointer represents an absent reference.
    #[derive(RustSpec)]
    #[repr(transparent)]
    #[doc(hidden)]
    pub struct SoftRef<T: CType + ?Sized> {
        data: *const T,
    }

    /// A thin mutable pointer carrier with a defined C ABI layout.
    ///
    /// Represents exclusive access that comes from `&mut T`.
    #[derive(RustSpec)]
    #[repr(transparent)]
    #[doc(hidden)]
    pub struct SoftRefMut<T: CType + ?Sized> {
        data: *mut T,
    }

    unsafe impl<T: CType> crate::marker::Zeroable for SoftRef<T> {}
    unsafe impl<T: CType> crate::marker::Zeroable for SoftRefMut<T> {}

    impl<T: CType> SoftRef<T> {
        /// Construct a carrier with a null data pointer.
        pub const fn null() -> Self {
            Self {
                // TODO: Make it work for ?Sized types also
                data: core::ptr::null(),
            }
        }
    }

    impl<T: CType> SoftRefMut<T> {
        /// Construct a carrier with a null data pointer.
        pub const fn null() -> Self {
            Self {
                // TODO: Make it work for ?Sized types also
                data: core::ptr::null_mut(),
            }
        }
    }

    impl<T: CType + ?Sized> SoftRef<T> {
        /// Wrap a pointer that follows the validity and aliasing rules of a shared reference.
        ///
        /// # Safety
        ///
        /// A non-null thin pointer must refer to a live `T` for any later dereference.
        pub const unsafe fn from_raw(data: *const T) -> Self {
            assert_thin_pointer::<T>();
            Self { data }
        }

        /// Wrap an existing shared reference to lowered data.
        pub const fn from_ref(data: &T) -> Self {
            assert_thin_pointer::<T>();
            Self { data }
        }
    }

    impl<T: CType + ?Sized> SoftRefMut<T> {
        /// Wrap a pointer in a mutable ABI carrier.
        ///
        /// # Safety
        ///
        /// The thin pointer must satisfy the validity and aliasing rules of the source
        /// reference. Accesses must be exclusive.
        pub const unsafe fn from_raw(data: *mut T) -> Self {
            assert_thin_pointer::<T>();
            Self { data }
        }
    }

    impl<T: CType + ?Sized> SoftRefMut<T> {
        /// Wrap an existing mutable reference to lowered data.
        pub const fn from_mut(data: &mut T) -> Self {
            assert_thin_pointer::<T>();
            Self { data }
        }
    }

    impl<T: CType + ?Sized> SoftRef<T> {
        /// Recover the shared reference, returning `None` for a null pointer.
        ///
        /// # Safety
        ///
        /// A non-null pointer must refer to a live `C` for the returned lifetime.
        pub const unsafe fn into_ref<'a>(self) -> Option<&'a T> {
            if self.data.is_null() {
                return None;
            }

            Some(unsafe { &*self.data })
        }
    }

    impl<T: CType + ?Sized> SoftRefMut<T> {
        /// Recover the mutable reference, returning `None` for a null pointer.
        ///
        /// # Safety
        ///
        /// The pointer must refer to a live `C` that is exclusively accessible for the returned lifetime.
        pub const unsafe fn into_mut<'a>(self) -> Option<&'a mut T> {
            if self.data.is_null() {
                return None;
            }

            Some(unsafe { &mut *self.data })
        }
    }

    impl<T: CType + ?Sized> SoftRefMut<T> {
        /// Recover the interior-mutable reference, returning `None` for a null pointer.
        ///
        /// # Safety
        ///
        /// The pointer must refer to a live `C` that is accessible for the returned lifetime.
        pub const unsafe fn into_ref<'a>(self) -> Option<&'a UnsafeCell<T>> {
            if self.data.is_null() {
                return None;
            }

            Some(unsafe { &*(self.data as *const UnsafeCell<T>) })
        }
    }

    /// A thin shared carrier whose source borrow lasts for `'a`.
    ///
    /// A null pointer represents an absent reference.
    #[derive(RustSpec)]
    #[repr(transparent)]
    #[doc(hidden)]
    pub struct Ref<'a, T: CType + ?Sized> {
        data: *const T,
        lifetime: core::marker::PhantomData<&'a ()>,
    }

    /// A thin exclusive carrier whose source borrow lasts for `'a`.
    #[derive(RustSpec)]
    #[repr(transparent)]
    #[doc(hidden)]
    pub struct RefMut<'a, T: CType + ?Sized> {
        data: *mut T,
        lifetime: core::marker::PhantomData<&'a ()>,
    }

    unsafe impl<'a, T: CType> crate::marker::Zeroable for Ref<'a, T> {}
    unsafe impl<'a, T: CType> crate::marker::Zeroable for RefMut<'a, T> {}

    impl<'a, T: CType> Ref<'a, T> {
        /// Construct a carrier with a null data pointer.
        pub const fn null() -> Self {
            Self {
                // TODO: Make it work for ?Sized types also
                data: core::ptr::null(),
                lifetime: core::marker::PhantomData,
            }
        }
    }

    impl<'a, T: CType> RefMut<'a, T> {
        /// Construct a carrier with a null data pointer.
        pub const fn null() -> Self {
            Self {
                // TODO: Make it work for ?Sized types also
                data: core::ptr::null_mut(),
                lifetime: core::marker::PhantomData,
            }
        }
    }

    impl<'a, T: CType + ?Sized> Ref<'a, T> {
        /// Wrap a pointer that follows the validity and aliasing rules of a shared reference.
        ///
        /// # Safety
        ///
        /// A non-null thin pointer must refer to a live `T` for 'a and satisfy the aliasing requirements.
        pub const unsafe fn from_raw(data: *const T) -> Self {
            assert_thin_pointer::<T>();
            Self {
                data,
                lifetime: core::marker::PhantomData,
            }
        }

        /// Wrap an existing shared reference to lowered data.
        pub const fn from_ref(data: &'a T) -> Self {
            assert_thin_pointer::<T>();
            Self {
                data,
                lifetime: core::marker::PhantomData,
            }
        }
    }

    impl<'a, T: CType + ?Sized> RefMut<'a, T> {
        /// Wrap a pointer in a mutable ABI carrier.
        ///
        /// # Safety
        ///
        /// The thin pointer must satisfy the validity and aliasing rules of the source
        /// reference for 'a. Access must be exclusive.
        pub const unsafe fn from_raw(data: *mut T) -> Self {
            assert_thin_pointer::<T>();
            Self {
                data,
                lifetime: core::marker::PhantomData,
            }
        }
    }

    impl<'a, T: CType + ?Sized> RefMut<'a, T> {
        /// Wrap an existing mutable reference to lowered data.
        pub const fn from_mut(data: &'a mut T) -> Self {
            assert_thin_pointer::<T>();
            Self {
                data,
                lifetime: core::marker::PhantomData,
            }
        }
    }

    impl<'a, T: CType + ?Sized> Ref<'a, T> {
        /// Recover the shared reference, returning `None` for a null pointer.
        ///
        /// # Safety
        ///
        /// A non-null pointer must refer to a live `C` for the returned lifetime.
        pub const unsafe fn into_ref(self) -> Option<&'a T> {
            if self.data.is_null() {
                return None;
            }

            Some(unsafe { &*self.data })
        }
    }

    impl<'a, T: CType + ?Sized> RefMut<'a, T> {
        /// Recover the mutable reference, returning `None` for a null pointer.
        ///
        /// # Safety
        ///
        /// The pointer must refer to a live `C` that is exclusively accessible for the returned lifetime.
        pub const unsafe fn into_mut(self) -> Option<&'a mut T> {
            if self.data.is_null() {
                return None;
            }

            Some(unsafe { &mut *self.data })
        }
    }

    impl<'a, T: CType + ?Sized> RefMut<'a, T> {
        /// Recover the interior-mutable reference, returning `None` for a null pointer.
        ///
        /// # Safety
        ///
        /// The pointer must refer to a live `C` that is accessible for the returned lifetime.
        pub const unsafe fn into_ref(self) -> Option<&'a UnsafeCell<T>> {
            if self.data.is_null() {
                return None;
            }

            Some(unsafe { &*(self.data as *const UnsafeCell<T>) })
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
                pub(crate) const unsafe fn cast<U: CType>(self) -> $ty<$($target)* U $($suffix)*> {
                    $ty { data: self.data.cast(), $($marker)* }
                }

                /// Return the stored pointer.
                pub const fn as_ptr(self) -> *$ptr T { self.data }
            }
        };
    }
    impl_ref_pointer_methods! { SoftRef, [T: CType + ?Sized], [T], [], [], const, [] }
    impl_ref_pointer_methods! { SoftRefMut, [T: CType + ?Sized], [T], [], [], mut, [] }

    impl_ref_pointer_methods! { Ref, ['a, T: CType + ?Sized], ['a, T], ['a,], [], const, [lifetime: core::marker::PhantomData,] }
    impl_ref_pointer_methods! { RefMut, ['a, T: CType + ?Sized], ['a, T], ['a,], [], mut, [lifetime: core::marker::PhantomData,] }

    unsafe impl<R: ReprC + ?Sized> RefCarrier<R> for SoftRef<R::CType> {
        fn as_ptr(&self) -> *const R::CType {
            self.data
        }
        unsafe fn from_shared_ptr(data: *const R::CType) -> Self {
            unsafe { Self::from_raw(data) }
        }
    }
    unsafe impl<'a, R: ReprC + ?Sized> RefCarrier<R> for Ref<'a, R::CType> {
        fn as_ptr(&self) -> *const R::CType {
            self.data
        }
        unsafe fn from_shared_ptr(data: *const R::CType) -> Self {
            unsafe { Self::from_raw(data) }
        }
    }

    unsafe impl<R: ReprC + ?Sized> RefMutCarrier<R> for SoftRefMut<R::CType> {
        fn as_ptr(&self) -> *mut R::CType {
            self.data
        }
        unsafe fn from_mut_ptr(data: *mut R::CType) -> Self {
            unsafe { Self::from_raw(data) }
        }
    }
    unsafe impl<'a, R: ReprC + ?Sized> RefMutCarrier<R> for RefMut<'a, R::CType>
    where
        Self: CType,
    {
        fn as_ptr(&self) -> *mut R::CType {
            self.data
        }
        unsafe fn from_mut_ptr(data: *mut R::CType) -> Self {
            unsafe { Self::from_raw(data) }
        }
    }

    macro_rules! impl_reference_carrier {
        ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
            impl_thin_pointer_traits! { $ty, [$($params)*], [$($args)*] }

            impl<$($params)*> core::fmt::Debug for $ty<$($args)*> {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    f.debug_tuple(stringify!($ty)).field(&self.data).finish()
                }
            }

            crate::borrow::__impl_borrowed_identity!([$($params)*] $ty<$($args)*>);

        };
    }

    macro_rules! impl_reference_identity {
        ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
            impl<$($params)*> ReprC for $ty<$($args)*> { type CType = Self; }
            unsafe impl<$($params)*> EncodeMove for $ty<$($args)*> {
                type Store = ();
                fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
                    self
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
            impl<$($params)*> crate::convert::EncodeUnchecked for $ty<$($args)*> {}
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

    impl_reference_carrier! { SoftRef, [T: CType + ?Sized], [T] }
    impl_reference_carrier! { SoftRefMut, [T: CType + ?Sized], [T] }
    impl_reference_identity! { SoftRef, [T: CType + ?Sized], [T] }
    impl_reference_identity! { SoftRefMut, [T: CType + ?Sized], [T] }

    impl_reference_carrier! { Ref, ['a, T: CType + ?Sized], ['a, T] }
    impl_reference_carrier! { RefMut, ['a, T: CType + ?Sized], ['a, T] }
    impl_reference_identity! { Ref, ['a, T: CType + ?Sized], ['a, T] }
    impl_reference_identity! { RefMut, ['a, T: CType + ?Sized], ['a, T] }

    const fn assert_thin_pointer<C: ?Sized>() {
        const {
            assert!(
                core::mem::size_of::<*const C>() == core::mem::size_of::<*const ()>(),
                "reference carrier requires a thin pointer"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use core::cell::UnsafeCell;

    use static_assertions::assert_impl_all;

    use super::abi::{Ref, RefMut, SoftRef, SoftRefMut};
    use crate::{
        CType, ReprC,
        convert::{Decode, Encode},
        niche::Niche,
    };

    struct Payload;
    unsafe impl CType for Payload {}

    #[test]
    fn direct_carriers_accept_lowered_payload_without_source_mapping() {
        static_assertions::assert_type_eq_all!(
            <Ref<'static, Payload> as ReprC>::CType,
            Ref<'static, Payload>
        );
        static_assertions::assert_type_eq_all!(
            <RefMut<'static, Payload> as ReprC>::CType,
            RefMut<'static, Payload>
        );
        static_assertions::assert_type_eq_all!(
            <&'static bool as ReprC>::CType,
            Ref<'static, crate::primitive::abi::bool>
        );
        static_assertions::assert_type_eq_all!(
            <SoftRef<Payload> as ReprC>::CType,
            SoftRef<Payload>
        );
        static_assertions::assert_type_eq_all!(
            <SoftRefMut<Payload> as ReprC>::CType,
            SoftRefMut<Payload>
        );
        static_assertions::assert_type_eq_all!(
            <&'static UnsafeCell<(u32, u32)> as ReprC>::CType,
            SoftRef<UnsafeCell<crate::tuple::abi::Tuple2<u32, u32>>>
        );
        static_assertions::assert_type_eq_all!(
            <&'static mut bool as ReprC>::CType,
            SoftRefMut<crate::primitive::abi::bool>
        );
        let mut payload = Payload;
        let pointer = core::ptr::from_ref(&payload);
        assert_eq!(SoftRef::from_ref(&payload).as_ptr(), pointer);
        assert_eq!(
            SoftRefMut::from_mut(&mut payload).as_ptr().cast_const(),
            pointer
        );
        assert_eq!(Ref::from_ref(&payload).as_ptr(), pointer);
        let exclusive = RefMut::from_mut(&mut payload);
        assert_eq!(exclusive.as_ptr().cast_const(), pointer);
        assert_eq!(
            core::ptr::from_ref(unsafe { exclusive.into_mut() }.unwrap()),
            pointer
        );
    }

    #[test]
    fn lifetime_carriers_preserve_layout_and_borrows() {
        use core::mem::{align_of, size_of};
        assert_eq!(size_of::<Ref<'_, u8>>(), size_of::<SoftRef<u8>>());
        assert_eq!(align_of::<Ref<'_, u8>>(), align_of::<SoftRef<u8>>());
        assert_eq!(size_of::<RefMut<'_, u8>>(), size_of::<SoftRefMut<u8>>());
        let value = 7_u8;
        let shared = Ref::<u8>::from_ref(&value);
        assert_eq!(unsafe { shared.into_ref() }, Some(&7));
        let mut value = 8_u8;
        let exclusive = RefMut::<u8>::from_mut(&mut value);
        *unsafe { exclusive.into_mut().unwrap() } = 9;
        assert_eq!(value, 9);
        let cell = UnsafeCell::new(2_u8);
        let shared_cell = Ref::<UnsafeCell<u8>>::from_ref(&cell);
        assert_eq!(unsafe { shared_cell.into_ref().unwrap() }.get(), cell.get());
        assert!(unsafe { Ref::<u8>::null().into_ref() }.is_none());
    }

    #[test]
    fn shared_reference_carrier_preserves_pointer_abi() {
        assert_impl_all!(&u8: Niche<CType = Ref<'static, u8>>, Encode, Decode<'static>);
        assert_impl_all!(&SoftRef<u8>: Niche<CType = Ref<'static, SoftRef<u8>>>, Encode, Decode<'static>);
        assert_eq!(
            core::mem::size_of::<SoftRef<u8>>(),
            core::mem::size_of::<*const u8>()
        );
        assert_eq!(
            core::mem::align_of::<SoftRef<u8>>(),
            core::mem::align_of::<*const u8>()
        );

        let value = 7_u8;
        let carrier = crate::encode(&value);
        assert_eq!(carrier, Ref::from_ref(&value));
        assert_eq!(unsafe { crate::decode::<&u8>(carrier) }, Some(&value));
    }

    #[test]
    fn exclusive_and_shared_mutable_references_have_distinct_carriers() {
        assert_impl_all!(&mut u8: Niche<CType = crate::reference::abi::RefMut<'static, u8>>);
        assert_impl_all!(&UnsafeCell<u8>: Niche<CType = crate::reference::abi::Ref<'static, UnsafeCell<u8>>>);
        assert_impl_all!(&mut UnsafeCell<u8>: Niche<CType = crate::reference::abi::RefMut<'static, UnsafeCell<u8>>>);
        assert_impl_all!(&mut [u8]: Niche<CType = crate::wide::abi::WideRef<'static, [u8]>>);
        assert_impl_all!(&[UnsafeCell<u8>]: Niche<CType = crate::wide::abi::WideRef<'static, [UnsafeCell<u8>]>>);
        assert_impl_all!(&mut [UnsafeCell<u8>]: Niche<CType = crate::wide::abi::WideRef<'static, [UnsafeCell<u8>]>>);

        assert_eq!(
            core::mem::size_of::<crate::reference::abi::RefMut<'static, u8>>(),
            core::mem::size_of::<*mut u8>()
        );
        assert_eq!(
            core::mem::align_of::<crate::reference::abi::RefMut<'static, u8>>(),
            core::mem::align_of::<*mut u8>()
        );

        let mut cell = UnsafeCell::new(1_u8);
        let pointer = cell.get().cast::<core::cell::UnsafeCell<u8>>();
        let exclusive: RefMut<'_, UnsafeCell<u8>> = crate::encode(&mut cell);
        assert_eq!(exclusive.as_ptr(), pointer);
        unsafe { exclusive.as_ptr().write(core::cell::UnsafeCell::new(2)) };
        assert_eq!(cell.into_inner(), 2);
    }

    const CONST_NULL_REF: Option<&'static u8> = unsafe { SoftRef::<u8>::null().into_ref() };

    #[test]
    fn const_null_reference_has_no_payload() {
        assert_eq!(CONST_NULL_REF, None);
    }

    #[test]
    fn shared_interior_decode_rejects_null() {
        assert!(unsafe { crate::decode::<&core::cell::Cell<u8>>(Ref::null()) }.is_none());
        assert!(
            unsafe { crate::decode::<&[core::cell::Cell<u8>]>(crate::wide::abi::WideRef::null()) }
                .is_none()
        );
    }
}
