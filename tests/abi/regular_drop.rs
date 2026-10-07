use co3::{ReprC, boxed::CBox, ffi, rust_spec::RustSpec, stored::EncodeOwned};
use core::ffi::{CStr, c_char};
use std::ffi::CString;
use std::sync::atomic::{AtomicUsize, Ordering};

static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
static DROP_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone, RustSpec, ReprC)]
#[repr(C)]
#[repr_c(with_custom_drop)]
struct DropProbe<T: Copy>(T);

impl<T: Copy> Drop for DropProbe<T> {
    fn drop(&mut self) {
        DROP_COUNT.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Clone, RustSpec, ReprC)]
#[repr(u8)]
#[repr_c(with_custom_drop)]
enum DropVariant {
    Value(u8),
    Empty,
}

impl Drop for DropVariant {
    fn drop(&mut self) {
        DROP_COUNT.fetch_add(1, Ordering::SeqCst);
    }
}

type BatteryString = CString;
type BatteryCStr = Box<CStr>;

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[repr_c(with_custom_drop)]
struct RemoteString(CBox<c_char>);

impl Clone for RemoteString {
    fn clone(&self) -> Self {
        let ptr = co3::borrow::borrow_cast(self.0);
        let value = unsafe { CStr::from_ptr(ptr) }.to_owned();
        Self(co3::encode(value))
    }
}

ffi! {
    #![unsafe(extern("C"))]

    impl Drop for RemoteString {
        #[symbol_name = "abi_regular_drop_cstring"]
        fn drop(&mut self);
    }
}

ffi! {
    #![unsafe(export("C"))]

    impl Drop for BatteryCStr {
        #[symbol_name = "abi_regular_drop_cstr"]
        fn drop(&mut self);
    }

    impl Drop for BatteryString {
        #[symbol_name = "abi_regular_drop_cstring"]
        fn drop(&mut self);
    }
}

unsafe extern "C" {
    #[link_name = "abi_regular_drop_cstr"]
    fn drop_cstr(value: CBox<c_char>);

}

#[test]
fn regular_drop_decodes_owned_carrier() {
    let carrier = CString::new("battery")
        .unwrap()
        .into_boxed_c_str()
        .soft_encode(&mut ());
    unsafe { drop_cstr(carrier) };
}

#[test]
fn regular_drop_import_calls_export() {
    let remote = RemoteString(co3::encode(CString::new("battery").unwrap()));
    drop(remote);
}

#[test]
fn custom_drop_borrow_keeps_original_in_owner() {
    use co3::borrow::{Borrow, FromBorrow};

    let _guard = DROP_TEST_LOCK.lock().unwrap();
    let before = DROP_COUNT.load(Ordering::SeqCst);
    let mut owner = Default::default();
    let borrowed = Borrow::borrow(DropProbe(7_u8), &mut owner);
    assert_eq!(borrowed.0, 7);
    let cloned: DropProbe<u8> = FromBorrow::from_borrow(borrowed);
    assert_eq!(cloned.0, 7);
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), before);
    drop(owner);
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), before + 1);
    drop(cloned);
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), before + 2);
}

#[test]
fn custom_drop_owned_encode_does_not_run_destructor() {
    let _guard = DROP_TEST_LOCK.lock().unwrap();
    let before = DROP_COUNT.load(Ordering::SeqCst);
    let carrier = co3::encode(DropProbe(11_u8));
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), before);
    let value: DropProbe<u8> = unsafe { co3::decode(carrier) }.unwrap();
    assert_eq!(value.0, 11);
    drop(value);
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), before + 1);
}

#[test]
fn custom_drop_data_enum_roundtrip() {
    let _guard = DROP_TEST_LOCK.lock().unwrap();
    let before = DROP_COUNT.load(Ordering::SeqCst);
    let carrier = co3::encode(DropVariant::Value(5));
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), before);
    let value: DropVariant = unsafe { co3::decode(carrier) }.unwrap();
    assert!(matches!(value, DropVariant::Value(5)));
    drop(value);
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), before + 1);
}
