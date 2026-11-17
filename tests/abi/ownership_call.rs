use co3::{
    borrow::CBorrowed,
    ffi,
    ops::{CFn1, CFn2},
    raw,
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
#[allow(clippy::boxed_local, clippy::redundant_allocation)] // Exercise allocation ownership and writeback together.
fn update_box(value: Box<&mut bool>) -> Number {
    **value = true;
    1
}
raw! {
    fn sum(values: Vec<u32>) -> Number;
    fn transfer(value: move Box<u32>) -> move Box<u32>;
    fn update(#[soft] value: &mut bool) -> Number;
    fn update_and_borrow<'a>(#[soft] value: &mut bool, output: &'a u32) -> &'a u32;
    fn update_box(#[soft] value: Box<&mut bool>) -> Number;
}

#[test]
fn call_returns_external_reference_while_synchronizing_argument_storage() {
    use co3::reference::{CRef, CSoftRefMut};
    type Callback<'a> =
        unsafe extern "C" fn(CSoftRefMut<bool, true>, CRef<'a, u32>) -> CRef<'a, u32>;
    let callback: Callback<'_> = update_and_borrow_raw;
    let mut changed = false;
    let value = 42;
    let output: &u32 = unsafe { callback.call(&mut changed, &value) }.unwrap();
    assert!(core::ptr::eq(output, &value));
    assert!(changed);
}

#[test]
fn call_selects_borrowed_and_moved_conversions() {
    let borrowed: unsafe extern "C" fn(_) -> _ = sum_raw;
    let result: Number = unsafe { borrowed.call(vec![2, 3, 5]) }.unwrap();
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
    let result: Number = unsafe { callback.call(&mut value) }.unwrap();
    assert_eq!(result, 1);
    assert!(value);
}

#[unsafe(export_name = "co3_borrowed_vector_echo")]
extern "C" fn echo_carrier(value: CBorrowed<Vec<u32>>) -> CBorrowed<Vec<u32>> {
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
    let result: Vec<u32> = unsafe { callback.call(vec![7, 11]) }.unwrap();
    assert_eq!(result, [7, 11]);
}

#[test]
fn soft_borrowed_call_retains_owner_until_writeback() {
    type Callback<'a> = unsafe extern "C" fn(CBorrowed<Box<&'a mut bool>>) -> CBorrowed<Number>;
    let callback: Callback<'_> = update_box_raw;
    let mut value = false;
    let result: Number = unsafe { callback.call(Box::new(&mut value)) }.unwrap();
    assert_eq!(result, 1);
    assert!(value);
}
