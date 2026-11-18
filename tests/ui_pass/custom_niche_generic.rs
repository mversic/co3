use core::{marker::PhantomData, num::NonZeroU8};

use co3::{ReprC, niche::Niche, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[rust_spec(custom_niche)]
#[repr_c(NICHE = unsafe { core::mem::zeroed() })]
struct Valid<T>(u8, PhantomData<T>);

#[derive(RustSpec, ReprC)]
#[repr_c(NICHE = unsafe { core::mem::zeroed() })]
struct MissingCustomNiche<T>(u8, PhantomData<T>);

#[derive(RustSpec, ReprC)]
#[rust_spec(custom_niche)]
#[repr_c(NICHE = unsafe { core::mem::zeroed() })]
struct OverridesInferredNiche<T>(NonZeroU8, PhantomData<T>);

#[derive(RustSpec, ReprC)]
#[rust_spec(custom_niche)]
#[repr_c(NICHE = unsafe { core::mem::zeroed() })]
struct MaybeUnsized<T: ?Sized>(T);

static_assertions::assert_impl_all!(MaybeUnsized<u8>: Niche);
static_assertions::assert_not_impl_any!(MaybeUnsized<[u8]>: Niche);
static_assertions::assert_not_impl_any!(MaybeUnsized<str>: Niche);

static_assertions::assert_impl_all!(Valid<()>: Niche);
static_assertions::assert_not_impl_any!(MissingCustomNiche<()>: Niche);
static_assertions::assert_not_impl_any!(OverridesInferredNiche<()>: Niche);

fn main() {}
