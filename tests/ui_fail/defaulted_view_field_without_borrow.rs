use co3::{ReprC, rust_spec::RustSpec};

#[derive(RustSpec)]
struct NoBorrow(u32);

impl ReprC for NoBorrow {
    type CType = u32;
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Wrapper<T = NoBorrow>(T);

fn main() {}
