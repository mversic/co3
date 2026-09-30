use co3::{ffi, raw};

fn overwrite(bytes: &mut [u8]) -> usize {
    bytes.fill(7);
    bytes.len()
}

fn narrow(value: u8) -> u8 {
    value + 1
}

raw! {
    fn overwrite(#[unpack(_, _)] bytes: &mut [u8]) -> usize;
    fn narrow(#[unpack(u16)] value: u8) -> u8;
}

#[test]
fn raw_companion_reconstructs_unpacked_slice() {
    let callback: unsafe extern "C" fn(*mut u8, usize) -> usize = overwrite_raw;
    let mut bytes = [0_u8; 3];
    assert_eq!(unsafe { callback(bytes.as_mut_ptr(), bytes.len()) }, 3);
    assert_eq!(bytes, [7; 3]);
    let narrow_callback: unsafe extern "C" fn(u16) -> u8 = narrow_raw;
    assert_eq!(unsafe { narrow_callback(41) }, 42);
}

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
