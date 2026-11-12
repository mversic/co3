#[cfg(feature = "alloc")]
pub use alloc::borrow::ToOwned;

use disjoint_impls::disjoint_impls;
use rust_spec::{RustSpec, Stable, Unstable};

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    reference::SharedRefCarrier,
    stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked, Store},
    sync::TakeDecoded,
    transmute::{AbiCompatible, AbiCompatibleValidate},
};

/// Encodes a value without transferring its owned resources across FFI.
///
/// # Safety
/// The carrier must obey borrowed access and remain valid for the store's lifetime.
/// An empty store must not own backing storage needed by the carrier.
#[doc(hidden)]
pub unsafe trait BorrowedEncodeUnchecked: ReprC<CType: Sized> + Sized {
    type SoftStore: Store + Default;
    unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
    where
        Self: 'itm;
}

/// Checked borrowed encoding.
///
/// # Safety
/// The carrier must satisfy the checked borrowed-access contract and the unchecked
/// trait's storage requirements.
#[doc(hidden)]
pub unsafe trait BorrowedEncode: BorrowedEncodeUnchecked {
    type Store: Store + Default;
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm;
}

/// Decodes a borrowed carrier into the original Rust type.
///
/// # Safety
/// Decoding must preserve the source's ownership and reference access requirements.
/// As with [`DecodeMove`], an empty store must not back references in the result.
#[doc(hidden)]
pub unsafe trait BorrowedDecode<'d>: ReprC<CType: Sized> + Sized {
    type Store: Store + Default;
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self>;
    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Self {
        unsafe { Self::soft_decode(source, store) }.expect("unchecked decoding produces a value")
    }
}

/// Encode a borrowed value while retaining its source type in the ABI marker.
#[doc(hidden)]
pub fn soft_encode<T: BorrowedEncode>(
    value: T,
    store: &mut T::Store,
) -> crate::borrow::CBorrowed<T> {
    crate::borrow::CBorrowed(T::soft_encode(value, store))
}
#[doc(hidden)]
pub fn encode<T: BorrowedEncode<Store: crate::stored::EmptyStore>>(
    value: T,
) -> crate::borrow::CBorrowed<T> {
    soft_encode(value, &mut Default::default())
}
/// Decode a borrowed ABI marker without consuming foreign ownership.
/// # Safety
/// The payload must satisfy `BorrowedDecode`'s access and validity contract.
#[doc(hidden)]
pub unsafe fn soft_decode<'d, T: BorrowedDecode<'d>>(
    value: crate::borrow::CBorrowed<T>,
    store: &'d mut T::Store,
) -> Option<T> {
    unsafe { T::soft_decode(value.0, store) }
}
/// # Safety
/// The payload must satisfy `BorrowedDecode`'s unchecked contract.
#[doc(hidden)]
pub unsafe fn soft_decode_unchecked<'d, T: BorrowedDecode<'d>>(
    value: crate::borrow::CBorrowed<T>,
    store: &'d mut T::Store,
) -> T {
    unsafe { T::soft_decode_unchecked(value.0, store) }
}
/// # Safety
/// The payload must satisfy `BorrowedDecode`'s access and validity contract.
#[doc(hidden)]
pub unsafe fn decode<'d, T: BorrowedDecode<'d, Store: crate::stored::EmptyStore + 'd>>(
    value: crate::borrow::CBorrowed<T>,
) -> Option<T> {
    let mut store = T::Store::default();
    // EmptyStore forbids references into the store from escaping decoding.
    let store = unsafe { core::mem::transmute::<&mut T::Store, &'d mut T::Store>(&mut store) };
    unsafe { T::soft_decode(value.0, store) }
}

/// Implements borrowed conversion by delegating to ordinary value conversion.
#[doc(hidden)]
#[macro_export]
macro_rules! __impl_borrowed_identity {
    ([] $ty:ty $(where $($bounds:tt)*)?) => {
        $crate::__impl_borrowed_identity!(@impl [for<'_co3>] [] $ty $(where $($bounds)*)?);
    };
    ([$($generics:tt)+] $ty:ty $(where $($bounds:tt)*)?) => {
        $crate::__impl_borrowed_identity!(@impl [] [$($generics)+] $ty $(where $($bounds)*)?);
    };
    (@impl [$($speculative:tt)*] [$($generics:tt)*] $ty:ty $(where $($bounds:tt)*)?) => {
        unsafe impl<$($generics)*> $crate::borrow::BorrowedEncodeUnchecked for $ty
        where $($speculative)* Self: $crate::stored::EncodeMoveUnchecked, $($($bounds)*)? {
            type SoftStore = <Self as $crate::stored::EncodeMoveUnchecked>::SoftStore;
            unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
            where Self: 'itm {
                unsafe { $crate::stored::EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
            }
        }
        unsafe impl<$($generics)*> $crate::borrow::BorrowedEncode for $ty
        where $($speculative)* Self: $crate::stored::EncodeMove, $($($bounds)*)? {
            type Store = <Self as $crate::stored::EncodeMove>::Store;
            fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
            where Self: 'itm {
                $crate::stored::EncodeMove::soft_encode(self, store)
            }
        }
        unsafe impl<'_co3_d, $($generics)*> $crate::borrow::BorrowedDecode<'_co3_d> for $ty
        where $($speculative)* Self: $crate::stored::DecodeMove<'_co3_d>, $($($bounds)*)? {
            type Store = <Self as $crate::stored::DecodeMove<'_co3_d>>::Store;
            unsafe fn soft_decode<'itm: '_co3_d>(source: Self::CType, store: &'itm mut Self::Store) -> core::option::Option<Self> {
                unsafe { $crate::stored::DecodeMove::soft_decode(source, store) }
            }
            unsafe fn soft_decode_unchecked<'itm: '_co3_d>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
                unsafe { $crate::stored::DecodeMove::soft_decode_unchecked(source, store) }
            }
        }
    };
}

/// Retains the original value until reference conversion and synchronization finish.
/// This store must not implement `EmptyStore`: the encoded carrier can point
/// into `value`, which is dropped when the store is dropped.
pub struct BorrowedEncodeStore<T, S> {
    value: Option<T>,
    reference: S,
}
impl<T, S: Default> Default for BorrowedEncodeStore<T, S> {
    fn default() -> Self {
        Self {
            value: None,
            reference: S::default(),
        }
    }
}
impl<T, S: Store> Store for BorrowedEncodeStore<T, S> {
    fn sync(self) -> Option<()> {
        let Self { value, reference } = self;
        let result = reference.sync();
        drop(value);
        result
    }
}
// Keep the higher-ranked bound outside disjoint_impls, which cannot expand its binder.
#[doc(hidden)]
pub trait BorrowedOwnerEncode: ReprC<CType: Copy> + Sized {
    type ReferenceStore: Store + Default;
    fn encode_borrowed(
        self,
        store: &mut BorrowedEncodeStore<Self, Self::ReferenceStore>,
    ) -> Self::CType;
}
impl<T: ReprC<CType: Copy>, S: Store + Default> BorrowedOwnerEncode for T
where
    for<'a> &'a mut Self:
        EncodeMoveUnchecked<SoftStore = S, CType: crate::reference::MutRefCarrier<Self, true>>,
{
    type ReferenceStore = S;
    fn encode_borrowed(
        self,
        store: &mut BorrowedEncodeStore<Self, Self::ReferenceStore>,
    ) -> Self::CType {
        let value = store.value.insert(self);
        // SAFETY: The returned borrowed carrier permits only read-only foreign access.
        let encoded =
            unsafe { EncodeMoveUnchecked::soft_encode_unchecked(value, &mut store.reference) };
        // SAFETY: The encoded carrier is initialized and Copy, with its owner retained.
        unsafe { crate::reference::MutRefCarrier::as_ptr(&encoded).read() }
    }
}

// Owner decoding is separate from fieldwise decoding so custom Drop stays intact.
disjoint_impls! {
    #[doc(hidden)]
    pub unsafe trait BorrowedOwnerDecode<'d>: ReprC<CType: Sized> + Sized {
        type Store: Store + Default;
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self>;
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self;
    }
    unsafe impl<'d, T: Clone + ReprC<CType: CType + Sized> + 'd> BorrowedOwnerDecode<'d> for T
    where
        T: RustSpec<Layout = Stable, Drop: crate::NeedsDrop>,
        &'d T: DecodeMove<'d, CType: SharedRefCarrier<T>, Store = ()>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut (),
        ) -> core::option::Option<Self> {
            let carrier = core::ptr::from_ref(&source);

            let carrier = unsafe { SharedRefCarrier::from_shared_ptr(carrier) };
            let value = unsafe { <&'d T as DecodeMove<'d>>::soft_decode(carrier, store)? };
            Some(T::clone(value))
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut (),
        ) -> Self {
            let carrier = core::ptr::from_ref(&source);

            let carrier = unsafe { SharedRefCarrier::from_shared_ptr(carrier) };
            let value = unsafe { <&'d T as DecodeMove<'d>>::soft_decode_unchecked(carrier, store) };
            T::clone(value)
        }
    }
    unsafe impl<'d, T: ReprC<CType: CType + Sized> + 'd> BorrowedOwnerDecode<'d> for T
    where
        T: RustSpec<Layout = Unstable, Drop: crate::NeedsDrop>,
        &'d T: DecodeMove<'d, CType: SharedRefCarrier<T>>,
        <&'d T as DecodeMove<'d>>::Store: TakeDecoded<Value = T>,
    {
        type Store = <&'d T as DecodeMove<'d>>::Store;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> core::option::Option<Self> {
            let store_ptr = core::ptr::from_mut(store);
            // SAFETY: The source is live through decoding. TakeDecoded guarantees
            // that the extracted value does not depend on this temporary carrier.
            let carrier = unsafe { SharedRefCarrier::from_shared_ptr(core::ptr::from_ref(&source)) };
            let _ = unsafe { <&'d T as DecodeMove<'d>>::soft_decode(carrier, store)? };
            unsafe { <<&'d T as DecodeMove<'d>>::Store as TakeDecoded>::take_decoded(store_ptr) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let store_ptr = core::ptr::from_mut(store);
            // SAFETY: The caller guarantees validity; TakeDecoded guarantees
            // independence from this temporary carrier.
            let carrier = unsafe { SharedRefCarrier::from_shared_ptr(core::ptr::from_ref(&source)) };
            let _ = unsafe { <&'d T as DecodeMove<'d>>::soft_decode_unchecked(carrier, store) };
            unsafe { <<&'d T as DecodeMove<'d>>::Store as TakeDecoded>::take_decoded(store_ptr) }.expect("unchecked decoding produces a value")
        }
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! __impl_borrowed_owner {
    ([] $ty:ty $(where $($bounds:tt)*)?) => {
        $crate::__impl_borrowed_owner!(@impl [for<'_co3>] [] $ty $(where $($bounds)*)?);
    };
    ([$($generics:tt)+] $ty:ty $(where $($bounds:tt)*)?) => {
        $crate::__impl_borrowed_owner!(@impl [] [$($generics)+] $ty $(where $($bounds)*)?);
    };
    (@impl [$($speculative:tt)*] [$($generics:tt)*] $ty:ty $(where $($bounds:tt)*)?) => {
        unsafe impl<$($generics)*> $crate::borrow::BorrowedEncodeUnchecked for $ty
        where $($speculative)* Self: $crate::borrow::BorrowedOwnerEncode, $($($bounds)*)? {
            type SoftStore = $crate::borrow::BorrowedEncodeStore<Self, <Self as $crate::borrow::BorrowedOwnerEncode>::ReferenceStore>;
            unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
            where Self: 'itm {
                $crate::borrow::BorrowedOwnerEncode::encode_borrowed(self, store)
            }
        }
        unsafe impl<$($generics)*> $crate::borrow::BorrowedEncode for $ty
        where $($speculative)* Self: $crate::borrow::BorrowedOwnerEncode, $($($bounds)*)? {
            type Store = <Self as $crate::borrow::BorrowedEncodeUnchecked>::SoftStore;
            fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
            where Self: 'itm {
                $crate::borrow::BorrowedOwnerEncode::encode_borrowed(self, store)
            }
        }
        unsafe impl<'_co3_d, $($generics)*> $crate::borrow::BorrowedDecode<'_co3_d> for $ty
        where $($speculative)* Self: $crate::borrow::BorrowedOwnerDecode<'_co3_d>, $($($bounds)*)? {
            type Store = <Self as $crate::borrow::BorrowedOwnerDecode<'_co3_d>>::Store;
            unsafe fn soft_decode<'itm: '_co3_d>(source: Self::CType, store: &'itm mut Self::Store) -> core::option::Option<Self> {
                unsafe { $crate::borrow::BorrowedOwnerDecode::soft_decode(source, store) }
            }
            unsafe fn soft_decode_unchecked<'itm: '_co3_d>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
                unsafe { $crate::borrow::BorrowedOwnerDecode::soft_decode_unchecked(source, store) }
            }
        }
    };
}

#[cfg(feature = "alloc")]
crate::__impl_borrowed_owner!([T: ?Sized] alloc::boxed::Box<T>);
#[cfg(feature = "alloc")]
crate::__impl_borrowed_owner!([T] alloc::vec::Vec<T>);
#[cfg(feature = "alloc")]
crate::__impl_borrowed_owner!([T] crate::vec::Vec<T>);
#[cfg(feature = "alloc")]
crate::__impl_borrowed_owner!([] alloc::string::String);
#[cfg(feature = "alloc")]
crate::__impl_borrowed_owner!([] alloc::ffi::CString);

macro_rules! reference_borrowed {
    ($($mutability:tt)?) => {
        unsafe impl<'a, T: ?Sized> BorrowedEncodeUnchecked for &'a $($mutability)? T
        where Self: EncodeMoveUnchecked {
            type SoftStore = <Self as EncodeMoveUnchecked>::SoftStore;
            unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
            where Self: 'itm {
                unsafe { EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
            }
        }
        unsafe impl<'d, 'a, T: ?Sized> BorrowedDecode<'d> for &'a $($mutability)? T
        where Self: DecodeMove<'d> {
            type Store = <Self as DecodeMove<'d>>::Store;
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> core::option::Option<Self> {
                unsafe { DecodeMove::soft_decode(source, store) }
            }
            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
                unsafe { DecodeMove::soft_decode_unchecked(source, store) }
            }
        }
    };
}
reference_borrowed!();
reference_borrowed!(mut);

unsafe impl<'a, T: ?Sized> BorrowedEncode for &'a mut T
where
    Self: EncodeMoveUnchecked,
{
    type Store = <Self as EncodeMoveUnchecked>::SoftStore;
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        // The borrowed carrier retains the existing restricted foreign-access contract.
        unsafe { EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
    }
}

const _: () = {
    use rust_spec::mutability::{Exclusive, WithInterior, WithoutInterior};
    disjoint_impls! {
        #[disjoint_impls(remote)]
        pub unsafe trait BorrowedEncode: BorrowedEncodeUnchecked {
            type Store: Store + Default;
            fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm;
        }
        unsafe impl<'a, T: ?Sized> BorrowedEncode for &'a T
        where T: RustSpec<Mutability = Exclusive<WithoutInterior>>, Self: EncodeMove {
            type Store = <Self as EncodeMove>::Store;
            fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
                EncodeMove::soft_encode(self, store)
            }
        }
        unsafe impl<'a, T: ?Sized, I> BorrowedEncode for &'a T
        where T: RustSpec<Mutability = Exclusive<WithInterior<I>>>, Self: EncodeMoveUnchecked {
            type Store = <Self as EncodeMoveUnchecked>::SoftStore;
            fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
                unsafe { EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
            }
        }
        unsafe impl<'a, T: ?Sized, I> BorrowedEncode for &'a T
        where T: RustSpec<Mutability = WithInterior<I>>, Self: EncodeMoveUnchecked {
            type Store = <Self as EncodeMoveUnchecked>::SoftStore;
            fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm {
                unsafe { EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
            }
        }
    }
};

// Owned cells can be opened; borrowed shared cells still use the reference codecs above.
macro_rules! cell_borrowed {
    ($cell:ident) => {
        unsafe impl<T: BorrowedEncodeUnchecked> BorrowedEncodeUnchecked for core::cell::$cell<T> {
            type SoftStore = T::SoftStore;
            unsafe fn soft_encode_unchecked<'itm>(
                self,
                store: &'itm mut Self::SoftStore,
            ) -> Self::CType
            where
                Self: 'itm,
            {
                core::cell::$cell::new(unsafe {
                    T::soft_encode_unchecked(self.into_inner(), store)
                })
            }
        }
        unsafe impl<T: BorrowedEncode> BorrowedEncode for core::cell::$cell<T> {
            type Store = T::Store;
            fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
            where
                Self: 'itm,
            {
                core::cell::$cell::new(T::soft_encode(self.into_inner(), store))
            }
        }
        unsafe impl<'d, T: BorrowedDecode<'d>> BorrowedDecode<'d> for core::cell::$cell<T> {
            type Store = T::Store;
            unsafe fn soft_decode<'itm: 'd>(
                source: Self::CType,
                store: &'itm mut Self::Store,
            ) -> core::option::Option<Self> {
                unsafe { T::soft_decode(source.into_inner(), store) }.map(Self::new)
            }
            unsafe fn soft_decode_unchecked<'itm: 'd>(
                source: Self::CType,
                store: &'itm mut Self::Store,
            ) -> Self {
                Self::new(unsafe { T::soft_decode_unchecked(source.into_inner(), store) })
            }
        }
    };
}
cell_borrowed!(Cell);
cell_borrowed!(UnsafeCell);

unsafe impl<T: BorrowedEncodeUnchecked, const N: usize> BorrowedEncodeUnchecked for [T; N] {
    type SoftStore = crate::sync::ArrayStore<T::SoftStore, N>;
    unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
    where
        Self: 'itm,
    {
        crate::elements::map_array(self, &mut store.0, |value, store| unsafe {
            T::soft_encode_unchecked(value, store)
        })
    }
}
unsafe impl<T: BorrowedEncode, const N: usize> BorrowedEncode for [T; N] {
    type Store = crate::sync::ArrayStore<T::Store, N>;
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        crate::elements::map_array(self, &mut store.0, T::soft_encode)
    }
}
unsafe impl<'d, T: BorrowedDecode<'d>, const N: usize> BorrowedDecode<'d> for [T; N] {
    type Store = crate::sync::ArrayStore<T::Store, N>;
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> core::option::Option<Self> {
        crate::elements::try_map_array(source, &mut store.0, |value, store| unsafe {
            T::soft_decode(value, store)
        })
    }
    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Self {
        crate::elements::map_array(source, &mut store.0, |value, store| unsafe {
            T::soft_decode_unchecked(value, store)
        })
    }
}

#[cfg(all(test, feature = "alloc", feature = "derive"))]
mod tests {
    use crate::borrow::CBorrowed;
    use crate::{ReprC, Store, rust_spec::RustSpec};
    use alloc::boxed::Box;
    use core::{
        cell::Cell,
        sync::atomic::{AtomicUsize, Ordering},
    };

    // Deliberately not Clone: borrowing converts each field on the original type.
    #[derive(RustSpec, ReprC)]
    struct Args<'a> {
        owner: Box<u32>,
        target: &'a mut u32,
    }

    #[test]
    fn aggregate_borrows_fields_without_cloning_the_aggregate() {
        let mut target = 9;
        let owner = Box::new(7);
        let original = core::ptr::from_ref(&*owner);
        let mut encode_store = Default::default();
        let encoded = crate::borrow::soft_encode(
            Args {
                owner,
                target: &mut target,
            },
            &mut encode_store,
        );
        let carrier = encoded.0;
        assert_eq!(carrier.owner.as_ptr().cast_const(), original);
        assert_eq!(unsafe { *carrier.target.as_ptr() }, 9);
        let mut decode_store = Default::default();
        let decoded = unsafe {
            crate::borrow::soft_decode::<Args<'_>>(CBorrowed(carrier), &mut decode_store)
        }
        .unwrap();
        let decoded = decoded;
        assert_eq!(*decoded.owner, 7);
        assert_ne!(core::ptr::from_ref(&*decoded.owner), original);
        assert_eq!(*decoded.target, 9);
        drop(decoded);
        decode_store.sync().unwrap();
        encode_store.sync().unwrap();
    }

    #[derive(RustSpec, ReprC)]
    #[rust_spec(non_robust)]
    #[repr_c(is_valid = |value: &Box<u32>| **value != 0)]
    struct NonZeroBox(Box<u32>);

    #[test]
    fn borrowed_validation_receives_original_fields() {
        let mut encode_store = Default::default();
        let carrier = crate::borrow::soft_encode(NonZeroBox(Box::new(5)), &mut encode_store);
        let mut decode_store = Default::default();
        let decoded =
            unsafe { crate::borrow::soft_decode::<NonZeroBox>(carrier, &mut decode_store) }
                .unwrap();
        assert_eq!(*decoded.0, 5);
        decode_store.sync().unwrap();
        encode_store.sync().unwrap();

        let owner = Box::new(0_u32);
        let carrier = unsafe {
            <NonZeroBox as crate::ReprC>::CType::from_fields(crate::boxed::CBox::from_box(owner))
        };
        let pointer = carrier.0;
        let mut decode_store = Default::default();
        assert!(
            unsafe {
                crate::borrow::soft_decode::<NonZeroBox>(CBorrowed(carrier), &mut decode_store)
            }
            .is_none()
        );
        decode_store.sync().unwrap();
        // Borrowed decoding did not consume the foreign allocation, even on rejection.
        let owner = unsafe { pointer.into_box() }.unwrap();
        assert_eq!(*owner, 0);
    }

    #[derive(RustSpec, ReprC)]
    enum Choice<'a> {
        Owner(Box<u32>),
        Reference(&'a mut u32),
    }

    #[test]
    fn enum_preserves_the_active_borrowed_payload() {
        let mut encode_store = Default::default();
        let encoded = crate::borrow::soft_encode(Choice::Owner(Box::new(12)), &mut encode_store);
        let mut decode_store = Default::default();
        let decoded =
            unsafe { crate::borrow::soft_decode::<Choice<'_>>(encoded, &mut decode_store) }
                .unwrap();
        let Choice::Owner(value) = decoded else {
            panic!("wrong variant")
        };
        assert_eq!(*value, 12);
        drop(value);
        decode_store.sync().unwrap();
        encode_store.sync().unwrap();
    }

    static DROPS: AtomicUsize = AtomicUsize::new(0);
    #[derive(Clone, RustSpec, ReprC)]
    #[rust_spec(custom_drop)]
    #[repr(C)]
    struct Owner(u8);
    impl Drop for Owner {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn custom_drop_owner_is_retained_whole_until_store_finishes() {
        DROPS.store(0, Ordering::SeqCst);
        let mut encode_store = Default::default();
        let encoded = crate::borrow::soft_encode(Owner(4), &mut encode_store);
        assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        let mut decode_store = Default::default();
        let decoded =
            unsafe { crate::borrow::soft_decode::<Owner>(encoded, &mut decode_store) }.unwrap();
        assert_eq!(decoded.0, 4);
        drop(decoded);
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
        decode_store.sync().unwrap();
        encode_store.sync().unwrap();
        assert_eq!(DROPS.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn arrays_and_owned_cells_retain_borrowed_owners() {
        let mut encode_store = Default::default();
        let encoded = crate::borrow::soft_encode([Cell::new(Box::new(3_u32))], &mut encode_store);
        let mut decode_store = Default::default();
        let decoded = unsafe {
            crate::borrow::soft_decode::<[Cell<Box<u32>>; 1]>(encoded, &mut decode_store)
        }
        .unwrap();
        let [cell] = decoded;
        assert_eq!(*cell.into_inner(), 3);
        decode_store.sync().unwrap();
        encode_store.sync().unwrap();
    }

    #[derive(RustSpec, ReprC)]
    struct InteriorMutableField(Cell<u8>);

    #[test]
    fn borrowed_wrapper_keeps_owner_axes() {
        static_assertions::assert_impl_all!(InteriorMutableField:
            RustSpec<Mutability = co3::rust_spec::mutability::WithInterior<co3::rust_spec::layout::Robust>>,
        );
        static_assertions::assert_impl_all!(co3::borrow::CBorrowed<InteriorMutableField>:
            RustSpec<
                Layout = rust_spec::Stable,
                Size = <InteriorMutableField as RustSpec>::Size,
                __Alignment = <InteriorMutableField as RustSpec>::__Alignment,
                Trap = <InteriorMutableField as RustSpec>::Trap,
                Niche = <InteriorMutableField as RustSpec>::Niche,
                Drop = co3::rust_spec::drop::NoDrop,
                Mutability = co3::rust_spec::mutability::WithInterior<co3::rust_spec::layout::Robust>,
                __IndirectTrap = <InteriorMutableField as RustSpec>::__IndirectTrap,
            >,
        );
    }

    #[test]
    fn identity_carriers_and_wrappers_implement_borrowed_codecs() {
        use super::{BorrowedDecode, BorrowedEncode, BorrowedEncodeUnchecked};
        use crate::reference::CSoftRefMut;
        use core::{mem::ManuallyDrop, num::NonZero};
        use static_assertions::{assert_impl_all, assert_not_impl_any};

        #[derive(RustSpec, ReprC)]
        #[repr(transparent)]
        struct Source<T>(T);
        #[derive(RustSpec, ReprC)]
        #[repr(C)]
        struct SourceZst;
        #[derive(RustSpec, ReprC)]
        #[repr(u8)]
        enum Sparse {
            A(u32) = 1,
            B,
            C = 5,
        }
        #[derive(RustSpec, ReprC)]
        #[repr_c(identity)]
        #[repr(transparent)]
        struct Bytes([u8]);

        macro_rules! borrowed {
            ($($ty:ty),* $(,)?) => { $(
                assert_impl_all!($ty: BorrowedEncode, BorrowedEncodeUnchecked, BorrowedDecode<'static>);
            )* };
        }
        borrowed!(Source<u32>, Source<()>, SourceZst, Sparse, <Sparse as co3::ReprC>::CType,
            Option<NonZero<u8>>, Option<extern "C" fn()>,
            Result<NonZero<u8>, ()>, Result<(), NonZero<u8>>,
            Cell<u8>, ManuallyDrop<u8>, CSoftRefMut<u8, true>);
        assert_not_impl_any!(Bytes: BorrowedEncode);
        assert_not_impl_any!(crate::borrow::CBorrowed<()>: crate::CFnArg, crate::CFnReturn);
    }
}

/// Carrier retaining the original Rust type parameter.
#[repr(transparent)]
#[derive(RustSpec)]
#[doc(hidden)]
pub struct CBorrowed<T: ReprC<CType: Sized>>(pub T::CType);
impl<T: ReprC<CType: Sized>> ReprC for CBorrowed<T> {
    type CType = Self;
}
unsafe impl<T: ReprC<CType: Sized>> CType for CBorrowed<T> {}
unsafe impl<T: ReprC<CType: Sized>> AbiCompatibleValidate for CBorrowed<T> {
    unsafe fn is_valid(_: &Self) -> bool {
        true
    }
}
unsafe impl<T: ReprC<CType: Sized>> AbiCompatible for CBorrowed<T> {}
unsafe impl<T: ReprC<CType: Sized>> CFnArg for CBorrowed<T> where T::CType: CFnArg {}
unsafe impl<T: ReprC<CType: Sized>> CFnReturn for CBorrowed<T>
where
    T::CType: CFnReturn,
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
{
}
impl<T: ReprC<CType: Sized>> CBorrowed<T> {
    pub const fn new(value: T::CType) -> Self {
        Self(value)
    }
    pub fn into_inner(self) -> T::CType {
        self.0
    }
}
unsafe impl<T: ReprC<CType: Sized>> EncodeMoveUnchecked for CBorrowed<T> {
    type SoftStore = ();
    unsafe fn soft_encode_unchecked<'a>(self, _: &'a mut ()) -> Self
    where
        Self: 'a,
    {
        self
    }
}
unsafe impl<T: ReprC<CType: Sized>> EncodeMove for CBorrowed<T> {
    type Store = ();
    fn soft_encode<'a>(self, _: &'a mut ()) -> Self
    where
        Self: 'a,
    {
        self
    }
}
unsafe impl<'d, T: ReprC<CType: Sized>> DecodeMove<'d> for CBorrowed<T> {
    type Store = ();
    unsafe fn soft_decode<'a: 'd>(value: Self, _: &'a mut ()) -> Option<Self> {
        Some(value)
    }
}
impl<T: ReprC<CType: Sized>> Encode for CBorrowed<T> {}
impl<T: ReprC<CType: Sized>> crate::EncodeUnchecked for CBorrowed<T> {}
impl<'d, T: ReprC<CType: Sized>> Decode<'d> for CBorrowed<T> {}
crate::__impl_borrowed_identity!([T: ReprC<CType: Sized>] CBorrowed<T>);
impl<T: ReprC<CType: Copy>> Copy for CBorrowed<T> {}
impl<T: ReprC<CType: Copy>> Clone for CBorrowed<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: ReprC<CType: core::fmt::Debug + Sized>> core::fmt::Debug for CBorrowed<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("CBorrowed").field(&self.0).finish()
    }
}
impl<T: ReprC<CType: PartialEq + Sized>> PartialEq for CBorrowed<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T: ReprC<CType: Eq + Sized>> Eq for CBorrowed<T> {}
