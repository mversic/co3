use co3::ffi::{c_int, c_long};

use co3::{ReprC, ffi, raw, rust_spec::RustSpec};

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

fn flip(value: bool) -> bool {
    !value
}

fn next_ascii(value: char) -> char {
    char::from_u32(value as u32 + 1).unwrap()
}

raw! {
    fn flip(value: bool) -> bool;
    fn next_ascii(value: char) -> char;
}

fn increment(value: c_int) -> c_int {
    c_int(value.0 + 1)
}

fn increment_carrier(value: c_int) -> c_int {
    c_int(value.0 + 1)
}

raw! {
    fn increment(value: c_int) -> c_int;
    fn increment_carrier(value: c_int) -> c_int;
}

fn double(value: c_long) -> c_long {
    c_long(value.0 * 2)
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "identity_primitives"]

    fn double(value: c_long) -> c_long;
}

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "identity_primitives__double"]
    fn imported_double(value: c_long) -> c_long;
}

#[test]
fn identity_primitives_keep_their_abi_types() {
    let raw_increment: unsafe extern "C" fn(c_int) -> c_int = increment_raw;
    assert_eq!(unsafe { raw_increment(c_int(20)) }, c_int(21));
    assert_eq!(imported_double(c_long(21)), c_long(42));
    let raw_carrier: unsafe extern "C" fn(c_int) -> c_int = increment_carrier_raw;
    let result = unsafe { raw_carrier(c_int(20)) };
    assert_eq!(result, c_int(21));
}

#[test]
fn c_alias_fields_use_distinct_carriers() {
    let carrier_field = co3::encode(CarrierField(c_int(3)));
    let _: c_int = carrier_field.0;
    let lowered = co3::encode(AliasFields(c_int(3), 4_u32, c_long(5)));
    let _: c_int = lowered.0;
    let _: u32 = lowered.1;
    let _: c_long = lowered.2;
    let decoded = unsafe { co3::decode::<AliasFields<u32>>(lowered) }.unwrap();
    assert_eq!((decoded.0, decoded.1, decoded.2), (c_int(3), 4, c_long(5)));

    let view = co3::borrow::Borrow::borrow(AliasFields(c_int(3), 4_u32, c_long(5)), &mut ());
    let borrowed_lowered = co3::encode(view);
    let _: c_int = borrowed_lowered.0;
    let _: c_long = borrowed_lowered.2;
    let view = co3::borrow::Borrow::borrow(AliasFields(c_int(3), 4_u32, c_long(5)), &mut ());
    let recovered: AliasFields<u32> = co3::borrow::FromBorrow::from_borrow(view);
    assert_eq!(
        (recovered.0, recovered.1, recovered.2),
        (c_int(3), 4, c_long(5))
    );
}

#[test]
fn c_alias_enum_field_round_trips() {
    let lowered = co3::encode(AliasEnum::Value(c_int(7)));
    let decoded = unsafe { co3::decode::<AliasEnum>(lowered) }.unwrap();
    assert!(matches!(decoded, AliasEnum::Value(c_int(7))));
}

#[test]
fn c_alias_field_preserves_inferred_niche() {
    let niche = <AliasWithNiche as co3::niche::Niche>::NICHE;
    let _: c_int = niche.0;
    assert_eq!(niche.1, <Option<bool> as co3::niche::Niche>::NICHE);
}

#[test]
fn copy_primitives_with_custom_carriers_use_value_conversion() {
    let raw_flip: unsafe extern "C" fn(_) -> _ = flip_raw;
    let raw_next_ascii: unsafe extern "C" fn(_) -> _ = next_ascii_raw;

    let flipped = unsafe { raw_flip(co3::encode(true)) };
    assert_eq!(unsafe { co3::decode::<bool>(flipped) }, Some(false));
    assert_eq!(
        unsafe { raw_next_ascii(co3::encode('a')) },
        co3::encode('b')
    );
}
