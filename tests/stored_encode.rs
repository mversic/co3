use core::{
    cell::UnsafeCell,
    mem::{ManuallyDrop, MaybeUninit},
};

use co3::{
    Decode, Encode,
    option::ReprCOption,
    primitives::CBool,
    stored::{DirectEncodeOwned, EmptyStore, EncodeOwned, Store},
    transmute::CheckedTransmute,
};
use static_assertions::{assert_impl_all, assert_not_impl_any};
// FIXME: Restore these fixtures with the disabled fast-path tests below.
/*
#[cfg(feature = "alloc")]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(feature = "alloc")]
static RAW_PROBE_CALLS: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "alloc")]
#[derive(co3::rust_spec::RustSpec)]
#[repr(transparent)]
struct RawProbe(u8);

#[cfg(feature = "alloc")]
impl co3::ReprC for RawProbe {
    type CType = u8;
}

#[cfg(feature = "alloc")]
unsafe impl co3::stored::EncodeOwned for RawProbe {
    type Store = ();

    fn soft_encode<'itm>(self, (): &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        RAW_PROBE_CALLS.fetch_add(1, Ordering::SeqCst);
        self.0
    }
}

#[cfg(feature = "alloc")]
unsafe impl DirectEncodeOwned for RawProbe {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
        self.0
    }
}

#[cfg(feature = "alloc")]
unsafe impl CheckedTransmute for RawProbe {
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}

#[cfg(feature = "alloc")]
impl co3::DirectEncode for RawProbe {}

#[cfg(feature = "alloc")]
impl Encode for RawProbe {}
*/

struct UncheckedOnly(u8);

impl co3::ReprC for UncheckedOnly {
    type CType = u8;
}

unsafe impl co3::stored::EncodeOwned for UncheckedOnly {
    type Store = ();

    fn soft_encode<'itm>(self, (): &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        self.0.wrapping_add(1)
    }
}

unsafe impl co3::stored::DirectEncodeOwned for UncheckedOnly {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked(
        self,
        _store: &mut <Self as co3::stored::DirectEncodeOwned>::SoftStore,
    ) -> Self::CType {
        co3::stored::EncodeOwned::soft_encode(self, &mut ())
    }
}

impl Encode for UncheckedOnly {}
impl co3::DirectEncode for UncheckedOnly {}

#[test]
fn direct_encode_does_not_require_checked_transmute() {
    assert_not_impl_any!(UncheckedOnly: CheckedTransmute);
    assert_eq!(unsafe { co3::encode_unchecked(UncheckedOnly(7)) }, 8);
}

#[derive(Default)]
struct DirectState {
    calls: usize,
}

impl Store for DirectState {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

#[derive(co3::rust_spec::RustSpec)]
#[repr(transparent)]
struct StoredDirect(u8);

impl co3::ReprC for StoredDirect {
    type CType = u8;
}

unsafe impl EncodeOwned for StoredDirect {
    type Store = ();

    fn soft_encode<'itm>(self, _: &mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        self.0
    }
}

unsafe impl DirectEncodeOwned for StoredDirect {
    type SoftStore = DirectState;

    unsafe fn soft_encode_unchecked(
        self,
        store: &mut <Self as DirectEncodeOwned>::SoftStore,
    ) -> Self::CType {
        store.calls += 1;
        self.0
    }
}

impl Encode for StoredDirect {}
impl co3::DirectEncode for StoredDirect {}

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(C)]
struct StoredDirectRecord {
    value: StoredDirect,
    tag: u8,
}

#[test]
fn direct_store_is_independent_and_propagates_through_derives() {
    let mut store = Default::default();
    let encoded = unsafe {
        co3::soft_encode_unchecked(
            StoredDirectRecord {
                value: StoredDirect(7),
                tag: 3,
            },
            &mut store,
        )
    };
    assert_eq!(encoded.value, 7);
    assert_eq!(encoded.tag, 3);
    assert_eq!(store.0.calls, 1);
    store.sync().unwrap();

    let mut option_store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(Some(StoredDirect(8)), &mut option_store) };
    assert_eq!(encoded.into_option(), Some(Some(8)));
    assert_eq!(option_store.as_ref().unwrap().calls, 1);
    let encoded = unsafe { co3::soft_encode_unchecked(None::<StoredDirect>, &mut option_store) };
    assert_eq!(encoded.into_option(), Some(None));
    assert!(option_store.is_none());

    let mut tuple_store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked((StoredDirect(9), 4_u8), &mut tuple_store) };
    assert_eq!(encoded.0, 9);
    assert_eq!(tuple_store.0.calls, 1);

    let mut array_store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked([StoredDirect(10)], &mut array_store) };
    assert_eq!(encoded, [10]);
    array_store.sync().unwrap();

    fn empty<T: EmptyStore>() {}
    empty::<<&mut bool as DirectEncodeOwned>::SoftStore>();
    empty::<<Option<&mut bool> as DirectEncodeOwned>::SoftStore>();
    empty::<<TransparentMut<'static> as DirectEncodeOwned>::SoftStore>();
    assert_not_impl_any!(<TransparentMut<'static> as EncodeOwned>::Store: EmptyStore);
}

#[test]
fn unchecked_encoding_can_require_writeback_storage() {
    let mut value = Some(8_u8);
    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(&mut value, &mut store) };
    unsafe { *encoded.as_ptr() = ReprCOption::Some(42) };
    store.sync().unwrap();
    assert_eq!(value, Some(42));
}

// FIXME: Generic Box encoding cannot select its `EmptyStore` branch with `impls::impls!`;
// checked Vec encoding also needs an allocation-reuse path.
/*
#[cfg(feature = "alloc")]
#[test]
fn stable_wrappers_use_fast_paths_without_inner_encoder_calls() {
    RAW_PROBE_CALLS.store(0, Ordering::SeqCst);

    let boxed = Box::new(RawProbe(7));
    let ptr = (&*boxed as *const RawProbe).cast_mut().cast::<u8>();
    let mut store = Default::default();
    let encoded = co3::soft_encode(boxed, &mut store);
    assert_eq!(encoded.as_ptr(), ptr);
    assert_eq!(unsafe { *encoded.as_ptr() }, 7);
    drop(unsafe { encoded.into_box().unwrap() });
    store.sync().unwrap();

    let mut store = Default::default();
    let encoded = co3::soft_encode(vec![RawProbe(1), RawProbe(2)], &mut store);
    assert_eq!(unsafe { *encoded.as_ptr() }, 1);
    drop(unsafe { encoded.into_box().unwrap() });
    store.sync().unwrap();

    let mut store = Default::default();
    let encoded = co3::soft_encode(ManuallyDrop::new(RawProbe(3)), &mut store);
    assert_eq!(encoded, 3);
    store.sync().unwrap();

    assert_eq!(RAW_PROBE_CALLS.load(Ordering::SeqCst), 0);
}
*/

#[test]
fn shared_interior_and_mutable_non_robust_refs_have_direct_paths() {
    assert_impl_all!(&UnsafeCell<u8>: DirectEncodeOwned);
    assert_impl_all!(&[UnsafeCell<u8>]: DirectEncodeOwned);
    assert_impl_all!(&mut bool: DirectEncodeOwned);
    assert_impl_all!(TransparentMut<'static>: DirectEncodeOwned);
    assert_impl_all!(&mut u8: DirectEncodeOwned);
    assert_impl_all!(&mut UnsafeCell<u8>: DirectEncodeOwned);
}

#[test]
fn unchecked_encode_exposes_the_original_mutable_bool() {
    let mut value = false;
    let original = core::ptr::from_mut(&mut value).cast::<CBool>();
    let encoded = unsafe { co3::encode_unchecked(&mut value) };
    assert_eq!(encoded.as_ptr(), original);
    unsafe { *encoded.as_ptr() = CBool::TRUE };
    assert!(value);

    let encoded = unsafe { co3::encode_unchecked(TransparentMut(&mut value)) };
    assert_eq!(encoded.as_ptr(), original);
}

#[cfg(feature = "alloc")]
#[test]
fn unchecked_encode_reuses_a_boxed_mutable_bool_reference() {
    let mut value = false;
    let boxed = Box::new(&mut value);
    let original = core::ptr::from_ref(&*boxed).cast_mut().cast();
    let encoded = unsafe { co3::encode_unchecked(boxed) };
    assert_eq!(encoded.as_ptr(), original);
    unsafe { *(*encoded.as_ptr()).as_ptr() = CBool::TRUE };
    drop(unsafe { encoded.into_box().unwrap() });
    assert!(value);
}

#[cfg(feature = "alloc")]
#[test]
fn unchecked_encode_reuses_a_boxed_slice_of_mutable_bool_references() {
    let mut value = false;
    let boxed = vec![&mut value].into_boxed_slice();
    let original = boxed.as_ptr().cast_mut().cast();
    let encoded = unsafe { co3::encode_unchecked(boxed) };
    assert_eq!(encoded.as_ptr(), original);
    unsafe { *(*encoded.as_ptr()).as_ptr() = CBool::TRUE };
    drop(unsafe { encoded.into_box().unwrap() });
    assert!(value);
}

#[test]
fn unchecked_encode_propagates_through_wrappers() {
    let mut value = false;
    let original = core::ptr::from_mut(&mut value).cast::<CBool>();

    let optional = unsafe { co3::encode_unchecked(Some(&mut value)) };
    assert_eq!(optional.as_ptr(), original);

    let result = unsafe { co3::encode_unchecked(Ok::<&mut bool, ()>(&mut value)) };
    assert_eq!(result.as_ptr(), original);

    let record = unsafe {
        co3::encode_unchecked(RecordMut {
            value: &mut value,
            tag: 7,
        })
    };
    assert_eq!(record.value.as_ptr(), original);
    unsafe { *record.value.as_ptr() = CBool::TRUE };
    assert!(value);

    let tuple = unsafe { co3::encode_unchecked((&mut value, 7_u8)) };
    assert_eq!(tuple.0.as_ptr(), original);

    let array = unsafe { co3::encode_unchecked([&mut value]) };
    assert_eq!(array[0].as_ptr(), original);
}

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(transparent)]
#[repr_c(transparent)]
struct TransparentMut<'a>(&'a mut bool);

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(C)]
struct RecordMut<'a> {
    value: &'a mut bool,
    tag: u8,
}

// FIXME: Restore these fixtures with the disabled fast-path test below.
/*
#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(C)]
struct RawRecord {
    value: u8,
    tag: u16,
}

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(transparent)]
struct RawTransparent(u8);

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
struct RustRecord(u8, u16);

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(C, u8)]
enum RawVariant {
    Value(u8),
    Empty,
}
*/

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
enum VariantMut<'a> {
    Value(&'a mut bool),
    Empty,
}

// FIXME: Generic Box encoding cannot select its `EmptyStore` branch with `impls::impls!`;
// checked Vec encoding also needs an allocation-reuse path.
/*
#[cfg(feature = "alloc")]
#[test]
fn derived_representations_preserve_stable_fast_paths() {
    assert_impl_all!(RawRecord: DirectEncodeOwned);
    assert_impl_all!(RawTransparent: DirectEncodeOwned);
    assert_impl_all!(RawVariant: DirectEncodeOwned);
    assert_impl_all!(RustRecord: DirectEncodeOwned);
    assert_impl_all!(RecordMut<'static>: DirectEncodeOwned);
    assert_impl_all!(VariantMut<'static>: DirectEncodeOwned);

    let value = Box::new(RawRecord { value: 3, tag: 5 });
    let ptr = (&*value as *const RawRecord).cast_mut().cast();
    let mut store = Default::default();
    let encoded = co3::soft_encode(value, &mut store);
    assert_eq!(encoded.as_ptr(), ptr);
    drop(unsafe { encoded.into_box().unwrap() });
    store.sync().unwrap();

    let values = vec![RawRecord { value: 7, tag: 9 }];
    let ptr = values.as_ptr().cast_mut().cast();
    let mut store = Default::default();
    let encoded = co3::soft_encode(values, &mut store);
    assert_eq!(encoded.as_ptr(), ptr);
    drop(unsafe { encoded.into_box().unwrap() });
    store.sync().unwrap();

    let value = Box::new(RawVariant::Value(11));
    let ptr = (&*value as *const RawVariant).cast_mut().cast();
    let mut store = Default::default();
    let encoded = co3::soft_encode(value, &mut store);
    assert_eq!(encoded.as_ptr(), ptr);
    let carrier = unsafe { encoded.into_box().unwrap() };
    let decoded: RawVariant = unsafe { co3::decode(*carrier) }.unwrap();
    assert!(matches!(decoded, RawVariant::Value(11)));
    store.sync().unwrap();
}
*/

#[test]
fn maybe_uninit_rejects_encoding_an_inner_value_with_a_store() {
    assert_impl_all!(MaybeUninit<&mut bool>: CheckedTransmute, Decode<'static>);
    assert_not_impl_any!(MaybeUninit<&mut bool>: Encode);
}

#[test]
fn manually_drop_mut_bool_syncs_its_store() {
    let mut value = false;
    let mut store = Default::default();
    let encoded = co3::soft_encode(ManuallyDrop::new(&mut value), &mut store);
    unsafe { *encoded.as_ptr() = CBool::TRUE };
    store.sync().unwrap();
    assert!(value);
}

#[cfg(feature = "alloc")]
#[test]
fn manually_drop_boxed_mut_bool_syncs_its_store() {
    let mut value = false;
    let mut store = Default::default();
    let encoded = co3::soft_encode(ManuallyDrop::new(Box::new(&mut value)), &mut store);
    unsafe { *(*encoded.as_ptr()).as_ptr() = CBool::TRUE };
    drop(unsafe { ManuallyDrop::into_inner(encoded).into_box().unwrap() });
    store.sync().unwrap();
    assert!(value);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_mut_bool_syncs_element_stores() {
    let mut values = [false, false];
    let mut store = Default::default();
    let encoded = co3::soft_encode(values.iter_mut().collect::<Vec<_>>(), &mut store);
    unsafe { *(*encoded.as_ptr()).as_ptr() = CBool::TRUE };
    drop(unsafe { encoded.into_box().unwrap() });
    store.sync().unwrap();
    assert_eq!(values, [true, false]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_with_empty_element_stores_reuses_allocation() {
    let values = vec![1u8, 2u8];
    let original = values.as_ptr();
    let mut store = Default::default();
    let encoded = co3::soft_encode(values, &mut store);
    assert_eq!(encoded.as_ptr(), original.cast_mut());
    drop(unsafe { encoded.into_box().unwrap() });
    store.sync().unwrap();
}

#[test]
fn optional_mut_bool_syncs_only_present_values() {
    let mut value = false;
    let mut store = Default::default();
    let encoded = co3::soft_encode(Some(&mut value), &mut store);
    unsafe { *encoded.as_ptr() = CBool::TRUE };
    store.sync().unwrap();
    assert!(value);

    let mut store = Default::default();
    let _: <Option<&mut bool> as co3::ReprC>::CType =
        co3::soft_encode(None::<&mut bool>, &mut store);
    store.sync().unwrap();
}

#[cfg(feature = "alloc")]
#[test]
fn vec_of_optional_mut_bool_syncs_present_elements() {
    let mut value = false;
    let mut store = Default::default();
    let encoded = co3::soft_encode(vec![Some(&mut value), None], &mut store);
    unsafe { *(*encoded.as_ptr()).as_ptr() = CBool::TRUE };
    drop(unsafe { encoded.into_box().unwrap() });
    store.sync().unwrap();
    assert!(value);
}

#[test]
fn repr_c_option_without_payload_skips_inner_store() {
    let mut store = Default::default();
    let _: <ReprCOption<&mut bool> as co3::ReprC>::CType =
        co3::soft_encode(ReprCOption::<&mut bool>::None(), &mut store);
    store.sync().unwrap();
}

#[test]
fn niche_result_without_mut_ref_skips_inner_store() {
    let mut store = Default::default();
    let _: <Result<&mut bool, ()> as co3::ReprC>::CType =
        co3::soft_encode(Err::<&mut bool, ()>(()), &mut store);
    store.sync().unwrap();

    let mut store = Default::default();
    let _: <Result<(), &mut bool> as co3::ReprC>::CType =
        co3::soft_encode(Ok::<(), &mut bool>(()), &mut store);
    store.sync().unwrap();
}

#[test]
fn derived_transparent_mut_bool_syncs_its_store() {
    assert_impl_all!(TransparentMut<'static>: CheckedTransmute);
    let mut value = false;
    let mut store = Default::default();
    let encoded = co3::soft_encode(TransparentMut(&mut value), &mut store);
    unsafe { *encoded.as_ptr() = CBool::TRUE };
    store.sync().unwrap();
    assert!(value);
}

#[test]
fn optional_derived_transparent_syncs_only_present_values() {
    let mut value = false;
    let mut store = Default::default();
    let encoded = co3::soft_encode(Some(TransparentMut(&mut value)), &mut store);
    unsafe { *encoded.as_ptr() = CBool::TRUE };
    store.sync().unwrap();
    assert!(value);

    let mut store = Default::default();
    let _encoded = co3::soft_encode(None::<TransparentMut<'static>>, &mut store);
    store.sync().unwrap();
}

#[test]
fn derived_record_mut_bool_syncs_its_field_store() {
    assert_impl_all!(RecordMut<'static>: CheckedTransmute);
    let mut value = false;
    let mut store = Default::default();
    let encoded = co3::soft_encode(
        RecordMut {
            value: &mut value,
            tag: 7,
        },
        &mut store,
    );
    unsafe { *encoded.value.as_ptr() = CBool::TRUE };
    assert_eq!(encoded.tag, 7);
    store.sync().unwrap();
    assert!(value);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_of_derived_records_syncs_field_stores() {
    let mut value = false;
    let mut store = Default::default();
    let encoded = co3::soft_encode(
        vec![RecordMut {
            value: &mut value,
            tag: 7,
        }],
        &mut store,
    );
    unsafe { *(*encoded.as_ptr()).value.as_ptr() = CBool::TRUE };
    drop(unsafe { encoded.into_box().unwrap() });
    store.sync().unwrap();
    assert!(value);
}

#[test]
fn derived_enum_syncs_its_active_variant_store() {
    let mut value = false;
    let mut store = Default::default();
    let _encoded = co3::soft_encode(VariantMut::Value(&mut value), &mut store);
    store.sync().unwrap();

    let mut store = Default::default();
    let _encoded = co3::soft_encode(VariantMut::Empty, &mut store);
    store.sync().unwrap();
}
