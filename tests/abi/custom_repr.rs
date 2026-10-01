use co3::{
    ReprC,
    rust_spec::RustSpec,
    stored::{DecodeOwned, EncodeOwned, Store},
};

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
#[rust_spec(with_custom_niche)]
#[repr_c(as(u32), NICHE_VALUE = 0, is_valid = |value| value.is_power_of_two())]
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
#[cfg_attr(debug_assertions, should_panic(expected = "reserved NICHE_VALUE"))]
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
