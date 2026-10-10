use co3::{ReprC, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[rust_spec(custom_drop)]
#[repr_c(transparent)]
struct Dropped(u8);

impl Drop for Dropped {
    fn drop(&mut self) {}
}

fn main() {}
