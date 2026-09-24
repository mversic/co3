#![allow(unpredictable_function_pointer_comparisons)]

use co3::{ffi, ops::CFn0, rust_spec::RustSpec, ReprC};

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(C)]
struct MixedCallbacks {
    c: Option<extern "C" fn(u8) -> u8>,
    system: Option<extern "system" fn(u8) -> u8>,
    zero: Option<unsafe extern "C" fn() -> u8>,
}

extern "C" fn identity(value: u8) -> u8 {
    value
}

extern "system" fn system_identity(value: u8) -> u8 {
    value
}

extern "C" fn four() -> u8 {
    4
}

fn accept_c_callback(callback: extern "C" fn(u8) -> u8, callbacks: MixedCallbacks) -> u8 {
    let zero: Option<u8> = unsafe { callbacks.zero.unwrap().call() };
    callback(1) + callbacks.c.unwrap()(2) + callbacks.system.unwrap()(3) + zero.unwrap()
}

fn return_c_callback() -> extern "C" fn(u8) -> u8 {
    identity
}

ffi! {
    #![unsafe(export("system"))]
    #![symbol_prefix = "callback_mixed_abi"]

    fn accept_c_callback(callback: extern "C" fn(u8) -> u8, callbacks: MixedCallbacks) -> u8;
    fn return_c_callback() -> extern "C" fn(u8) -> u8;
}

mod imported {
    use super::*;

    ffi! {
        #![unsafe(extern("system"))]
        #![symbol_prefix = "callback_mixed_abi"]

        pub fn accept_c_callback(callback: extern "C" fn(u8) -> u8, callbacks: MixedCallbacks) -> u8;
        pub fn return_c_callback() -> extern "C" fn(u8) -> u8;
    }
}

fn main() {
    let callbacks = MixedCallbacks {
        c: Some(identity),
        system: Some(system_identity),
        zero: Some(four),
    };
    assert_eq!(imported::accept_c_callback(identity, callbacks), 10);
    assert_eq!(imported::return_c_callback()(4), 4);
}
