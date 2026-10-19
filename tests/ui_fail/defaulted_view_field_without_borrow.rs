use co3::{ReprC, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Wrapper<T: ?Sized = core::ffi::CStr>(T);

fn main() {}
