#![allow(improper_ctypes_definitions)]

use co3::{raw, ReprC};

fn existing(value: u8) -> u8 {
    value
}

raw! {
    fn existing(value: u8) -> u8;
}

fn identity<T>(value: T) -> T {
    value
}

raw! {
    fn identity<T>(value: move T) -> move T;
}

type ArrayCompanion = unsafe extern "C" fn([u8; 2]) -> [u8; 2];

static_assertions::assert_not_impl_any!(ArrayCompanion: ReprC);

fn accept(_callback: for<'a> unsafe extern "C" fn()) {}

raw! {
    pub fn accept(callback: for<'a> unsafe extern "C" fn());
}

fn borrowed_generic<'a, T>(value: &'a T) -> &'a T {
    value
}

raw! {
    fn borrowed_generic<'a, T>(value: &'a T) -> &'a T;
}

fn take<'a, T>(_value: &'a T) {}

struct ArgumentHost;

impl ArgumentHost {
    fn take<'a, T>(_value: &'a T) {}
}

raw! {
    fn take<'a, T>(value: &'a T);

    impl ArgumentHost {
        fn take<'a, T>(value: &'a T);
    }
}

struct MethodHost;

impl MethodHost {
    fn borrowed_generic<'a, T>(value: &'a T) -> &'a T {
        value
    }
}

raw! {
    impl MethodHost {
        fn borrowed_generic<'a, T>(value: &'a T) -> &'a T;
    }
}

fn main() {
    let _: ArrayCompanion = identity_raw::<[u8; 2]>;

    let value = 7_u8;
    unsafe {
        take_raw::<u8>(&value);
        ArgumentHost::take_raw::<u8>(&value);
    }
}
