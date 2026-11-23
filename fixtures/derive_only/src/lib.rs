#![no_std]

use co3::{ReprC, rust_spec::RustSpec};

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(C)]
pub struct Record<'a> {
    pub value: Option<&'a u32>,
}

pub fn round_trip(value: Record<'_>) -> Option<Record<'_>> {
    unsafe { co3::decode(co3::encode(value)) }
}

co3::ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "derive_only"]

    fn echo(value: Record<'_>) -> Record<'_>;
}

fn echo(value: Record<'_>) -> Record<'_> {
    value
}
