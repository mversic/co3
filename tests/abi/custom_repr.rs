use co3::{
    ReprC,
    rust_spec::RustSpec,
    stored::{DecodeOwned, EncodeOwned, Store},
};
use static_assertions::{assert_impl_all, assert_not_impl_any};
use std::cell::{Cell, UnsafeCell};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct FieldHandle(u32);

impl From<u32> for FieldHandle {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct TransparentFieldHandle(u32);

impl From<u32> for TransparentFieldHandle {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
#[repr_c(is_valid = |value| *value > 10)]
struct ValidatedByte(u8);

impl From<u8> for ValidatedByte {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct NonZeroField(core::num::NonZeroU8);

impl From<core::num::NonZeroU8> for NonZeroField {
    fn from(value: core::num::NonZeroU8) -> Self {
        Self(value)
    }
}

#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct AutoDropBox {
    inner: Box<u32>,
}

impl From<Box<u32>> for AutoDropBox {
    fn from(value: Box<u32>) -> Self {
        Self { inner: value }
    }
}

#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct GenericField<T>(T);

impl<T> From<T> for GenericField<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

static DROP_BOX_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(RustSpec, ReprC)]
#[rust_spec(custom_drop)]
struct DropBox(Box<u32>);

impl Drop for DropBox {
    fn drop(&mut self) {
        DROP_BOX_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct FieldSlice<T>([T]);

#[test]
fn field_representation_maps_owned_conversion_and_transmutation() {
    assert_impl_all!(FieldHandle: ReprC<CType = u32>);
    assert_not_impl_any!(FieldHandle: co3::transmute::CheckedTransmute);
    assert_impl_all!(TransparentFieldHandle: co3::transmute::CheckedTransmute);
    assert!(!unsafe {
        <ValidatedByte as co3::transmute::CheckedTransmute>::is_valid(&CValidatedByte::from_fields(
            10,
        ))
    });
    assert!(unsafe {
        <ValidatedByte as co3::transmute::CheckedTransmute>::is_valid(&CValidatedByte::from_fields(
            11,
        ))
    });
    assert!(unsafe { co3::decode::<ValidatedByte>(CValidatedByte::from_fields(10)) }.is_none());
    assert_eq!(
        unsafe { co3::decode::<ValidatedByte>(CValidatedByte::from_fields(11)) }
            .unwrap()
            .0,
        11
    );
    assert!(!unsafe { <NonZeroField as co3::transmute::CheckedTransmute>::is_valid(&0) });
    assert!(unsafe { <NonZeroField as co3::transmute::CheckedTransmute>::is_valid(&1) });
    assert_impl_all!(FieldSlice<u8>: ReprC<CType = [u8]>);
    assert_impl_all!(FieldSlice<u8>: co3::transmute::CheckedTransmute);
    assert_impl_all!(FieldSlice<core::mem::MaybeUninit<u8>>: ReprC<CType = [core::mem::MaybeUninit<u8>]>);
    assert_impl_all!(FieldSlice<core::mem::MaybeUninit<u8>>: co3::transmute::CheckedTransmute);
    assert_eq!(co3::encode(FieldHandle(7)), 7);
    assert_eq!(unsafe { co3::decode::<FieldHandle>(7) }.unwrap().0, 7);
}

#[test]
fn field_representation_preserves_inner_niche() {
    assert_impl_all!(NonZeroField: co3::niche::Niche<CType = u8>);
    assert_eq!(<NonZeroField as co3::niche::Niche>::NICHE, 0_u8);
    assert!(matches!(
        unsafe { co3::decode::<Option<NonZeroField>>(0) },
        Some(None)
    ));
}

#[test]
fn field_representation_transfers_custom_drop_field() {
    let before = DROP_BOX_COUNT.load(Ordering::Relaxed);
    let encoded = co3::encode(DropBox(Box::new(7)));
    assert_eq!(DROP_BOX_COUNT.load(Ordering::Relaxed), before);

    let decoded: DropBox = unsafe { co3::decode(encoded) }.unwrap();
    assert_eq!(*decoded.0, 7);
    drop(decoded);
    assert_eq!(DROP_BOX_COUNT.load(Ordering::Relaxed), before + 1);
}

#[test]
fn field_representation_moves_auto_drop_field() {
    let encoded = co3::encode(AutoDropBox {
        inner: Box::new(11),
    });
    let decoded: AutoDropBox = unsafe { co3::decode(encoded) }.unwrap();
    assert_eq!(*decoded.inner, 11);
}

#[test]
fn field_representation_selects_generic_drop_behavior() {
    assert_eq!(co3::encode(GenericField(13_u32)), 13);
    let encoded = co3::encode(GenericField(Box::new(17_u32)));
    let decoded: GenericField<Box<u32>> = unsafe { co3::decode(encoded) }.unwrap();
    assert_eq!(*decoded.0, 17);
}

#[test]
fn cells_rebuild_transparent_slice_wrappers() {
    use co3::wide::Wide;

    let mut bytes = [1_u8, 2, 3];
    let slice = core::ptr::slice_from_raw_parts_mut(bytes.as_mut_ptr(), bytes.len());
    let unsafe_cell = slice as *mut UnsafeCell<FieldSlice<u8>>;
    let header = <UnsafeCell<FieldSlice<u8>> as Wide>::as_mut_ptr(unsafe_cell);
    let len = <UnsafeCell<FieldSlice<u8>> as Wide>::metadata(unsafe_cell);
    let rebuilt = unsafe { <UnsafeCell<FieldSlice<u8>> as Wide>::from_raw_parts_mut(header, len) };
    assert!(core::ptr::eq(unsafe_cell, rebuilt));
    assert_eq!(len, 3);

    let cell = slice as *mut Cell<FieldSlice<u8>>;
    let header = <Cell<FieldSlice<u8>> as Wide>::as_mut_ptr(cell);
    let rebuilt = unsafe { <Cell<FieldSlice<u8>> as Wide>::from_raw_parts_mut(header, 3) };
    assert!(core::ptr::eq(cell, rebuilt));
}

#[derive(RustSpec, ReprC)]
#[repr_c(as(u32))]
struct Handle(u32);

impl From<Handle> for u32 {
    fn from(value: Handle) -> Self {
        value.0
    }
}

impl TryFrom<u32> for Handle {
    type Error = ();

    fn try_from(raw: u32) -> Result<Self, Self::Error> {
        (raw != 0).then_some(Self(raw)).ok_or(())
    }
}

#[test]
fn intermediate_codecs_round_trip_and_reject_invalid_input() {
    assert_eq!(co3::encode(Handle(7)), 7);
    assert_eq!(unsafe { co3::decode::<Handle>(7) }.unwrap().0, 7);
    assert!(unsafe { co3::decode::<Handle>(0) }.is_none());
}

#[derive(RustSpec, ReprC)]
#[repr_c(as(u32))]
struct AutoDropHandle(Box<u32>);

impl From<AutoDropHandle> for u32 {
    fn from(value: AutoDropHandle) -> Self {
        *value.0
    }
}

impl From<u32> for AutoDropHandle {
    fn from(value: u32) -> Self {
        Self(Box::new(value))
    }
}

#[test]
fn intermediate_representation_allows_auto_drop() {
    let encoded = co3::encode(AutoDropHandle(Box::new(7)));
    assert_eq!(encoded, 7);
    let decoded: AutoDropHandle = unsafe { co3::decode(encoded) }.unwrap();
    assert_eq!(*decoded.0, 7);
}

#[derive(Default)]
struct ConversionStore(usize);

impl Store for ConversionStore {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

struct Intermediate(u32);

impl ReprC for Intermediate {
    type CType = u32;
}

unsafe impl EncodeOwned for Intermediate {
    type Store = ConversionStore;

    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> u32
    where
        Self: 'itm,
    {
        store.0 += 1;
        self.0
    }
}

unsafe impl<'d> DecodeOwned<'d> for Intermediate {
    type Store = ConversionStore;

    unsafe fn soft_decode<'itm: 'd>(raw: u32, store: &'itm mut Self::Store) -> Option<Self> {
        store.0 += 1;
        Some(Self(raw))
    }
}

#[derive(RustSpec, ReprC)]
#[repr_c(as(Intermediate))]
struct StoredHandle(u32);

impl From<StoredHandle> for Intermediate {
    fn from(value: StoredHandle) -> Self {
        Self(value.0)
    }
}

impl TryFrom<Intermediate> for StoredHandle {
    type Error = ();

    fn try_from(value: Intermediate) -> Result<Self, Self::Error> {
        (value.0 != 0).then_some(Self(value.0)).ok_or(())
    }
}

#[test]
fn intermediate_store_is_forwarded_in_both_directions() {
    let mut encode_store = ConversionStore::default();
    assert_eq!(co3::soft_encode(StoredHandle(7), &mut encode_store), 7);
    assert_eq!(encode_store.0, 1);

    let mut decode_store = ConversionStore::default();
    assert_eq!(
        unsafe { co3::soft_decode::<StoredHandle>(7, &mut decode_store) }
            .unwrap()
            .0,
        7
    );
    assert_eq!(decode_store.0, 1);
    assert!(unsafe { co3::soft_decode::<StoredHandle>(0, &mut decode_store) }.is_none());
    assert_eq!(decode_store.0, 2);
}
