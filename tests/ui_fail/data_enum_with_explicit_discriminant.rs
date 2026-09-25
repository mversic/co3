use co3::{ReprC, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(u8)]
pub enum EnumWithExplicitDiscriminant {
    A = 1,
    B(String),
    C,
    D,
}

fn main() {}
