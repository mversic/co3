use core::ffi::CStr;
use std::{
    ffi::CString,
    sync::atomic::{AtomicUsize, Ordering},
};

use co3::{
    CType, ReprC,
    boxed::{Box, abi::Box as AbiBox},
    ffi,
    marker::Owned,
    ops::CFn1,
    rust_spec::{self, RustSpec},
};

type BatteryString = CString;
type BatteryCStr = Box<CStr>;

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[rust_spec(custom_drop)]
struct RemoteString(AbiBox<co3::ffi::c_char>);

impl Clone for RemoteString {
    fn clone(&self) -> Self {
        let ptr = self.0;
        let value = unsafe { CStr::from_ptr(ptr.as_ptr().cast()) }.to_owned();
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

ffi! {
    #![unsafe(extern("C"))]
    #[symbol_name = "abi_regular_drop_cstr"]
    raw fn drop_cstr(value: move BatteryCStr);
}

#[test]
fn regular_drop_decodes_owned_carrier() {
    let callback: unsafe extern "C" fn(_) = drop_cstr;
    let result: Option<()> =
        unsafe { callback.call(co3::encode(CString::new("battery").unwrap())) };
    assert_eq!(result, Some(()));
}

#[test]
fn regular_drop_import_calls_export() {
    let remote = RemoteString(co3::encode(CString::new("battery").unwrap()));
    drop(remote);
}

static MOVABLE_DROPS: AtomicUsize = AtomicUsize::new(0);

#[derive(RustSpec, ReprC)]
#[repr(C)]
#[rust_spec(custom_drop)]
struct MovableDrop(u32);

impl Drop for MovableDrop {
    fn drop(&mut self) {
        MOVABLE_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(ReprC)]
#[repr(transparent)]
struct MovableTransparentDrop(u32);

// The ownership marker follows this classification even without a derive attribute.
unsafe impl RustSpec for MovableTransparentDrop {
    type Layout = <u32 as RustSpec>::Layout;
    type Size = <u32 as RustSpec>::Size;
    type __Alignment = <u32 as RustSpec>::__Alignment;
    type Trap = <u32 as RustSpec>::Trap;
    type Niche = <u32 as RustSpec>::Niche;
    type Drop = rust_spec::drop::CustomDrop<rust_spec::drop::NoDrop>;
    type Mutability = <u32 as RustSpec>::Mutability;
    type Opacity = <u32 as RustSpec>::Opacity;
    type __IndirectTrap = <u32 as RustSpec>::__IndirectTrap;
}

impl Drop for MovableTransparentDrop {
    fn drop(&mut self) {
        MOVABLE_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(RustSpec, ReprC)]
#[repr(u8)]
#[rust_spec(custom_drop)]
enum MovableFieldlessDrop {
    Value = 3,
}

impl Drop for MovableFieldlessDrop {
    fn drop(&mut self) {
        MOVABLE_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(RustSpec, ReprC)]
#[repr(u8)]
#[rust_spec(custom_drop)]
enum MovableTransparentFieldlessDrop {
    Value = 4,
}

impl Drop for MovableTransparentFieldlessDrop {
    fn drop(&mut self) {
        MOVABLE_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn custom_drop_types_use_owned_carriers() {
    let before = MOVABLE_DROPS.load(Ordering::Relaxed);

    fn assert_owned<T: CType>(_: &Owned<T>) {}

    let carrier = co3::encode(MovableDrop(7));
    assert_owned(&carrier);
    assert_eq!(MOVABLE_DROPS.load(Ordering::Relaxed), before);
    let decoded: MovableDrop = unsafe { co3::decode(carrier) }.unwrap();
    assert_eq!(decoded.0, 7);
    drop(decoded);

    let carrier = unsafe { co3::encode_unchecked(MovableDrop(9)) };
    assert_eq!(MOVABLE_DROPS.load(Ordering::Relaxed), before + 1);
    let decoded: MovableDrop = unsafe { co3::decode(carrier) }.unwrap();
    assert_eq!(decoded.0, 9);
    drop(decoded);

    let carrier = co3::encode(MovableFieldlessDrop::Value);
    assert_owned(&carrier);
    assert_eq!(MOVABLE_DROPS.load(Ordering::Relaxed), before + 2);
    let decoded: MovableFieldlessDrop = unsafe { co3::decode(carrier) }.unwrap();
    drop(decoded);

    let carrier = co3::encode(MovableTransparentDrop(11));
    assert_owned(&carrier);
    assert_eq!(MOVABLE_DROPS.load(Ordering::Relaxed), before + 3);
    let decoded: MovableTransparentDrop = unsafe { co3::decode(carrier) }.unwrap();
    assert_eq!(decoded.0, 11);
    drop(decoded);

    let carrier = co3::encode(MovableTransparentFieldlessDrop::Value);
    assert_eq!(MOVABLE_DROPS.load(Ordering::Relaxed), before + 4);
    let decoded: MovableTransparentFieldlessDrop = unsafe { co3::decode(carrier) }.unwrap();
    drop(decoded);

    assert_eq!(MOVABLE_DROPS.load(Ordering::Relaxed), before + 5);
}
