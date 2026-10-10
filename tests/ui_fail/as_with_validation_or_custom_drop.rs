use co3::{ReprC, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[rust_spec(custom_niche)]
#[repr_c(as(u8), NICHE = 0)]
struct WithNiche(u8);

#[derive(RustSpec, ReprC)]
#[repr_c(as(u8), is_valid = |value| *value != 0)]
struct WithValidation(u8);

#[derive(RustSpec, ReprC)]
#[repr_c(as(u8))]
enum WithVariantValidation {
    #[repr_c(is_valid = |value| *value != 0)]
    Value(u8),
}

#[derive(RustSpec, ReprC)]
#[rust_spec(custom_drop)]
#[repr_c(as(u8))]
struct WithCustomDrop(u8);

impl Drop for WithCustomDrop {
    fn drop(&mut self) {}
}

impl From<WithCustomDrop> for u8 {
    fn from(value: WithCustomDrop) -> Self {
        value.0
    }
}

impl From<u8> for WithCustomDrop {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

fn main() {}
