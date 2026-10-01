use co3::{ReprC, ffi, raw, rust_spec::RustSpec};
use core::ffi::{CStr, c_char};

fn cstr_len(value: &CStr) -> usize {
    value.to_bytes().len()
}

raw! {
    fn cstr_len(value: &CStr) -> usize;
}

#[test]
fn raw_cstr_uses_thin_char_pointer() {
    let callback: unsafe extern "C" fn(*const c_char) -> usize = cstr_len_raw;
    assert_eq!(unsafe { callback(c"hello".as_ptr()) }, 5);
}

#[unsafe(export_name = "abi_cstr__borrowed")]
extern "C" fn borrowed_source(value: *const c_char) -> *const c_char {
    value
}

#[unsafe(export_name = "abi_cstr__borrowed_native")]
extern "C" fn borrowed_native_source(value: *const c_char) -> *const c_char {
    value
}

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "abi_cstr"]

    raw fn borrowed<'a>(value: &'a CStr) -> &'a CStr;
    fn borrowed_native<'a>(value: &'a CStr) -> &'a CStr;
}

#[test]
fn imported_cstr_uses_thin_char_pointer() {
    let callback: unsafe extern "C" fn(*const c_char) -> *const c_char = borrowed;
    let original = c"hello";
    assert_eq!(unsafe { callback(original.as_ptr()) }, original.as_ptr());
    assert_eq!(borrowed_native(original), original);
}

#[repr(C)]
#[derive(RustSpec, ReprC)]
struct CStrTail {
    tag: u32,
    tail: CStr,
}

#[test]
fn cstr_tail_companion_starts_tail_at_char_field() {
    type CCompanion = <CStrTail as ReprC>::CType;
    assert_eq!(
        core::mem::offset_of!(CCompanion, tail),
        core::mem::size_of::<u32>()
    );
}
