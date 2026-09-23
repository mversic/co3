use co3::{ReprC, ffi, rust_spec::RustSpec};
type CCallback = extern "C" fn(Value, Value) -> Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, RustSpec, ReprC)]
#[reprC(identity)]
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

    extern "C" fn echo_raw(value: Self) -> Self;
}

fn double_rust(value: RustValue) -> RustValue {
    RustValue(value.0 * 2)
}

extern "C" fn sum_pair(left: Value, right: Value) -> Value {
    Value(left.0 + right.0)
}

fn apply_callback(callback: CCallback, left: Value, right: Value) -> Value {
    callback(left, right)
}

fn apply_optional_callback(callback: Option<CCallback>, value: Value) -> Value {
    callback.map_or(value, |call| call(value, Value(1)))
}

fn sum_native(left: Value, right: Value) -> Value {
    Value(left.0 + right.0)
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "abi_c_callback"]

    fn apply_callback(callback: CCallback, left: Value, right: Value) -> Value;
    fn apply_optional_callback(callback: Option<CCallback>, value: Value) -> Value;
}

mod imported {
    use co3::ffi;

    use super::*;

    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "abi_c_callback"]

        pub fn apply_callback(callback: super::CCallback, left: Value, right: Value) -> Value;
        pub fn apply_optional_callback(callback: Option<super::CCallback>, value: Value) -> Value;

    }
}

ffi! {
    #![unsafe(extern("C"))]

    raw fn double_rust(value: RustValue) -> RustValue;
    pub raw fn sum_native(left: Value, right: Value) -> Value;

    impl Echo for Value {
        raw fn echo(self) -> Value;
    }

    impl RustValue {
        pub raw fn scaled(#[soft] &self, factor: u8) -> RustValue;
    }
}

#[test]
fn c_callback_crosses_export_and_import() {
    fn assert_existing_fn_pointer_impls<T: co3::ExternC + co3::Encode + co3::Decode<'static>>(
        _: T,
    ) {
    }
    assert_existing_fn_pointer_impls(sum_native_raw as CCallback);
    let raw_value = co3::borrow::borrow_cast(co3::encode(RustValue(21)));
    let raw_result = double_rust_raw(raw_value);
    assert_eq!(
        unsafe { co3::decode::<RustValue>(raw_result) },
        Some(RustValue(42))
    );
    let encoded = co3::encode(RustValue(7));
    let raw_result = RustValue::scaled_raw(&encoded, 6);
    assert_eq!(
        unsafe { co3::decode::<RustValue>(raw_result) },
        Some(RustValue(42))
    );
    assert_eq!(<Value as Echo>::echo_raw(Value(42)), Value(42));
    assert_eq!(
        imported::apply_callback(sum_pair, Value(19), Value(23)),
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
