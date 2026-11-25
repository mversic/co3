#![no_std]

use co3::{ReprC, lang, rust_spec::RustSpec};

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(C)]
pub struct Record<'a> {
    pub value: Option<&'a u32>,
}

pub fn round_trip(value: Record<'_>) -> Option<Record<'_>> {
    unsafe { co3::decode::<_, lang::C>(co3::encode::<_, lang::C>(value)) }
}

co3::ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "derive_only"]

    fn echo(value: Record<'_>) -> Record<'_>;
}

fn echo(value: Record<'_>) -> Record<'_> {
    value
}
