use core::num::NonZeroU8;

use co3::{CType, ReprC};
use co3::rust_spec::RustSpec;

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[rust_spec(with_custom_niche)]
#[repr_c(NICHE_VALUE = COverridesInferredNiche(1))]
pub struct OverridesInferredNiche(NonZeroU8);

#[derive(ReprC)]
#[repr_c(NICHE_VALUE = CCustomNicheRequiresUnstable(1))]
struct CustomNicheRequiresUnstable(u8);

fn main() {}
