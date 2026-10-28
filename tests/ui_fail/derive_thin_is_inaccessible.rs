use co3::{rust_spec::RustSpec, ReprC};

#[derive(RustSpec, ReprC)]
struct Record(u32);

fn requires_thin<T: Thin>() {}

fn main() {}
