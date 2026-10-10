use co3::{ReprC, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr_c(transparent, NICHE = 0)]
struct Reserved(u8);

#[derive(RustSpec, ReprC)]
#[repr_c(transparent, is_valid = |value| *value > 0)]
struct Validated(u8);

fn main() {}
