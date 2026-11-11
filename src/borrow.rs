#[cfg(feature = "alloc")]
pub use alloc::borrow::ToOwned;
#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};

use disjoint_impls::disjoint_impls;
use rust_spec::{
    RustSpec,
    mutability::{Exclusive, WithInterior, WithoutInterior},
};

use crate::{
    CFnArg, CFnReturn, CType, ReprC,
    niche::Niche,
    stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked, Store},
    transmute::{AbiCompatible, AbiCompatibleValidate},
};

disjoint_impls! {
    /// A trait for structurally borrowing data.
    ///
    /// # Safety
    ///
    /// `Borrowed` must be correctly categorized as:
    ///
    /// - `Self` for types without drop glue
    /// - structural view for types with drop glue
    /// - [`Borrowed<Self>`] for types with custom drop
    pub unsafe trait Borrow: Sized {
        type Borrowed;

        fn borrow(self) -> Self::Borrowed;

        /// Construct owned form from [`Borrow::Borrowed`]
        fn from_borrow(source: Self::Borrowed) -> Self;
    }

    unsafe impl<R: ?Sized> Borrow for &R
    where
        R: RustSpec<Mutability = Exclusive<WithoutInterior>>
    {
        type Borrowed = Self;

        #[inline(always)]
        fn borrow(self) -> Self::Borrowed {
            self
        }

        #[inline(always)]
        fn from_borrow(source: Self::Borrowed) -> Self {
            source
        }
    }
    unsafe impl<R: ?Sized, I> Borrow for &R
    where
        R: RustSpec<Mutability = Exclusive<WithInterior<I>>>
    {
        type Borrowed = Borrowed<Self>;

        #[inline(always)]
        fn borrow(self) -> Self::Borrowed {
            Borrowed(self)
        }

        #[inline(always)]
        fn from_borrow(source: Self::Borrowed) -> Self {
            source.0
        }
    }
    unsafe impl<R: ?Sized, I> Borrow for &R
    where
        R: RustSpec<Mutability = WithInterior<I>>
    {
        type Borrowed = Borrowed<Self>;

        #[inline(always)]
        fn borrow(self) -> Self::Borrowed {
            Borrowed(self)
        }

        #[inline(always)]
        fn from_borrow(source: Self::Borrowed) -> Self {
            source.0
        }
    }
}

/// Marker for types that are borrowed and clone across the FFI boundary.
///
/// Foreign consumers should only read the carrier, even if its pointer type permits writes.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
#[repr(transparent)]
pub struct Borrowed<T>(T);

impl<T> Borrowed<T> {
    /// Wrap a value for read-only borrowed conversion, taking ownership of it.
    #[inline]
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    #[inline]
    pub fn into_inner(value: Self) -> T {
        value.0
    }
}

impl<T> From<T> for Borrowed<T> {
    #[inline]
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: ReprC<CType: Sized>> ReprC for Borrowed<T> {
    type CType = Borrowed<T::CType>;
}

impl<'d, R> crate::Decode<'d> for Borrowed<R> where Self: DecodeMove<'d> {}
impl<T> crate::EncodeUnchecked for Borrowed<T> where Self: EncodeMoveUnchecked {}
impl<T> crate::Encode for Borrowed<T> where Self: EncodeMove {}

unsafe impl<C: CType> CType for Borrowed<C> {}
unsafe impl<C: CType> crate::encode_repr::EncodeRepr for Borrowed<C> where Self: Sized { type Encoded = Self; }
unsafe impl<C: CFnArg> CFnArg for Borrowed<C> {}
unsafe impl<C: CFnReturn> CFnReturn for Borrowed<C> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}

unsafe impl<T: AbiCompatibleValidate<CType: Sized>> AbiCompatibleValidate for Borrowed<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(&target.0) }
    }
}
unsafe impl<T: AbiCompatible<CType: Sized>> AbiCompatible for Borrowed<T> {}

impl<T: Niche> Niche for Borrowed<T> {
    const NICHE: Self::CType = Borrowed(T::NICHE);

    fn is_niche(value: &Self::CType) -> bool {
        T::is_niche(&value.0)
    }
}

unsafe impl<R: ?Sized> Borrow for &mut R {
    type Borrowed = Borrowed<Self>;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        Borrowed(self)
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        source.0
    }
}

#[cfg(feature = "alloc")]
unsafe impl<R: ?Sized> Borrow for Box<R> {
    type Borrowed = Borrowed<Self>;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        Borrowed(self)
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        source.0
    }
}

#[cfg(feature = "alloc")]
unsafe impl<R> Borrow for Vec<R> {
    type Borrowed = Borrowed<Self>;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        Borrowed::new(self)
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        Borrowed::into_inner(source)
    }
}

unsafe impl<R> Borrow for Borrowed<R> {
    type Borrowed = Self;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        self
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        source
    }
}

#[cfg(feature = "alloc")]
unsafe impl Borrow for alloc::ffi::CString {
    type Borrowed = Borrowed<Self>;

    fn borrow(self) -> Self::Borrowed {
        Borrowed::new(self)
    }

    fn from_borrow(source: Self::Borrowed) -> Self {
        Borrowed::into_inner(source)
    }
}
unsafe impl<R: Borrow, const N: usize> Borrow for [R; N] {
    type Borrowed = [R::Borrowed; N];

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        self.map(Borrow::borrow)
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        source.map(R::from_borrow)
    }
}

unsafe impl<R: Borrow> Borrow for Option<R> {
    type Borrowed = Option<R::Borrowed>;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        self.map(Borrow::borrow)
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        source.map(R::from_borrow)
    }
}

unsafe impl<T: Borrow, E: Borrow> Borrow for Result<T, E> {
    type Borrowed = Result<T::Borrowed, E::Borrowed>;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        self.map(Borrow::borrow).map_err(Borrow::borrow)
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        match source {
            Ok(value) => Ok(T::from_borrow(value)),
            Err(err) => Err(E::from_borrow(err)),
        }
    }
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
// FIXME: This trait should be removed IMO or at least private
pub trait BorrowedOwnerEncode: ReprC<CType: Copy> + Sized {
    type ReferenceStore: Store + Default;
    fn encode_borrowed(
        self,
        store: &mut BorrowedEncodeStore<Self, Self::ReferenceStore>,
    ) -> Borrowed<Self::CType>;
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
    ) -> Borrowed<Self::CType> {
        let value = store.value.insert(self);
        // SAFETY: The returned borrowed carrier permits only read-only foreign access.
        let encoded = unsafe { value.soft_encode_unchecked(&mut store.reference) };
        // SAFETY: The encoded carrier is initialized and Copy, with its owner retained.
        Borrowed::new(unsafe { crate::reference::MutRefCarrier::as_ptr(&encoded).read() })
    }
}

#[cfg(all(test, feature = "alloc", feature = "derive"))]
mod tests {
    use alloc::{boxed::Box, string::String, vec, vec::Vec};
    use core::{
        cell::UnsafeCell,
        mem::MaybeUninit,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use rust_spec::RustSpec;
    use static_assertions::{assert_impl_all, assert_type_eq_all};

    use super::{Borrow, Borrowed, BorrowedEncodeStore};
    use crate::{
        ReprC,
        boxed::CBox,
        primitives::CBool,
        reference::{CRefMut, CSoftRef, CSoftRefMut},
        stored::{
            EncodeMove, EncodeMoveUnchecked, SoftEncodeMut, SoftEncodeMutUnchecked, SoftEncodeRef,
            SoftEncodeRefUnchecked, Store,
        },
    };
    #[test]
    fn retained_owners_expose_borrowed_views_without_changing_the_carrier() {
        use co3::{borrow::Borrowed, stored::Store};
        assert_type_eq_all!(<Box<u32> as Borrow>::Borrowed, Borrowed<Box<u32>>);
        assert_type_eq_all!(<Vec<u32> as Borrow>::Borrowed, Borrowed<Vec<u32>>);
        assert_type_eq_all!(
            <co3::vec::Vec<u32> as Borrow>::Borrowed,
            Borrowed<co3::vec::Vec<u32>>
        );
        assert_type_eq_all!(
            <alloc::ffi::CString as Borrow>::Borrowed,
            Borrowed<alloc::ffi::CString>
        );
        assert_type_eq_all!(<String as Borrow>::Borrowed, Borrowed<String>);
        let value = Box::new(42_u32);
        let original = core::ptr::from_ref(&*value);
        let mut store = Default::default();
        let encoded = co3::soft_encode(value.borrow(), &mut store);
        assert_eq!(
            Borrowed::into_inner(encoded).as_ptr().cast_const(),
            original
        );
        let mut decode_store = Default::default();
        let decoded =
            unsafe { co3::soft_decode::<Borrowed<Box<u32>>>(encoded, &mut decode_store) }.unwrap();
        let cloned = Borrowed::into_inner(decoded);
        assert_eq!(*cloned, 42);
        assert_ne!(core::ptr::from_ref(&*cloned), original);
        assert_eq!(decode_store.sync(), Some(()));
        assert_eq!(store.sync(), Some(()));
        assert_eq!(
            String::from_borrow(String::from("borrowed").borrow()),
            "borrowed"
        );
        assert_eq!(Vec::<u32>::from_borrow(vec![1_u32, 2].borrow()), [1, 2]);
    }

    static DROPS: AtomicUsize = AtomicUsize::new(0);

    #[derive(Clone, RustSpec, ReprC)]
    #[repr(transparent)]
    #[rust_spec(custom_drop)]
    struct RetainedDrop(u32);

    impl Drop for RetainedDrop {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn custom_drop_keeps_the_original_alive_and_clones_on_restore() {
        use co3::{borrow::Borrowed, stored::Store};
        assert_type_eq_all!(<RetainedDrop as Borrow>::Borrowed, Borrowed<RetainedDrop>);
        let mut store = Default::default();
        let encoded = co3::soft_encode(RetainedDrop(7).borrow(), &mut store);
        let mut decode_store = Default::default();
        let cloned = Borrowed::into_inner(
            unsafe { co3::soft_decode::<Borrowed<RetainedDrop>>(encoded, &mut decode_store) }
                .unwrap(),
        );
        assert_eq!(cloned.0, 7);
        assert_eq!(DROPS.load(Ordering::Relaxed), 0);
        drop(cloned);
        assert_eq!(DROPS.load(Ordering::Relaxed), 1);
        assert_eq!(decode_store.sync(), Some(()));
        assert_eq!(store.sync(), Some(()));
        assert_eq!(DROPS.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn borrowed_encode_store_retains_its_owner() {
        use co3::stored::EmptyStore;
        static_assertions::assert_not_impl_any!(BorrowedEncodeStore<Box<u8>, ()>: EmptyStore);
    }

    #[test]
    fn owned_containers_preserve_their_type_and_allocation_when_borrowed() {
        fn restore<T: Borrow<Borrowed = Borrowed<T>>>(value: T) -> T {
            T::from_borrow(value.borrow())
        }

        let boxed = Box::new(7_u8);
        let original = core::ptr::from_ref(&*boxed);
        assert_eq!(core::ptr::from_ref(&*restore(boxed)), original);

        let values = vec![1_u8, 2];
        let original = values.as_ptr();
        let values = restore(values);
        assert_eq!(values.as_ptr(), original);
        let values = restore(co3::vec::Vec::from(values));
        assert_eq!(values.as_ptr(), original);

        let text = String::from("owned");
        let original = text.as_ptr();
        let restored = restore(text);
        assert_eq!(restored.as_ptr(), original);

        let text = alloc::ffi::CString::new("owned").unwrap();
        let original = text.as_ptr();
        let restored = restore(text);
        assert_eq!(restored.as_ptr(), original);
    }

    #[test]
    fn borrowed_mut_reference_keeps_its_marker() {
        let mut value = 7u8;
        let borrowed: Borrowed<&mut u8> = (&mut value).borrow();
        let mut store = ();
        let encoded = co3::soft_encode(borrowed, &mut store);
        assert_eq!(unsafe { *Borrowed::into_inner(encoded).as_ptr() }, 7);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(value, 7);
    }

    #[test]
    fn borrowed_box_keeps_owner_until_sync() {
        let value = Box::new(9u8);
        let borrowed: Borrowed<Box<u8>> = value.borrow();
        let mut store = Default::default();
        let encoded = co3::soft_encode(borrowed, &mut store);
        assert_eq!(unsafe { *Borrowed::into_inner(encoded).as_ptr() }, 9);
        assert_eq!(store.sync(), Some(()));
    }

    #[test]
    fn shared_interior_reference_encodes_directly() {
        let value = core::cell::UnsafeCell::new(7u8);
        let borrowed = (&value).borrow();
        let encoded = co3::soft_encode(borrowed, &mut Default::default());
        assert_eq!(Borrowed::into_inner(encoded).as_ptr(), value.get().cast());
    }

    #[test]
    fn owned_decode_clones_without_taking_ownership() {
        let value = Box::new(11u8);
        let original = core::ptr::from_ref(&*value);
        let mut encode_store = Default::default();
        let carrier = co3::soft_encode(value.borrow(), &mut encode_store);
        let mut decode_store = Default::default();
        let decoded =
            unsafe { co3::soft_decode::<Borrowed<Box<u8>>>(carrier, &mut decode_store) }.unwrap();
        let cloned = Borrowed::into_inner(decoded);
        assert_eq!(*cloned, 11);
        assert_ne!(core::ptr::from_ref(&*cloned), original);
        assert_eq!(decode_store.sync(), Some(()));
        assert_eq!(encode_store.sync(), Some(()));
        assert_eq!(*cloned, 11);
    }

    #[test]
    fn shared_references_with_reachable_interior_are_wrapped() {
        use core::cell::UnsafeCell;
        let plain = 5u8;
        let identity: &u8 = (&plain).borrow();
        assert!(core::ptr::eq(identity, &plain));

        let cell = UnsafeCell::new(7u8);
        let inner = &cell;
        let wrapped: Borrowed<&&UnsafeCell<u8>> = (&inner).borrow();
        let encoded = co3::soft_encode(wrapped, &mut Default::default());
        let mut store = ();
        let decoded =
            unsafe { co3::soft_decode::<Borrowed<&&UnsafeCell<u8>>>(encoded, &mut store) }.unwrap();
        assert!(core::ptr::eq(
            core::ptr::from_ref(Borrowed::into_inner(decoded)),
            &inner
        ));

        let cell = UnsafeCell::new(false);
        let inner = &cell;
        let wrapped: Borrowed<&&UnsafeCell<bool>> = (&inner).borrow();
        assert!(core::ptr::eq(
            core::ptr::from_ref(Borrowed::into_inner(wrapped)),
            &inner
        ));
    }

    #[test]
    fn borrowed_vector_keeps_owner_and_decodes_a_clone() {
        use co3::{
            borrow::{Borrow, Borrowed},
            stored::Store,
        };
        let value = vec![7u32, 8];
        let original = value.as_ptr();
        let mut encode_store = Default::default();
        let carrier = co3::soft_encode(co3::vec::Vec::from(value).borrow(), &mut encode_store);
        let mut decode_store = Default::default();
        let cloned =
            unsafe { co3::soft_decode::<Borrowed<co3::vec::Vec<u32>>>(carrier, &mut decode_store) }
                .unwrap();
        let cloned = Borrowed::into_inner(cloned);
        assert_eq!(&*cloned, &[7, 8]);
        assert_ne!(cloned.as_ptr(), original);
        assert_eq!(decode_store.sync(), Some(()));
        assert_eq!(encode_store.sync(), Some(()));
        assert_eq!(&*cloned, &[7, 8]);
    }

    #[test]
    fn mutable_reference_decode_validates() {
        let mut carrier = CBool(1);
        let mut store = ();
        let decoded = unsafe {
            co3::soft_decode::<Borrowed<&mut bool>>(
                CRefMut::from_mut(&mut carrier).into(),
                &mut store,
            )
        }
        .unwrap();
        assert!(*Borrowed::into_inner(decoded));
        assert_eq!(store.sync(), Some(()));
        let mut invalid = CBool(2);
        let mut store = ();
        assert!(
            unsafe {
                co3::soft_decode::<Borrowed<&mut bool>>(
                    CRefMut::from_mut(&mut invalid).into(),
                    &mut store,
                )
            }
            .is_none()
        );
    }

    #[test]
    fn mutable_reference_unchecked_decode_preserves_pointer() {
        let mut value = 42u32;
        let pointer = core::ptr::from_mut(&mut value);
        let mut store = ();
        let decoded = unsafe {
            co3::soft_decode_unchecked::<Borrowed<&mut u32>>(
                co3::reference::CRefMut::from_mut(&mut value).into(),
                &mut store,
            )
        };
        assert_eq!(core::ptr::from_mut(Borrowed::into_inner(decoded)), pointer);
    }

    #[test]
    fn shared_interior_reference_decodes_directly() {
        let value = UnsafeCell::new(7u8);
        let mut store = ();
        let decoded = unsafe {
            co3::soft_decode::<Borrowed<&UnsafeCell<u8>>>(
                co3::reference::CRefMut::from_raw(core::ptr::from_ref(&value).cast_mut()).into(),
                &mut store,
            )
        }
        .unwrap();
        assert!(core::ptr::eq(
            core::ptr::from_ref(Borrowed::into_inner(decoded)),
            &value
        ));
    }

    static CLONES: AtomicUsize = AtomicUsize::new(0);
    static SYNCS: AtomicUsize = AtomicUsize::new(0);

    #[derive(co3::rust_spec::RustSpec)]
    #[rust_spec(custom_drop)]
    struct Probe(u8);

    impl Drop for Probe {
        fn drop(&mut self) {}
    }

    impl Clone for Probe {
        fn clone(&self) -> Self {
            CLONES.fetch_add(1, Ordering::Relaxed);
            Self(self.0)
        }
    }

    impl ReprC for Probe {
        type CType = u8;
    }

    #[derive(Default)]
    struct ProbeStore;

    impl Store for ProbeStore {
        fn sync(self) -> Option<()> {
            SYNCS.fetch_add(1, Ordering::Relaxed);
            Some(())
        }
    }

    unsafe impl EncodeMoveUnchecked for Probe {
        type SoftStore = ProbeStore;

        unsafe fn soft_encode_unchecked<'itm>(self, _: &'itm mut ProbeStore) -> u8
        where
            Self: 'itm,
        {
            self.0
        }
    }

    unsafe impl EncodeMove for Probe {
        type Store = ProbeStore;

        fn soft_encode<'itm>(self, _: &'itm mut ProbeStore) -> u8
        where
            Self: 'itm,
        {
            self.0
        }
    }

    // Borrow the actual byte: no snapshot, clone, or deferred writeback is needed.
    unsafe impl<'a> SoftEncodeRefUnchecked<'a> for Probe {
        type SoftStore = ProbeStore;

        unsafe fn soft_encode_ref_unchecked<'itm>(
            &'a self,
            _: &'itm mut ProbeStore,
        ) -> <&'a Self as ReprC>::CType
        where
            'a: 'itm,
        {
            CSoftRef::from_ref(&self.0)
        }
    }

    unsafe impl<'a> SoftEncodeRef<'a> for Probe {
        type Store = ProbeStore;

        fn soft_encode_ref<'itm>(
            &'a self,
            _: &'itm mut ProbeStore,
        ) -> co3::encode_repr::Encoded<&'a Self>
        where
            'a: 'itm,
        {
            CSoftRef::from_ref(&self.0)
        }
    }

    unsafe impl<'a> SoftEncodeMutUnchecked<'a> for Probe {
        type SoftStore = ProbeStore;

        unsafe fn soft_encode_mut_unchecked<'itm>(
            &'a mut self,
            _: &'itm mut ProbeStore,
        ) -> <&'a mut Self as ReprC>::CType
        where
            'a: 'itm,
        {
            CSoftRefMut::from_mut(&mut self.0)
        }
    }

    unsafe impl<'a> SoftEncodeMut<'a> for Probe {
        type Store = ProbeStore;

        fn soft_encode_mut<'itm>(
            &'a mut self,
            _: &'itm mut ProbeStore,
        ) -> co3::encode_repr::Encoded<&'a mut Self>
        where
            'a: 'itm,
        {
            CSoftRefMut::from_mut(&mut self.0)
        }
    }

    #[test]
    fn reference_and_borrowed_encoding_avoid_clones_and_sync_nested_state() {
        let mut store = Default::default();
        let value = Probe(7);
        let encoded = co3::soft_encode(&value, &mut store);
        assert_eq!(unsafe { *encoded.as_ptr() }, 7);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let value = Probe(8);
        let encoded = unsafe { co3::soft_encode_unchecked(&value, &mut store) };
        assert_eq!(unsafe { *encoded.as_ptr() }, 8);
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let borrowed: Borrowed<Probe> = Probe(9).into();
        assert_eq!(
            Borrowed::into_inner(co3::soft_encode(borrowed, &mut store)),
            9
        );
        assert_eq!(store.sync(), Some(()));

        let mut store = Default::default();
        let borrowed: Borrowed<Probe> = Probe(10).into();
        let encoded = unsafe { co3::soft_encode_unchecked(borrowed, &mut store) };
        assert_eq!(Borrowed::into_inner(encoded), 10);
        assert_eq!(store.sync(), Some(()));

        let mut value = Probe(11);
        let mut store = Default::default();
        let encoded = co3::soft_encode(&mut value, &mut store);
        unsafe { *encoded.as_ptr() = 12 };
        assert_eq!(store.sync(), Some(()));
        assert_eq!(value.0, 12);

        let mut store = Default::default();
        let encoded = unsafe { co3::soft_encode_unchecked(&mut value, &mut store) };
        unsafe { *encoded.as_ptr() = 13 };
        assert_eq!(store.sync(), Some(()));
        assert_eq!(value.0, 13);

        assert_eq!(CLONES.load(Ordering::Relaxed), 0);
        assert_eq!(SYNCS.load(Ordering::Relaxed), 6);
    }

    #[derive(RustSpec, ReprC)]
    #[repr(transparent)]
    #[rust_spec(custom_drop)]
    struct Custom(CBox<u8>);

    impl Drop for Custom {
        fn drop(&mut self) {}
    }

    #[test]
    fn custom_drop_borrows_whole_while_carriers_borrow_as_self() {
        static_assertions::assert_type_eq_all!(<CBox<u8> as Borrow>::Borrowed, CBox<u8>);
        static_assertions::assert_type_eq_all!(<CCustom as Borrow>::Borrowed, CCustom);
        static_assertions::assert_type_eq_all!(<Custom as Borrow>::Borrowed, Borrowed<Custom>);
    }

    #[test]
    fn pointer_carriers_borrow_without_borrowing_the_pointee() {
        use core::cell::{Cell, UnsafeCell};

        use co3::{boxed::CBox, vec::CVec, wide::CWideBox};

        fn identity<C: Borrow<Borrowed = C>>(carrier: C) -> C {
            C::from_borrow(carrier.borrow())
        }

        assert!(identity(CBox::<UnsafeCell<u8>>::null()).as_ptr().is_null());
        assert!(identity(CBox::<UnsafeCell<u8>>::null()).as_ptr().is_null());
        assert!(
            identity(CWideBox::<[UnsafeCell<u8>]>::null())
                .as_ptr()
                .is_null()
        );
        assert!(
            identity(CWideBox::<[UnsafeCell<u8>]>::null())
                .as_ptr()
                .is_null()
        );
        assert!(identity(CVec::<UnsafeCell<u8>>::null()).as_ptr().is_null());
        assert!(identity(CBox::<Borrowed<u8>>::null()).as_ptr().is_null());
        assert!(identity(CVec::<Borrowed<u8>>::null()).as_ptr().is_null());
        assert_eq!(identity(Cell::new(7_u8)).into_inner(), 7);
    }

    // A downstream field can support representation without structural borrowing.
    #[derive(RustSpec)]
    struct Field(u8);
    impl ReprC for Field {
        type CType = u8;
    }

    #[derive(RustSpec, ReprC)]
    #[rust_spec(custom_drop)]
    struct Owner<T>(T);
    impl<T> Drop for Owner<T> {
        fn drop(&mut self) {}
    }

    #[derive(RustSpec, ReprC)]
    #[rust_spec(custom_drop)]
    enum EnumOwner<T> {
        Value(T),
        Empty,
    }
    impl<T> Drop for EnumOwner<T> {
        fn drop(&mut self) {}
    }

    #[derive(RustSpec, ReprC)]
    struct Automatic(Box<u8>);

    #[test]
    fn custom_drop_owners_keep_their_borrowing_contract() {
        assert_type_eq_all!(<Owner<Field> as Borrow>::Borrowed, Borrowed<Owner<Field>>);
        assert_type_eq_all!(
            <EnumOwner<Field> as Borrow>::Borrowed,
            Borrowed<EnumOwner<Field>>
        );
        let restored = Owner::from_borrow(Owner(Field(7)).borrow());
        assert_eq!(restored.0.0, 7);
    }

    #[test]
    fn automatic_drop_still_borrows_structurally() {
        let view: AutomaticView = Automatic(Box::new(9)).borrow();
        let restored = Automatic::from_borrow(view);
        assert_eq!(*restored.0, 9);
    }

    #[derive(RustSpec, ReprC)]
    struct GenericTail<T: ?Sized>(u8, T);

    #[test]
    fn generic_tail_borrowing_requires_a_sized_tail() {
        assert_type_eq_all!(<GenericTail<u8> as Borrow>::Borrowed, GenericTail<u8>);
        static_assertions::assert_not_impl_any!(GenericTail<[u8]>: Borrow);
        let view = GenericTail(3, Box::new(7)).borrow();
        let restored: GenericTail<Box<u8>> = GenericTail::from_borrow(view);
        assert_eq!((restored.0, *restored.1), (3, 7));
    }

    #[derive(RustSpec, ReprC)]
    #[repr(u8)]
    enum Tagged {
        Value(Box<u32>) = 1,
        Empty,
    }

    #[derive(RustSpec, ReprC)]
    #[repr(C, u8)]
    enum TaggedPayload {
        Value(Box<u32>) = 1,
        Empty,
    }

    #[test]
    fn borrowed_enum_companions_do_not_reborrow_fields() {
        let value = Tagged::Value(Box::new(7));
        let view = value.borrow();
        assert!(matches!(Tagged::from_borrow(view), Tagged::Value(value) if *value == 7));
        let value = TaggedPayload::Value(Box::new(9));
        let view = value.borrow();
        assert!(
            matches!(TaggedPayload::from_borrow(view), TaggedPayload::Value(value) if *value == 9)
        );
    }

    #[test]
    fn initialized_carrier_borrow_preserves_its_pointer() {
        let owner = CBox::from_box(Box::new(42_u32));
        let pointer = owner.as_ptr();
        let borrowed: MaybeUninit<CBox<u32>> = MaybeUninit::new(owner).borrow();

        // The fixture initialized the value; borrowing must not consume its allocation.
        assert_eq!(unsafe { borrowed.assume_init() }.as_ptr(), pointer);
        assert_eq!(*unsafe { owner.into_box() }.unwrap(), 42);
    }

    #[test]
    fn uninitialized_carrier_can_be_borrowed_without_initializing_it() {
        let borrowed: MaybeUninit<CBox<u32>> = MaybeUninit::zeroed().borrow();
        let _: MaybeUninit<CBox<u32>> = Borrow::from_borrow(borrowed);
    }

    static FIELDLESS_DROPS: AtomicUsize = AtomicUsize::new(0);

    #[derive(RustSpec, ReprC)]
    #[repr(C)]
    struct NoDropValue(u32);

    #[test]
    fn no_drop_borrows_itself_without_owner() {
        use co3::borrow::Borrow;
        static_assertions::assert_impl_all!(NoDropValue:
            co3::stored::SoftEncodeMut<'static, Store = ()>,
            co3::stored::SoftEncodeMutUnchecked<'static, SoftStore = ()>);

        static_assertions::assert_type_eq_all!(<CNoDropValue as Borrow>::Borrowed, CNoDropValue);

        let borrowed: NoDropValue = Borrow::borrow(NoDropValue(5));
        let restored = <NoDropValue as Borrow>::from_borrow(borrowed);
        assert_eq!(restored.0, 5);
    }

    #[derive(RustSpec, ReprC)]
    #[repr(C)]
    struct InheritedDrop(Box<u32>);

    #[test]
    fn inherited_drop_borrows_fields() {
        use co3::borrow::Borrow;

        static_assertions::assert_type_eq_all!(
            <CInheritedDrop as Borrow>::Borrowed,
            CInheritedDrop
        );

        let borrowed = Borrow::borrow(InheritedDrop(Box::new(7)));
        let borrowed: InheritedDropView = borrowed;
        let restored = <InheritedDrop as Borrow>::from_borrow(borrowed);
        assert_eq!(*restored.0, 7);
    }

    #[derive(Clone, RustSpec, ReprC)]
    #[repr(C)]
    #[rust_spec(custom_drop)]
    struct CustomAndInheritedDrop(Box<u32>);

    impl Drop for CustomAndInheritedDrop {
        fn drop(&mut self) {}
    }

    #[test]
    fn custom_and_inherited_drop_borrows_whole_value() {
        use co3::borrow::Borrow;

        let borrowed = Borrow::borrow(CustomAndInheritedDrop(Box::new(9)));
        let borrowed: co3::borrow::Borrowed<CustomAndInheritedDrop> = borrowed;
        let restored = <CustomAndInheritedDrop as Borrow>::from_borrow(borrowed);
        assert_eq!(*restored.0, 9);
    }

    #[test]
    fn custom_drop_companion_borrows_as_self() {
        static_assertions::assert_type_eq_all!(
            <CCustomAndInheritedDrop as co3::borrow::Borrow>::Borrowed,
            CCustomAndInheritedDrop
        );
        assert_impl_all!(CBox<CCustomAndInheritedDrop>: co3::borrow::Borrow);
    }

    #[derive(RustSpec, ReprC)]
    #[repr(C, u8)]
    enum InheritedDropEnum {
        #[repr_c(allow_zero_tag)]
        Value(Box<u32>),
        Empty,
    }

    #[test]
    fn inherited_drop_enum_borrows_fields() {
        use co3::borrow::Borrow;

        let borrowed = Borrow::borrow(InheritedDropEnum::Value(Box::new(11)));
        let borrowed: InheritedDropEnumView = borrowed;
        let restored = <InheritedDropEnum as Borrow>::from_borrow(borrowed);
        assert!(matches!(restored, InheritedDropEnum::Value(value) if *value == 11));
    }

    #[derive(Clone, RustSpec, ReprC)]
    #[repr(u8)]
    #[rust_spec(custom_drop)]
    enum ExplicitFieldlessDrop {
        First = 2,
        Second = 5,
    }

    impl Drop for ExplicitFieldlessDrop {
        fn drop(&mut self) {
            FIELDLESS_DROPS.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[derive(Clone, RustSpec, ReprC)]
    #[rust_spec(custom_drop)]
    enum InferredFieldlessDrop {
        First = 2,
        Second,
    }

    impl Drop for InferredFieldlessDrop {
        fn drop(&mut self) {
            FIELDLESS_DROPS.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn fieldless_enums_with_drop_borrow() {
        use co3::borrow::Borrow;

        let before = FIELDLESS_DROPS.load(Ordering::Relaxed);

        let borrowed = Borrow::borrow(ExplicitFieldlessDrop::Second);
        let cloned = <ExplicitFieldlessDrop as Borrow>::from_borrow(borrowed);
        assert!(matches!(cloned, ExplicitFieldlessDrop::Second));
        drop(cloned);
        assert_eq!(FIELDLESS_DROPS.load(Ordering::Relaxed) - before, 1);
    }
}
