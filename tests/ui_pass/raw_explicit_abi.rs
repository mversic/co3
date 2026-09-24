use co3::{ReprC, ffi, rust_spec::RustSpec};

extern "system" fn system_source(value: u8) -> u8 {
    value + 1
}

fn rust_source(value: u8) -> u8 {
    value + 2
}

fn default_source(value: u8) -> u8 {
    value + 3
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[reprC(identity)]
#[repr(transparent)]
struct Value(u8);

impl Value {
    extern "system" fn add(self, amount: u8) -> u8 {
        self.0 + amount
    }
}

ffi! {
    #![unsafe(extern("C"))]

    type SystemCallback = raw "system" fn(u8) -> u8;

    pub raw "C" extern "system" fn system_source(value: u8) -> u8;
    pub raw "system" fn rust_source(value: u8) -> u8;
    pub raw fn default_source(value: u8) -> u8;

    impl Value {
        pub raw "C" extern "system" fn add(self, amount: u8) -> u8;
    }
}

fn main() {
    let c: unsafe extern "C" fn(u8) -> u8 = system_source_raw;
    let system: unsafe extern "system" fn(u8) -> u8 = rust_source_raw;
    let default: unsafe extern "C" fn(u8) -> u8 = default_source_raw;
    let method: unsafe extern "C" fn(Value, u8) -> u8 = Value::add_raw;
    let callback: SystemCallback = system_source;
    let _: unsafe extern "system" fn(u8) -> u8 = callback;

    assert_eq!(unsafe { c(1) }, 2);
    assert_eq!(unsafe { system(1) }, 3);
    assert_eq!(unsafe { default(1) }, 4);
    assert_eq!(unsafe { method(Value(1), 2) }, 3);
    assert_eq!(unsafe { callback(1) }, 2);
}
