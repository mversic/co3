use core::{ffi::CStr, marker::PhantomData};

use co3::{ReprC, ffi, ffi::c_char, raw, reference::CSoftRef, rust_spec::RustSpec};

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
struct WrappedCStr(CStr);

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
struct WrappedCStrWithMarkers(PhantomData<()>, (), CStr);

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
struct NamedWrappedCStr {
    _marker: PhantomData<()>,
    value: CStr,
}

#[test]
fn transparent_cstr_uses_char_data() {
    static_assertions::assert_type_eq_all!(<CStr as ReprC>::CType, co3::ffi::CStr);
    static_assertions::assert_type_ne_all!(co3::ffi::CStr, c_char);
    assert_eq!(
        core::mem::size_of::<co3::ffi::CStr>(),
        core::mem::size_of::<c_char>()
    );
    assert_eq!(
        core::mem::align_of::<co3::ffi::CStr>(),
        core::mem::align_of::<c_char>()
    );
    static_assertions::assert_impl_all!(WrappedCStr: co3::ffi::NulTerminatedBuf<Header = c_char>);
    static_assertions::assert_impl_all!(Box<WrappedCStr>: co3::stored::EncodeMove, co3::stored::DecodeMove<'static>);
    static_assertions::assert_impl_all!(WrappedCStrWithMarkers: co3::ffi::NulTerminatedBuf<Header = c_char>);
    static_assertions::assert_impl_all!(NamedWrappedCStr: co3::ffi::NulTerminatedBuf<Header = c_char>);

    let source = c"hello";
    let wrapped = unsafe {
        <WrappedCStrWithMarkers as co3::ffi::NulTerminatedBuf>::from_raw(source.as_ptr().cast())
    };
    assert_eq!(wrapped.2.to_bytes(), source.to_bytes());
    assert_eq!(
        <WrappedCStrWithMarkers as co3::ffi::NulTerminatedBuf>::as_ptr(wrapped),
        source.as_ptr().cast(),
    );
}

fn cstr_len(value: &CStr) -> usize {
    value.to_bytes().len()
}

raw! {
    fn cstr_len(value: &CStr) -> usize;
}

#[test]
fn raw_cstr_uses_thin_char_pointer() {
    let callback: unsafe extern "C" fn(CSoftRef<CStr>) -> usize = cstr_len_raw;
    assert_eq!(unsafe { callback(co3::encode(c"hello")) }, 5);
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
    let callback: unsafe extern "C" fn(CSoftRef<CStr>) -> CSoftRef<CStr> = borrowed;
    let original = c"hello";
    assert_eq!(
        unsafe { callback(co3::encode(original)) }
            .as_ptr()
            .cast::<u8>(),
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
