use co3::{ReprC, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
pub enum EnumWithExplicitDiscriminant {
    A = 1,
    B(String),
    C,
}

fn main() {}
