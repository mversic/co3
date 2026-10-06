use core::num::NonZeroU8;

use co3::ReprC;
use co3::rust_spec::RustSpec;

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[rust_spec(with_custom_niche)]
#[repr_c(NICHE = COverridesInferredNiche(1))]
pub struct OverridesInferredNiche(NonZeroU8);

#[derive(ReprC)]
#[repr_c(NICHE = CCustomNicheRequiresUnstable(1))]
struct CustomNicheRequiresUnstable(u8);

fn main() {}
