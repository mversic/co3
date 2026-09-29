use co3::{ReprC, raw, rust_spec::RustSpec};

extern "system" fn system_source(value: u8) -> u8 {
    value + 1
}

extern "Rust" fn explicit_rust_source(value: u8) -> u8 {
    value + 4
}

fn rust_source(value: u8) -> u8 {
    value + 2
}

fn default_source(value: u8) -> u8 {
    value + 3
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr_c(identity)]
#[repr(transparent)]
struct Value(u8);

impl Value {
    fn add(self, amount: u8) -> u8 {
        self.0 + amount
    }

    extern "system" fn add_system(self, amount: u8) -> u8 {
        self.0 + amount + 1
    }
}

raw! {
    pub fn default_source(value: u8) -> u8;

    impl Value {
        pub fn add(self, amount: u8) -> u8;
    }
}

raw! {
    pub fn rust_source(value: u8) -> u8;
    pub extern "system" fn system_source(value: u8) -> u8;
    pub extern "Rust" fn explicit_rust_source(value: u8) -> u8;

    impl Value {
        pub extern "system" fn add_system(self, amount: u8) -> u8;
    }
}

type OrdinaryAlias<T> = Option<T>;

co3::ffi! {
    #![unsafe(export("C"))]
    type SystemCallback = raw extern "system" fn(u8) -> u8;
}

fn main() {
    let c: unsafe extern "system" fn(u8) -> u8 = system_source;
    let system: unsafe extern "C" fn(u8) -> u8 = rust_source_raw;
    let default: unsafe extern "C" fn(u8) -> u8 = default_source_raw;
    let method: unsafe extern "C" fn(Value, u8) -> u8 = Value::add_raw;
    let callback: SystemCallback = system_source;
    let system_companion: unsafe extern "C" fn(u8) -> u8 = system_source_raw;
    let rust_companion: unsafe extern "C" fn(u8) -> u8 = explicit_rust_source_raw;
    let method_companion: unsafe extern "C" fn(Value, u8) -> u8 = Value::add_system_raw;
    let _: OrdinaryAlias<Value> = Some(Value(1));
    let _: unsafe extern "system" fn(u8) -> u8 = callback;

    assert_eq!(unsafe { c(1) }, 2);
    assert_eq!(unsafe { system(1) }, 3);
    assert_eq!(unsafe { default(1) }, 4);
    assert_eq!(unsafe { method(Value(1), 2) }, 3);
    assert_eq!(unsafe { callback(1) }, 2);
    assert_eq!(unsafe { system_companion(1) }, 2);
    assert_eq!(unsafe { rust_companion(1) }, 5);
    assert_eq!(unsafe { method_companion(Value(1), 2) }, 4);
}
