use co3::ffi::c_char;
use co3::{ReprC, boxed::CBox, ffi, rust_spec::RustSpec, stored::EncodeOwned};
use core::ffi::CStr;
use static_assertions::{assert_impl_all, assert_not_impl_any};
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
        let ptr = co3::borrow::borrow_cast(self.0);
        let value = unsafe { CStr::from_ptr(ptr.cast()) }.to_owned();
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

static FIELDLESS_DROPS: AtomicUsize = AtomicUsize::new(0);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct NoDropValue(u32);

#[test]
fn no_drop_borrows_itself_without_owner() {
    use co3::borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow};

    fn assert_identity_cast<C: BorrowCast<AsConst = C>>() {}
    fn assert_identity_mut_cast<C: BorrowCastMut<AsMut = C>>() {}
    assert_identity_cast::<CNoDropValue>();
    assert_identity_mut_cast::<CNoDropValue>();

    let mut owner: <NoDropValue as Borrow>::Owner = ();
    let borrowed: NoDropValue = Borrow::borrow(NoDropValue(5), &mut owner);
    let restored = <NoDropValue as FromBorrow>::from_borrow(borrowed);
    assert_eq!(restored.0, 5);
}

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct InheritedDrop(Box<u32>);

#[test]
fn inherited_drop_borrows_fields() {
    use co3::borrow::{Borrow, BorrowCastMut, FromBorrow};

    static_assertions::assert_type_eq_all!(
        <CInheritedDrop as BorrowCastMut>::AsMut,
        CInheritedDropMutView
    );

    let mut owner = <InheritedDrop as Borrow>::Owner::default();
    let borrowed = Borrow::borrow(InheritedDrop(Box::new(7)), &mut owner);
    let borrowed: InheritedDropView<'_> = borrowed;
    let restored = <InheritedDrop as FromBorrow>::from_borrow(borrowed);
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
    use co3::borrow::{Borrow, FromBorrow};

    let mut owner = <CustomAndInheritedDrop as Borrow>::Owner::default();
    let borrowed = Borrow::borrow(CustomAndInheritedDrop(Box::new(9)), &mut owner);
    let borrowed: &CustomAndInheritedDrop = borrowed;
    let restored = <CustomAndInheritedDrop as FromBorrow>::from_borrow(borrowed);
    assert_eq!(*restored.0, 9);
}

#[test]
fn custom_drop_companion_has_no_borrow_casts() {
    use co3::borrow::{BorrowCast, BorrowCastMut};

    assert_not_impl_any!(CCustomAndInheritedDrop: BorrowCast, BorrowCastMut);
    assert_impl_all!(CBox<CCustomAndInheritedDrop>: BorrowCast, BorrowCastMut);
}

#[derive(RustSpec, ReprC)]
#[repr(C, u8)]
enum InheritedDropEnum {
    Value(Box<u32>),
    Empty,
}

#[test]
fn inherited_drop_enum_borrows_fields() {
    use co3::borrow::{Borrow, FromBorrow};

    let mut owner = <InheritedDropEnum as Borrow>::Owner::default();
    let borrowed = Borrow::borrow(InheritedDropEnum::Value(Box::new(11)), &mut owner);
    let borrowed: InheritedDropEnumView<'_> = borrowed;
    let restored = <InheritedDropEnum as FromBorrow>::from_borrow(borrowed);
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
fn fieldless_enums_with_drop_convert_and_borrow() {
    use co3::borrow::{Borrow, FromBorrow};

    let before = FIELDLESS_DROPS.load(Ordering::Relaxed);
    assert_eq!(co3::encode(ExplicitFieldlessDrop::Second).0, 5);
    assert_eq!(co3::encode(InferredFieldlessDrop::Second).0, 3);
    assert_eq!(FIELDLESS_DROPS.load(Ordering::Relaxed) - before, 2);

    let decoded = ExplicitFieldlessDrop::try_from(CExplicitFieldlessDrop(2)).unwrap();
    assert!(matches!(decoded, ExplicitFieldlessDrop::First));
    drop(decoded);
    assert!(ExplicitFieldlessDrop::try_from(CExplicitFieldlessDrop(3)).is_err());

    let mut owner = <ExplicitFieldlessDrop as Borrow>::Owner::default();
    let borrowed = Borrow::borrow(ExplicitFieldlessDrop::Second, &mut owner);
    let cloned = <ExplicitFieldlessDrop as FromBorrow>::from_borrow(borrowed);
    assert!(matches!(cloned, ExplicitFieldlessDrop::Second));
    drop(cloned);
    drop(owner);
    assert_eq!(FIELDLESS_DROPS.load(Ordering::Relaxed) - before, 5);
}
