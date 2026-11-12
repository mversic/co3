use co3::{ReprC, rust_spec::RustSpec};

// Borrowed conversion validates the original Box field without requiring Clone on the struct.
#[derive(RustSpec, ReprC)]
#[rust_spec(non_robust)]
#[repr_c(is_valid = |value| **value != 0)]
struct NonZeroBox(Box<u32>);

fn main() {}
