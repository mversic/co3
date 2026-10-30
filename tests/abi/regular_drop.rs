use co3::ffi::c_char;
use co3::{ReprC, boxed::CBox, ffi, rust_spec::RustSpec};
use core::ffi::CStr;
use static_assertions::assert_impl_all;
use std::ffi::CString;
use std::sync::atomic::{AtomicUsize, Ordering};

type BatteryString = CString;
type BatteryCStr = Box<CStr>;

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[rust_spec(custom_drop)]
struct RemoteString(CBox<c_char>);

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

unsafe extern "C" {
    #[link_name = "abi_regular_drop_cstr"]
    fn drop_cstr(value: CBox<c_char>);

}

#[test]
fn regular_drop_decodes_owned_carrier() {
    let carrier = co3::encode(CString::new("battery").unwrap());
    unsafe { drop_cstr(carrier) };
}

#[test]
fn regular_drop_import_calls_export() {
    let remote = RemoteString(co3::encode(CString::new("battery").unwrap()));
    drop(remote);
}

static FIELDLESS_DROPS: AtomicUsize = AtomicUsize::new(0);
static MOVABLE_DROPS: AtomicUsize = AtomicUsize::new(0);

#[derive(RustSpec, ReprC)]
#[repr(C)]
#[repr_c(move)]
#[rust_spec(custom_drop)]
struct MovableDrop(u32);

impl Drop for MovableDrop {
    fn drop(&mut self) {
        MOVABLE_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[repr_c(transparent, move)]
#[rust_spec(custom_drop)]
struct MovableTransparentDrop(u32);

impl Drop for MovableTransparentDrop {
    fn drop(&mut self) {
        MOVABLE_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(RustSpec, ReprC)]
#[repr(u8)]
#[repr_c(move)]
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
#[repr_c(transparent, move)]
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
fn move_opt_in_enables_checked_and_unchecked_custom_drop_moves() {
    let before = MOVABLE_DROPS.load(Ordering::Relaxed);

    let carrier = co3::encode(MovableDrop(7));
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
    assert_eq!(MOVABLE_DROPS.load(Ordering::Relaxed), before + 2);
    let decoded: MovableFieldlessDrop = unsafe { co3::decode(carrier) }.unwrap();
    drop(decoded);

    let carrier = co3::encode(MovableTransparentDrop(11));
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

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct NoDropValue(u32);

#[test]
fn no_drop_borrows_itself_without_owner() {
    use co3::borrow::Borrow;
    static_assertions::assert_impl_all!(NoDropValue:
        co3::stored::EncodeMut<'static, Store = ()>,
        co3::stored::EncodeMutUnchecked<'static, SoftStore = ()>);

    static_assertions::assert_type_eq_all!(<CNoDropValue as Borrow>::Borrowed, CNoDropValue);

    let borrowed: NoDropValue = Borrow::borrow(NoDropValue(5));
    let restored = <NoDropValue as Borrow>::from_borrow(borrowed);
    assert_eq!(restored.0, 5);
}

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct InheritedDrop(Box<u32>);

#[test]
fn inherited_drop_borrows_fields() {
    use co3::borrow::Borrow;

    static_assertions::assert_type_eq_all!(<CInheritedDrop as Borrow>::Borrowed, CInheritedDrop);

    let borrowed = Borrow::borrow(InheritedDrop(Box::new(7)));
    let borrowed: InheritedDropView = borrowed;
    let restored = <InheritedDrop as Borrow>::from_borrow(borrowed);
    assert_eq!(*restored.0, 7);
}

#[derive(Clone, RustSpec, ReprC)]
#[repr(C)]
#[rust_spec(custom_drop)]
struct CustomAndInheritedDrop(Box<u32>);

impl Drop for CustomAndInheritedDrop {
    fn drop(&mut self) {}
}

#[test]
fn custom_and_inherited_drop_borrows_whole_value() {
    use co3::borrow::Borrow;

    let borrowed = Borrow::borrow(CustomAndInheritedDrop(Box::new(9)));
    let borrowed: co3::borrow::Borrowed<CustomAndInheritedDrop> = borrowed;
    let restored = <CustomAndInheritedDrop as Borrow>::from_borrow(borrowed);
    assert_eq!(*restored.0, 9);
}

#[test]
fn custom_drop_companion_borrows_as_self() {
    static_assertions::assert_type_eq_all!(
        <CCustomAndInheritedDrop as co3::borrow::Borrow>::Borrowed,
        CCustomAndInheritedDrop
    );
    assert_impl_all!(CBox<CCustomAndInheritedDrop>: co3::borrow::Borrow);
}

#[derive(RustSpec, ReprC)]
#[repr(C, u8)]
enum InheritedDropEnum {
    #[repr_c(allow_zero_tag)]
    Value(Box<u32>),
    Empty,
}

#[test]
fn inherited_drop_enum_borrows_fields() {
    use co3::borrow::Borrow;

    let borrowed = Borrow::borrow(InheritedDropEnum::Value(Box::new(11)));
    let borrowed: InheritedDropEnumView = borrowed;
    let restored = <InheritedDropEnum as Borrow>::from_borrow(borrowed);
    assert!(matches!(restored, InheritedDropEnum::Value(value) if *value == 11));
}

#[derive(Clone, RustSpec, ReprC)]
#[repr(u8)]
#[rust_spec(custom_drop)]
enum ExplicitFieldlessDrop {
    First = 2,
    Second = 5,
}

impl Drop for ExplicitFieldlessDrop {
    fn drop(&mut self) {
        FIELDLESS_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Clone, RustSpec, ReprC)]
#[rust_spec(custom_drop)]
enum InferredFieldlessDrop {
    First = 2,
    Second,
}

impl Drop for InferredFieldlessDrop {
    fn drop(&mut self) {
        FIELDLESS_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn fieldless_enums_with_drop_borrow() {
    use co3::borrow::Borrow;

    let before = FIELDLESS_DROPS.load(Ordering::Relaxed);

    let borrowed = Borrow::borrow(ExplicitFieldlessDrop::Second);
    let cloned = <ExplicitFieldlessDrop as Borrow>::from_borrow(borrowed);
    assert!(matches!(cloned, ExplicitFieldlessDrop::Second));
    drop(cloned);
    assert_eq!(FIELDLESS_DROPS.load(Ordering::Relaxed) - before, 1);
}
