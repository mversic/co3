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
#[repr_c(transparent, is_valid = |value| *value > 10)]
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

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
#[rust_spec(custom_niche)]
#[repr_c(transparent, NICHE = 0)]
struct ReservedByte(u8);

static DROP_BOX_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(RustSpec, ReprC)]
#[rust_spec(custom_drop)]
#[repr_c(transparent)]
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
    assert!(!unsafe { <ValidatedByte as co3::transmute::CheckedTransmute>::is_valid(&10) });
    assert!(unsafe { <ValidatedByte as co3::transmute::CheckedTransmute>::is_valid(&11) });
    assert!(unsafe { co3::decode::<ValidatedByte>(10) }.is_none());
    assert_eq!(unsafe { co3::decode::<ValidatedByte>(11) }.unwrap().0, 11);
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
fn field_representation_rejects_reserved_niche() {
    assert_impl_all!(ReservedByte: co3::niche::Niche<CType = u8>);
    assert_impl_all!(NonZeroField: co3::niche::Niche<CType = u8>);
    assert_eq!(<ReservedByte as co3::niche::Niche>::NICHE, 0_u8);
    assert_eq!(<NonZeroField as co3::niche::Niche>::NICHE, 0_u8);
    assert!(!unsafe { <ReservedByte as co3::transmute::CheckedTransmute>::is_valid(&0) });
    assert!(unsafe { <ReservedByte as co3::transmute::CheckedTransmute>::is_valid(&7) });
    assert!(unsafe { co3::decode::<ReservedByte>(0) }.is_none());
    assert_eq!(unsafe { co3::decode::<ReservedByte>(7) }.unwrap().0, 7);
    assert!(matches!(
        unsafe { co3::decode::<Option<ReservedByte>>(0) },
        Some(None)
    ));
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

#[derive(ReprC)]
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

#[derive(ReprC)]
#[repr_c(as(u32))]
struct GenericCustomDrop<T: Copy>(T);

impl<T: Copy> Drop for GenericCustomDrop<T> {
    fn drop(&mut self) {}
}

impl<T: Copy + Into<u32>> From<GenericCustomDrop<T>> for u32 {
    fn from(value: GenericCustomDrop<T>) -> Self {
        value.0.into()
    }
}

impl<T: Copy + TryFrom<u32>> TryFrom<u32> for GenericCustomDrop<T> {
    type Error = T::Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        T::try_from(value).map(Self)
    }
}

#[test]
fn generic_custom_drop_intermediate_round_trip() {
    let encoded = co3::encode(GenericCustomDrop(7_u32));
    assert_eq!(encoded, 7);
    let decoded: GenericCustomDrop<u32> = unsafe { co3::decode(encoded) }.unwrap();
    assert_eq!(decoded.0, 7);
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

#[derive(ReprC)]
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

#[derive(Clone, Copy, RustSpec, ReprC)]
#[rust_spec(custom_niche)]
#[repr_c(as(u32), NICHE = 0, is_valid = |value| value.is_power_of_two())]
struct PowerOfTwo(u32);

impl From<PowerOfTwo> for u32 {
    fn from(value: PowerOfTwo) -> Self {
        value.0
    }
}

impl From<u32> for PowerOfTwo {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

#[test]
fn intermediate_conversion_respects_custom_niche_and_validity() {
    assert_eq!(co3::encode(None::<PowerOfTwo>), 0);
    assert_eq!(co3::encode(Some(PowerOfTwo(4))), 4);
    assert!(unsafe { co3::decode::<PowerOfTwo>(0) }.is_none());
    assert!(unsafe { co3::decode::<PowerOfTwo>(3) }.is_none());
    assert_eq!(unsafe { co3::decode::<PowerOfTwo>(4) }.unwrap().0, 4);
    assert!(
        unsafe { co3::decode::<Option<PowerOfTwo>>(0) }
            .unwrap()
            .is_none()
    );
}

#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "reserved NICHE"))]
fn intermediate_conversion_rejects_encoding_the_reserved_niche() {
    let _ = co3::encode(PowerOfTwo(0));
}

#[derive(RustSpec, ReprC)]
#[repr_c(as(u32))]
enum ValidatedEnum {
    #[repr_c(is_valid = |value| value.is_power_of_two())]
    Power(u32),
    Other,
}

impl From<ValidatedEnum> for u32 {
    fn from(value: ValidatedEnum) -> Self {
        match value {
            ValidatedEnum::Power(raw) => raw,
            ValidatedEnum::Other => 0,
        }
    }
}

impl From<u32> for ValidatedEnum {
    fn from(raw: u32) -> Self {
        if raw == 0 {
            Self::Other
        } else {
            Self::Power(raw)
        }
    }
}

#[test]
fn intermediate_conversion_respects_variant_validity() {
    assert!(matches!(
        unsafe { co3::decode::<ValidatedEnum>(0) },
        Some(ValidatedEnum::Other)
    ));
    assert!(unsafe { co3::decode::<ValidatedEnum>(3) }.is_none());
    assert!(matches!(
        unsafe { co3::decode::<ValidatedEnum>(4) },
        Some(ValidatedEnum::Power(4))
    ));
}
