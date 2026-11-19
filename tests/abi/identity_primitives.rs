use co3::{
    ReprC,
    boxed::Box,
    ffi,
    ffi::{c_int, c_long},
    niche::Niche,
    raw,
    rust_spec::RustSpec,
};

use crate::TransparentCarrier;

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(C)]
struct AliasFields<T>(c_int, T, c_long);

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(C)]
struct CarrierField(c_int);

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(C)]
struct AliasWithNiche(c_int, Option<bool>);

#[derive(Clone, Copy, RustSpec, ReprC)]
enum AliasEnum {
    Value(c_int),
    Empty,
}

fn increment(value: c_int) -> c_int {
    value + c_int::new(1)
}

fn increment_carrier(value: c_int) -> c_int {
    value + c_int::new(1)
}

raw! {
    fn increment(value: c_int) -> c_int;
    fn increment_carrier(value: c_int) -> c_int;
}

fn double(value: c_long) -> c_long {
    value + value
}

fn update(seed: u32, value: &mut u32) -> u32 {
    *value += seed;
    *value
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "identity_primitives"]

    fn double(value: c_long) -> c_long;

    #[symbol_name = "argument_conversion_update"]
    fn update(
        #[soft] seed: u32,
        #[cfg(any())] absent: MissingType,
        #[soft] value: &mut u32,
    ) -> u32;
}

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "identity_primitives__double"]
    fn imported_double(value: c_long) -> c_long;

    #[symbol_name = "argument_conversion_update"]
    fn imported_update(
        #[soft] seed: u32,
        #[cfg(any())] absent: MissingType,
        #[soft] value: &mut u32,
    ) -> u32;
}

#[test]
fn identity_arguments_skip_stores_without_skipping_reference_conversion() {
    let mut value = 7;
    assert_eq!(imported_update(5, &mut value), 12);
    assert_eq!(value, 12);
}

#[test]
fn identity_primitives_keep_their_abi_types() {
    let raw_increment: unsafe extern "C" fn(_) -> _ = increment_raw;
    assert_eq!(unsafe { raw_increment(c_int::new(20)) }, c_int::new(21));
    assert_eq!(imported_double(c_long::new(21)), c_long::new(42));
    let raw_carrier: unsafe extern "C" fn(_) -> _ = increment_carrier_raw;
    let result = unsafe { raw_carrier(c_int::new(20)) };
    assert_eq!(result, c_int::new(21));
}

#[test]
fn c_alias_fields_use_distinct_carriers() {
    let carrier_field = co3::encode(CarrierField(c_int::new(3)));
    let _: c_int = carrier_field.0;
    let lowered = co3::encode(AliasFields(c_int::new(3), 4_u32, c_long::new(5)));
    let _: c_int = lowered.0;
    let _: u32 = lowered.1;
    let _: c_long = lowered.2;
    let decoded = unsafe { co3::decode::<AliasFields<u32>>(lowered) }.unwrap();
    assert_eq!(
        (decoded.0, decoded.1, decoded.2),
        (c_int::new(3), 4, c_long::new(5))
    );
}

#[test]
fn c_alias_enum_field_round_trips() {
    let lowered = co3::encode(AliasEnum::Value(c_int::new(7)));
    let decoded = unsafe { co3::decode::<AliasEnum>(lowered) }.unwrap();
    assert!(matches!(decoded, AliasEnum::Value(value) if value == c_int::new(7)));
}

#[test]
fn c_alias_field_preserves_inferred_niche() {
    let niche = <AliasWithNiche as Niche>::NICHE;
    let _: c_int = niche.0;
    assert!(<Option<bool> as Niche>::is_niche(&niche.1));
    assert!(<AliasWithNiche as Niche>::is_niche(&niche));
}

#[test]
fn lowered_aggregates_use_codecs_without_conversion_traits() {
    use co3::{option::abi::Option as AbiOption, result::abi::Result as AbiResult, tuple::Tuple2};
    use static_assertions::assert_not_impl_any;

    assert_not_impl_any!(AbiOption<u8>: From<Option<u8>>, TryFrom<Option<u8>>);
    assert_not_impl_any!(Option<u8>: From<AbiOption<u8>>, TryFrom<AbiOption<u8>>);
    assert_not_impl_any!(AbiResult<u8, u8>: From<Result<u8, u8>>, TryFrom<Result<u8, u8>>);
    assert_not_impl_any!(Result<u8, u8>: From<AbiResult<u8, u8>>, TryFrom<AbiResult<u8, u8>>);
    assert_not_impl_any!(Tuple2<u8, u8>: From<(u8, u8)>, TryFrom<(u8, u8)>);
    assert_not_impl_any!((u8, u8): From<Tuple2<u8, u8>>, TryFrom<Tuple2<u8, u8>>);

    for value in [None, Some(42_u8)] {
        assert_eq!(
            unsafe { co3::decode::<Option<u8>>(co3::encode(value)) },
            Some(value)
        );
    }
    let option = AbiOption::Some(42_u8);
    assert_eq!(unsafe { co3::decode::<Option<u8>>(option) }, Some(Some(42)));

    for value in [Ok(42_u8), Err(7_u8)] {
        let mut store = Default::default();
        let lowered = co3::soft_encode(value, &mut store);
        assert_eq!(
            unsafe { co3::decode::<Result<u8, u8>>(lowered) },
            Some(value)
        );
    }
    let result = AbiResult::<u8, u8>::Err(7_u8);
    assert_eq!(
        unsafe { co3::decode::<Result<u8, u8>>(result) },
        Some(Err(7))
    );
    assert_eq!(
        unsafe { co3::decode::<(u8, u8)>(co3::encode((42_u8, 7_u8))) },
        Some((42, 7))
    );
}

#[test]
fn safe_carrier_constructors_accept_lowered_storage() {
    use co3::{
        boxed::abi::Box as AbiBox,
        reference::abi::{SoftRef, SoftRefMut},
        wide::abi::{
            SoftWideRef as AbiSoftWideRef, SoftWideRefMut as AbiSoftWideRefMut,
            WideBox as AbiWideBox,
        },
    };

    let boxed = AbiBox::from_box(Box::new(9_u8));
    let borrowed = unsafe { SoftRef::<u8>::from_raw(boxed.as_ptr()) };
    assert_eq!(borrowed.as_ptr(), boxed.as_ptr());
    assert_eq!(*unsafe { co3::decode::<Box<u8>>(boxed) }.unwrap(), 9);

    let value = 8_u8;
    assert_eq!(
        co3::reference::abi::Ref::from_ref(&value),
        co3::encode(&value)
    );
    assert!(co3::encode(None::<&u8>).payload().as_ptr().is_null());

    let raw = Box::into_raw(Box::new(10_u8));
    let boxed = unsafe { AbiBox::from_non_null(core::ptr::NonNull::new(raw).unwrap()) };
    assert_eq!(*unsafe { co3::decode::<Box<u8>>(boxed) }.unwrap(), 10);

    let boxed_slice = AbiWideBox::from_box(Box::<[u8]>::from([4_u8, 5]));
    assert_eq!(
        &*unsafe { co3::decode::<Box<[u8]>>(boxed_slice) }.unwrap(),
        &[4, 5]
    );
    assert!(co3::encode(None::<Box<u8>>).payload().as_ptr().is_null());
    assert!(co3::encode(None::<Box<[u8]>>).payload().as_ptr().is_null());
    assert!(co3::encode(None::<&[u8]>).payload().as_ptr().is_null());
    assert!(
        co3::soft_encode(None::<&mut u8>, &mut Default::default())
            .payload()
            .as_ptr()
            .is_null()
    );
    let _: AbiSoftWideRefMut<[u8]> = AbiSoftWideRefMut::null();
    assert!(co3::encode(None::<String>).payload().as_ptr().is_null());

    let bytes: Box<[u8]> = Box::from("hello".as_bytes());
    let utf8 = co3::string::abi::String(co3::vec::abi::Vec::from_vec(bytes.into_vec()));
    assert_eq!(unsafe { co3::decode::<String>(utf8) }.unwrap(), "hello");

    let bytes: Box<[u8]> = Box::from("world".as_bytes());
    let len = bytes.len();
    let utf8 = co3::string::abi::String(co3::vec::abi::Vec::from_vec(bytes.into_vec()));
    assert_eq!(utf8.len(), len);
    assert_eq!(unsafe { co3::decode::<String>(utf8) }.unwrap(), "world");

    let bytes = [1_u8, 2];
    let _slice = AbiSoftWideRef::<[u8]>::from_ref(&bytes[..]);
    assert_eq!(
        unsafe {
            co3::wide::abi::WideRef::<<str as co3::ReprC>::CType>::from_raw_parts(
                "hello".as_ptr(),
                "hello".len(),
            )
        },
        co3::encode("hello")
    );
    let mut bytes = [3_u8, 4];
    let _slice_mut = AbiSoftWideRefMut::<[u8]>::from_mut(&mut bytes[..]);
    let _ref_mut = SoftRefMut::<u8>::from_mut(&mut bytes[0]);
}

fn round_trip_native_box(value: Box<bool>) -> Box<bool> {
    value
}

fn pass_abi_box(
    value: co3::boxed::abi::Box<co3::primitive::abi::bool>,
) -> co3::boxed::abi::Box<co3::primitive::abi::bool> {
    value
}

co3::raw! {
    fn round_trip_native_box(value: move Box<bool>) -> move Box<bool>;
    fn pass_abi_box(value: move co3::boxed::abi::Box<co3::primitive::abi::bool>) -> move co3::boxed::abi::Box<co3::primitive::abi::bool>;
}

#[test]
fn raw_functions_distinguish_native_and_abi_boxes() {
    let carrier = co3::encode(Box::new(true));
    let carrier = unsafe { pass_abi_box_raw(carrier) };
    let carrier = unsafe { round_trip_native_box_raw(carrier) };
    assert!(*unsafe { co3::decode::<Box<bool>>(carrier) }.unwrap());
}
