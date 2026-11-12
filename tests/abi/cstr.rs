use core::{ffi::CStr, marker::PhantomData};

use co3::{ReprC, ffi::c_char, rust_spec::RustSpec};

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
    static_assertions::assert_type_eq_all!(<CStr as ReprC>::CType, c_char);
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
