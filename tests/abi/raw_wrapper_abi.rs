use co3::ffi;

#[unsafe(export_name = "raw_wrapper__value")]
extern "C" fn value_source(value: u8) -> u8 {
    value + 1
}

#[unsafe(export_name = "raw_wrapper__Host__method")]
extern "C" fn method_source(value: u8) -> u8 {
    value + 2
}

#[unsafe(export_name = "raw_wrapper__borrowed")]
extern "C" fn borrowed_source(value: *const u8) -> *const u8 {
    value
}

struct Host;

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "raw_wrapper"]

    raw extern "Rust" fn value(value: u8) -> u8;
    raw fn borrowed<'a>(value: &'a u8) -> &'a u8;

    impl Host {
        raw extern "system" fn method(value: u8) -> u8;
    }
}

#[test]
fn explicit_abi_wraps_raw_imports() {
    let _: unsafe extern "Rust" fn(u8) -> u8 = value;
    let _: unsafe extern "system" fn(u8) -> u8 = Host::method;
    let _: unsafe extern "C" fn(*const u8) -> *const u8 = borrowed;
    assert_eq!(unsafe { value(4) }, 5);
    assert_eq!(unsafe { Host::method(4) }, 6);
    let input = 7;
    assert_eq!(unsafe { borrowed(&input) }, &input as *const u8);
}
