use core::cell::UnsafeCell;

use co3::{
    ReprC,
    borrow::Borrowed,
    ffi,
    ops::CFn1,
    raw,
    reference::{CRef, CSoftRef},
};
use static_assertions::assert_type_eq_all;

type Text = String;

ffi! {
    #![unsafe(extern("C"))]

    type Shared<'a> = raw extern "C" fn(&'a u32) -> &'a u32;
    type SharedString<'a> = raw extern "C" fn(&'a String) -> &'a String;
    type ExclusiveBox<'a> = raw extern "C" fn(&'a mut Box<u32>) -> &'a mut Box<u32>;
    type InteriorBox<'a> = raw extern "C" fn(&'a UnsafeCell<Box<u32>>) -> &'a UnsafeCell<Box<u32>>;
    type SharedAlias<'a> = raw extern "C" fn(&'a Text) -> &'a Text;
}

assert_type_eq_all!(
    Shared<'static>,
    unsafe extern "C" fn(CRef<'static, u32>) -> CRef<'static, u32>
);
assert_type_eq_all!(
    SharedString<'static>,
    unsafe extern "C" fn(CSoftRef<String>) -> CSoftRef<String>
);
assert_type_eq_all!(
    ExclusiveBox<'static>,
    unsafe extern "C" fn(
        Borrowed<<&'static mut Box<u32> as ReprC>::CType>,
    ) -> Borrowed<<&'static mut Box<u32> as ReprC>::CType>
);
assert_type_eq_all!(
    InteriorBox<'static>,
    unsafe extern "C" fn(
        Borrowed<<&'static UnsafeCell<Box<u32>> as ReprC>::CType>,
    ) -> Borrowed<<&'static UnsafeCell<Box<u32>> as ReprC>::CType>
);
assert_type_eq_all!(
    SharedAlias<'static>,
    unsafe extern "C" fn(
        Borrowed<<&'static Text as ReprC>::CType>,
    ) -> Borrowed<<&'static Text as ReprC>::CType>
);

fn echo(value: &u32) -> &u32 {
    value
}

raw! {
    fn echo(value: &u32) -> &u32;
}

#[test]
fn reference_without_ownership_can_use_call_directly() {
    let value = 42;
    let callback: Shared<'_> = echo_raw;
    let output: &u32 = unsafe { callback.call(&value) }.unwrap();
    assert!(core::ptr::eq(output, &value));
}
