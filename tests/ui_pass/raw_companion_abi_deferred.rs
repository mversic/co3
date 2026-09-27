#![allow(improper_ctypes_definitions)]

use co3::{ReprC, raw};

fn identity<T: Copy>(value: T) -> T {
    value
}

raw! {
    fn identity<T: Copy>(value: move T) -> move T;
}

type ArrayCompanion = unsafe extern "C" fn([u8; 2]) -> [u8; 2];

static_assertions::assert_not_impl_any!(ArrayCompanion: ReprC);

fn main() {
    let _: ArrayCompanion = identity_raw::<[u8; 2]>;
}
