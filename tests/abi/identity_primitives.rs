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
    assert_eq!(unsafe { raw_increment(c_int::new(20)) }, c_int::new(21));
    assert_eq!(imported_double(c_long::new(21)), c_long::new(42));
    let raw_carrier: unsafe extern "C" fn(c_int) -> c_int = increment_carrier_raw;
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

    let view =
        co3::borrow::Borrow::borrow(AliasFields(c_int::new(3), 4_u32, c_long::new(5)), &mut ());
    let borrowed_lowered = co3::encode(view);
    let _: c_int = borrowed_lowered.0;
    let _: c_long = borrowed_lowered.2;
    let view =
        co3::borrow::Borrow::borrow(AliasFields(c_int::new(3), 4_u32, c_long::new(5)), &mut ());
    let recovered: AliasFields<u32> = co3::borrow::FromBorrow::from_borrow(view);
    assert_eq!(
        (recovered.0, recovered.1, recovered.2),
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

#[test]
fn lowered_aggregates_use_codecs_without_conversion_traits() {
    use co3::{option::ReprCOption, result::ReprCResult, tuple::ReprCTuple2};
    use static_assertions::assert_not_impl_any;

    assert_not_impl_any!(ReprCOption<u8>: From<Option<u8>>, TryFrom<Option<u8>>);
    assert_not_impl_any!(Option<u8>: From<ReprCOption<u8>>, TryFrom<ReprCOption<u8>>);
    assert_not_impl_any!(ReprCResult<u8, u8>: From<Result<u8, u8>>, TryFrom<Result<u8, u8>>);
    assert_not_impl_any!(Result<u8, u8>: From<ReprCResult<u8, u8>>, TryFrom<ReprCResult<u8, u8>>);
    assert_not_impl_any!(ReprCTuple2<u8, u8>: From<(u8, u8)>, TryFrom<(u8, u8)>);
    assert_not_impl_any!((u8, u8): From<ReprCTuple2<u8, u8>>, TryFrom<ReprCTuple2<u8, u8>>);

    for value in [None, Some(42_u8)] {
        assert_eq!(
            unsafe { co3::decode::<Option<u8>>(co3::encode(value)) },
            Some(value)
        );
    }
    let option = ReprCOption::Some(42_u8);
    assert_eq!(unsafe { co3::decode::<Option<u8>>(option) }, Some(Some(42)));

    for value in [Ok(42_u8), Err(7_u8)] {
        let mut store = Default::default();
        let lowered = co3::soft_encode(value, &mut store);
        assert_eq!(
            unsafe { co3::decode::<Result<u8, u8>>(lowered) },
            Some(value)
        );
    }
    let result = ReprCResult::<u8, u8>::Err(7_u8);
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
        boxed::{CBox, CBoxedSlice},
        reference::CRefMut,
        slice::{CSlice, CSliceMut},
        str::Utf8String,
    };

    let boxed = CBox::from_box(Box::new(9_u8));
    assert_eq!(*unsafe { co3::decode::<Box<u8>>(boxed) }.unwrap(), 9);

    let raw = Box::into_raw(Box::new(10_u8));
    let boxed = unsafe { CBox::from_non_null(core::ptr::NonNull::new(raw).unwrap()) };
    assert_eq!(*unsafe { co3::decode::<Box<u8>>(boxed) }.unwrap(), 10);

    let boxed_slice = CBoxedSlice::from_boxed_slice(Box::from([4_u8, 5]));
    assert_eq!(
        &*unsafe { co3::decode::<Box<[u8]>>(boxed_slice) }.unwrap(),
        &[4, 5]
    );
    assert_eq!(CBox::<u8>::null(), co3::encode(None::<Box<u8>>));
    assert_eq!(CBoxedSlice::<u8>::null(), co3::encode(None::<Box<[u8]>>));
    assert_eq!(CSlice::<u8>::null(), co3::encode(None::<&[u8]>));
    assert_eq!(
        CRefMut::<u8>::null(),
        co3::soft_encode(None::<&mut u8>, &mut Default::default())
    );
    let _: CSliceMut<u8> = CSliceMut::null();
    assert_eq!(Utf8String::null(), co3::encode(None::<String>));

    let utf8 = Utf8String(CBoxedSlice::from_boxed_slice(Box::from("hello".as_bytes())));
    assert_eq!(unsafe { co3::decode::<String>(utf8) }.unwrap(), "hello");

    let bytes: Box<[u8]> = Box::from("world".as_bytes());
    let len = bytes.len();
    let data = core::ptr::NonNull::new(Box::into_raw(bytes).cast::<u8>()).unwrap();
    let utf8 = Utf8String(unsafe { CBoxedSlice::from_raw_parts(data, len) });
    assert_eq!(unsafe { co3::decode::<String>(utf8) }.unwrap(), "world");

    let bytes = [1_u8, 2];
    let _slice = CSlice::from_slice(&bytes);
    assert_eq!(
        CSlice::<u8>::from_slice("hello".as_bytes()),
        co3::encode("hello")
    );
    let mut bytes = [3_u8, 4];
    let _slice_mut = CSliceMut::<u8>::from_slice(&mut bytes);
    let _ref_mut = CRefMut::<u8>::from_mut(&mut bytes[0]);
}
