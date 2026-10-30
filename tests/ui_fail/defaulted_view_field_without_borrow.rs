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

fn main() {
    fn require_borrow<T: co3::borrow::Borrow>() {}
    require_borrow::<SizedWrapper>();
}
