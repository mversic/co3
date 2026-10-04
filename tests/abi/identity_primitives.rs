use core::ffi::{c_int, c_long};

use co3::{ffi, raw};

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
    value + 1
}

raw! {
    fn increment(value: c_int) -> c_int;
}

fn double(value: c_long) -> c_long {
    value * 2
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
    assert_eq!(unsafe { raw_increment(20) }, 21);
    assert_eq!(imported_double(21), 42);
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
