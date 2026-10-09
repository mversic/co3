use co3::ffi::c_char;
use co3::{ReprC, ffi, raw, rust_spec::RustSpec};
use core::ffi::CStr;

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
struct WrappedCStr(CStr);

#[test]
fn transparent_cstr_uses_char_data() {
    static_assertions::assert_impl_all!(WrappedCStr: co3::ffi::NulTerminatedBuf<Data = c_char>);
    static_assertions::assert_impl_all!(Box<WrappedCStr>: co3::stored::EncodeOwned, co3::stored::DecodeOwned<'static>);
}

fn cstr_len(value: &CStr) -> usize {
    value.to_bytes().len()
}

raw! {
    fn cstr_len(value: &CStr) -> usize;
}

#[test]
fn raw_cstr_uses_thin_char_pointer() {
    let callback: unsafe extern "C" fn(*const c_char) -> usize = cstr_len_raw;
    assert_eq!(unsafe { callback(c"hello".as_ptr().cast()) }, 5);
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
    assert_eq!(
        unsafe { callback(original.as_ptr().cast()) }.cast::<u8>(),
        original.as_ptr().cast::<u8>()
    );
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
