use co3::{
    ReprC,
    boxed::Box,
    ffi,
    ops::{CFn1, CFn2},
    raw,
    rust_spec::RustSpec,
    vec::abi,
};

type Number = u32;
fn sum(values: Vec<u32>) -> Number {
    values.iter().sum()
}
fn transfer(value: Box<u32>) -> Box<u32> {
    value
}
fn update(value: &mut bool) -> Number {
    *value = true;
    1
}
fn update_and_borrow<'a>(value: &mut bool, output: &'a u32) -> &'a u32 {
    *value = true;
    output
}
raw! {
    fn sum(values: Vec<u32>) -> Number;
    fn transfer(value: move Box<u32>) -> move Box<u32>;
    fn update(#[soft] value: &mut bool) -> Number;
    fn update_and_borrow<'a>(#[soft] value: &mut bool, output: &'a u32) -> &'a u32;
}

#[test]
fn call_returns_external_reference_while_synchronizing_argument_storage() {
    use co3::reference::{Ref, SoftRefMut};
    type Callback<'a> =
        unsafe extern "C" fn(SoftRefMut<<bool as co3::ReprC>::CType>, Ref<'a, u32>) -> Ref<'a, u32>;
    let callback: Callback<'_> = update_and_borrow_raw;
    let mut changed = false;
    let value = 42;
    let output: &u32 = unsafe { callback.soft_call(&mut changed, &value) }.unwrap();
    assert!(core::ptr::eq(output, &value));
    assert!(changed);
}

#[test]
fn call_selects_borrowed_and_moved_conversions() {
    let borrowed: unsafe extern "C" fn(_) -> _ = sum_raw;
    let result: Number = unsafe { borrowed.call(vec![2_u32, 3, 5]) }.unwrap();
    assert_eq!(result, 10);

    let moved: unsafe extern "C" fn(_) -> _ = transfer_raw;
    let value = Box::new(17);
    let pointer = core::ptr::from_ref(&*value);
    let result: Box<u32> = unsafe { moved.call(value) }.unwrap();
    assert_eq!(core::ptr::from_ref(&*result), pointer);
    assert_eq!(*result, 17);
}

#[test]
fn call_synchronizes_before_releasing_storage() {
    let callback: unsafe extern "C" fn(_) -> _ = update_raw;
    let mut value = false;
    let result: Number = unsafe { callback.soft_call(&mut value) }.unwrap();
    assert_eq!(result, 1);
    assert!(value);
}

#[unsafe(export_name = "co3_borrowed_vector_echo")]
extern "C" fn echo_carrier(value: abi::Vec<u32>) -> abi::Vec<u32> {
    value
}
ffi! {
    #![unsafe(extern("C"))]
    #[symbol_name = "co3_borrowed_vector_echo"]
    raw fn echo(value: Vec<u32>) -> Vec<u32>;
}

#[test]
fn borrowed_result_is_reconstructed_before_argument_owner_is_dropped() {
    let callback: unsafe extern "C" fn(_) -> _ = echo;
    let result: Vec<u32> = unsafe { callback.call(vec![7_u32, 11]) }.unwrap();
    assert_eq!(result, [7, 11]);
}

#[derive(Clone, RustSpec, ReprC)]
#[repr(C)]
struct MovedSoftValue(bool);

fn update_moved_soft(value: &mut MovedSoftValue) -> Number {
    value.0 = true;
    1
}

raw! {
    fn update_moved_soft(#[soft] value: move &mut MovedSoftValue) -> Number;
}

ffi! {
    #![unsafe(extern("C"))]

    type MovedSoftCallback = raw extern "C" fn(#[soft] move &mut MovedSoftValue) -> Number;
}

#[test]
fn moved_soft_call_synchronizes_both_conversion_stores() {
    let callback: MovedSoftCallback = update_moved_soft_raw;
    let mut value = MovedSoftValue(false);
    let result: Number = unsafe { callback.soft_call(&mut value) }.unwrap();
    assert_eq!(result, 1);
    assert!(value.0);
}
