use co3::{ReprC, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Wrapper<T: ?Sized = core::ffi::CStr>(T);

#[derive(RustSpec)]
#[rust_spec(custom_drop)]
struct NoBorrow(u8);

impl Drop for NoBorrow {
    fn drop(&mut self) {}
}

impl ReprC for NoBorrow {
    type CType = u8;
}

#[derive(RustSpec, ReprC)]
struct SizedWrapper<T = NoBorrow>(T);

fn accept(_value: SizedWrapper) {}

co3::raw! {
    fn accept(value: SizedWrapper);
}

fn main() {}
