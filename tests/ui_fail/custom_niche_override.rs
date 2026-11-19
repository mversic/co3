use core::num::NonZeroU8;

use co3::{ReprC, num::NonZero, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[rust_spec(custom_niche)]
#[repr_c(NICHE = OverridesInferredNicheCType(NonZero(1)))]
pub struct OverridesInferredNiche(NonZeroU8);

#[derive(RustSpec, ReprC)]
#[repr_c(NICHE = CustomNicheRequiresUnstableCType(1))]
struct CustomNicheRequiresUnstable(u8);

fn main() {}
