use core::ffi::c_int;

use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(C)]
struct Value(c_int);

ffi! {
    #![unsafe(extern("C"))]
    fn take(value: c_int);
}

fn main() {}
