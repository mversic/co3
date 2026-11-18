use co3::{ReprC, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
struct Record(u32);

fn requires_thin<T: Thin>() {}

fn main() {}
