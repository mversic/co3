use core::{marker::PhantomData, num::NonZeroU8};

use co3::ReprC;
use co3::rust_spec::RustSpec;

#[derive(RustSpec, ReprC)]
#[rust_spec(with_custom_niche)]
#[repr_c(NICHE = unsafe { core::mem::zeroed() })]
struct Valid<T>(u8, PhantomData<T>);

#[derive(RustSpec, ReprC)]
#[repr_c(NICHE = unsafe { core::mem::zeroed() })]
struct MissingCustomNiche<T>(u8, PhantomData<T>);

#[derive(RustSpec, ReprC)]
#[rust_spec(with_custom_niche)]
#[repr_c(NICHE = unsafe { core::mem::zeroed() })]
struct OverridesInferredNiche<T>(NonZeroU8, PhantomData<T>);

static_assertions::assert_impl_all!(Valid<()>: co3::niche::Niche);
static_assertions::assert_not_impl_any!(MissingCustomNiche<()>: co3::niche::Niche);
static_assertions::assert_not_impl_any!(OverridesInferredNiche<()>: co3::niche::Niche);

fn main() {}
