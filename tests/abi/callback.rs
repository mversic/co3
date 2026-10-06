use co3::{
    CType, ReprC, ffi, ops::{CFn0, CFn1, CFn2, CFn12}, option::ReprCOption, raw, restrict::CRestrict, rust_spec::RustSpec,
};

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr_c(identity)]
#[repr(C)]
struct StructWithCallback(Option<unsafe extern "C" fn(*const u32) -> u32>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, RustSpec, ReprC)]
#[repr_c(identity)]
#[repr(C)]
struct Value(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
struct RustValue(u8);

impl RustValue {
    fn scaled(&self, factor: u8) -> Self {
        Self(self.0 * factor)
    }
}

trait Echo: Sized {
    fn echo(self) -> Self {
        self
    }

    unsafe extern "C" fn echo_raw(value: Self) -> Self;
}

mod raw {
    pub(super) type Existing = super::CCallback;
}

type CompanionPathAlias = raw::Existing;

unsafe extern "C" fn increment_pointer(value: *const u32) -> u32 {
    (unsafe { *value }) + 1
}

unsafe extern "C" fn return_pointer(value: *const u32) -> *const u32 {
    value
}

unsafe extern "C" fn ignore_pointer(_: *const u32) {}

unsafe extern "C" fn update_option(value: CRestrict<ReprCOption<u8>>) -> u32 {
    let value = value.as_ptr();
    unsafe { *value = ReprCOption::Some(42) };
    7
}

extern "C" fn return_forty_two() -> u32 {
    42
}

extern "system" fn increment_system(value: *const u32) -> u32 {
    (unsafe { *value }) + 2
}

unsafe extern "C" fn add_from_pointer(value: *const u32, offset: u32) -> u32 {
    (unsafe { *value }) + offset
}

extern "C" fn sum_twelve(
    a0: u32,
    a1: u32,
    a2: u32,
    a3: u32,
    a4: u32,
    a5: u32,
    a6: u32,
    a7: u32,
    a8: u32,
    a9: u32,
    a10: u32,
    a11: u32,
) -> u32 {
    a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11
}

fn double_rust(value: RustValue) -> RustValue {
    RustValue(value.0 * 2)
}

extern "C" fn sum_pair(left: Value, right: Value) -> Value {
    Value(left.0 + right.0)
}

fn apply_callback(callback: CCallback, left: Value, right: Value) -> Value {
    unsafe { callback(left, right) }
}

fn apply_optional_callback(callback: Option<CCallback>, value: Value) -> Value {
    callback.map_or(value, |call| unsafe { call(value, Value(1)) })
}

fn sum_native(left: Value, right: Value) -> Value {
    Value(left.0 + right.0)
}

#[unsafe(export_name = "abi_c_callback_raw_method")]
extern "C" fn add_values(left: Value, right: Value) -> Value {
    Value(left.0 + right.0)
}

#[unsafe(export_name = "abi_system_callback")]
extern "system" fn add_system_values(left: Value, right: Value) -> Value {
    Value(left.0 + right.0)
}

fn return_owned(value: Box<Value>) -> Box<Value> {
    value
}

fn borrow_rust_value(value: RustValue) -> RustValue {
    value
}

extern "C" fn borrowed_return() -> *const Value {
    core::ptr::null()
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "abi_c_callback"]

    type CCallback = raw extern "C" fn(Value, Value) -> Value;

    fn apply_callback(callback: CCallback, left: Value, right: Value) -> Value;
    fn apply_optional_callback(callback: Option<CCallback>, value: Value) -> Value;
    #[symbol_name = "abi_borrowed_value"]
    fn borrow_rust_value(value: RustValue) -> RustValue;
}

mod borrowed_export_import {
    use super::*;

    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "abi_c_callback"]

        #[symbol_name = "abi_borrowed_value"]
        pub fn borrow_rust_value(value: RustValue) -> RustValue;
    }
}

#[test]
fn borrowed_return_round_trips_through_custom_symbol() {
    let input = RustValue(7);
    assert_eq!(borrowed_export_import::borrow_rust_value(input), input);
}

mod exported_raw_import {
    use co3::ffi;

    use super::*;

    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "abi_c_callback_exported_raw"]

        pub fn sum_native(left: Value, right: Value) -> Value;
        #[symbol_name = "abi_c_callback_exported_raw__sum_native"]
        pub raw fn sum_native_raw(left: Value, right: Value) -> Value;
    }
}

mod system_wrapped_raw_import {
    use co3::ffi;

    use super::*;

    ffi! {
        #![unsafe(extern("system"))]
        #![symbol_prefix = "abi_system_callback"]

        #[symbol_name = "abi_system_callback"]
        pub raw fn sum_native(left: Value, right: Value) -> Value;
    }
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "abi_c_callback_exported_raw"]

    fn sum_native(left: Value, right: Value) -> Value;
}

mod imported {
    use co3::ffi;

    use super::*;

    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "abi_c_callback"]

        type CCallback = raw extern "C" fn(Value, Value) -> Value;

        pub fn apply_callback(callback: CCallback, left: Value, right: Value) -> Value;
        #[symbol_name = "abi_c_callback__apply_callback"]
        pub raw fn apply_callback_raw(callback: CCallback, left: Value, right: Value) -> Value;
        pub fn apply_optional_callback(callback: Option<CCallback>, value: Value) -> Value;
    }
}

mod raw_only {
    use co3::ffi;

    use super::*;

    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "abi_c_callback"]

        pub raw fn apply_callback(callback: CCallback, left: Value, right: Value) -> Value;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    impl Value {
        #[symbol_name = "abi_c_callback_raw_method"]
        pub raw fn add_raw(self, other: Value) -> Value;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    type BorrowedReturn = raw extern "C" fn() -> Box<Value>;
    type BorrowedRustValue = raw extern "C" fn(RustValue) -> RustValue;
    type OwnedTransform = raw extern "C" fn(move Box<Value>) -> move Box<Value>;
    type CompanionCallback = raw extern "C" fn(Value, Value) -> Value;
}

raw! {
    fn borrow_rust_value(value: RustValue) -> RustValue;
    fn return_owned(value: move Box<Value>) -> move Box<Value>;
    fn double_rust(value: RustValue) -> move RustValue;
    pub fn sum_native(left: Value, right: Value) -> Value;

    impl Echo for Value {
        fn echo(self) -> Value;
    }

    impl RustValue {
        pub fn scaled(#[soft] &self, factor: u8) -> move RustValue;
    }
}

#[test]
fn zero_argument_callback_decodes_result() {
    let callback: unsafe extern "C" fn() -> _ = return_forty_two;
    let result: Option<u32> = unsafe { callback.call() };
    assert_eq!(result, Some(42));
}

#[test]
fn stored_c_callback_accepts_rust_reference() {
    fn assert_first_arg<T: CFn1<Arg1 = *const u32>>()
    where
        Option<T>: CType,
    {
    }
    assert_first_arg::<unsafe extern "C" fn(*const u32) -> u32>();

    let callback = StructWithCallback(Some(increment_pointer));
    let value = 41_u32;
    let result: Option<u32> = unsafe { callback.0.unwrap().call(&value) };
    assert_eq!(result, Some(42));

    let void_callback: unsafe extern "C" fn(_) = ignore_pointer;
    let result: Option<()> = unsafe { void_callback.call(&value) };
    assert_eq!(result, Some(()));

    let system_callback: unsafe extern "system" fn(_) -> _ = increment_system;
    let result: Option<u32> = unsafe { system_callback.call(&value) };
    assert_eq!(result, Some(43));
}

#[test]
fn callback_returns_rust_reference() {
    let callback: unsafe extern "C" fn(_) -> _ = return_pointer;
    let value = 42_u32;
    let result: Option<&u32> = unsafe { callback.call(&value) };

    assert_eq!(result, Some(&value));
}

#[test]
fn soft_call_synchronizes_mutable_argument() {
    let callback: unsafe extern "C" fn(_) -> _ = update_option;
    let mut value = Some(1_u8);

    let result: Option<u32> = unsafe { callback.soft_call(&mut value) };

    assert_eq!(result, Some(7));
    assert_eq!(value, Some(42));
}

#[test]
fn multi_argument_callbacks_encode_each_argument() {
    fn assert_two_args<T: CFn2<Arg1 = *const u32, Arg2 = u32>>()
    where
        Option<T>: CType,
    {
    }
    assert_two_args::<unsafe extern "C" fn(*const u32, u32) -> u32>();

    let value = 40_u32;
    let callback: unsafe extern "C" fn(_, _) -> _ = add_from_pointer;
    let result: Option<u32> = unsafe { callback.call(&value, 2_u32) };
    assert_eq!(result, Some(42));

    let callback: unsafe extern "C" fn(_, _, _, _, _, _, _, _, _, _, _, _) -> _ = sum_twelve;
    let result: Option<u32> = unsafe {
        callback.call(
            1_u32, 2_u32, 3_u32, 4_u32, 5_u32, 6_u32, 7_u32, 8_u32, 9_u32, 10_u32, 11_u32, 12_u32,
        )
    };
    assert_eq!(result, Some(78));
}

#[test]
fn c_callback_crosses_export_and_import() {
    let _: CompanionCallback = sum_pair;
    let _: CompanionPathAlias = sum_pair;
    let _: BorrowedReturn = borrowed_return;
    let _: BorrowedRustValue = borrow_rust_value_raw;
    let _: OwnedTransform = return_owned_raw;
    let result = unsafe { return_owned_raw(co3::encode(Box::new(Value(42)))) };
    assert_eq!(
        unsafe { co3::decode::<Box<Value>>(result) },
        Some(Box::new(Value(42)))
    );
    fn assert_existing_fn_pointer_impls<T>(_: T) {}
    assert_existing_fn_pointer_impls(sum_native_raw as CCallback);
    let raw_value = co3::borrow::borrow_cast(co3::encode(RustValue(21)));
    let raw_result = unsafe { double_rust_raw(raw_value) };
    assert_eq!(
        unsafe { co3::decode::<RustValue>(raw_result) },
        Some(RustValue(42))
    );
    let encoded = co3::encode(RustValue(7));
    let raw_result = unsafe { RustValue::scaled_raw(&encoded, 6) };
    assert_eq!(
        unsafe { co3::decode::<RustValue>(raw_result) },
        Some(RustValue(42))
    );
    assert_eq!(unsafe { <Value as Echo>::echo_raw(Value(42)) }, Value(42));
    assert_eq!(
        imported::apply_callback(sum_pair, Value(19), Value(23)),
        Value(42)
    );
    assert_eq!(
        unsafe { imported::apply_callback_raw(Some(sum_pair), Value(19), Value(23)) },
        Value(42)
    );
    assert_eq!(
        unsafe { raw_only::apply_callback(Some(sum_pair), Value(19), Value(23)) },
        Value(42)
    );
    assert_eq!(unsafe { Value::add_raw(Value(19), Value(23)) }, Value(42));
    let system_raw: unsafe extern "system" fn(Value, Value) -> Value =
        system_wrapped_raw_import::sum_native;
    assert_eq!(unsafe { system_raw(Value(19), Value(23)) }, Value(42));
    assert_eq!(
        exported_raw_import::sum_native(Value(19), Value(23)),
        Value(42)
    );
    assert_eq!(
        unsafe { exported_raw_import::sum_native_raw(Value(19), Value(23)) },
        Value(42)
    );
    assert_eq!(
        imported::apply_callback(sum_native_raw, Value(19), Value(23)),
        Value(42)
    );
    assert_eq!(
        imported::apply_optional_callback(Some(sum_pair), Value(41)),
        Value(42)
    );
    assert_eq!(
        imported::apply_optional_callback(None, Value(41)),
        Value(41)
    );
}
