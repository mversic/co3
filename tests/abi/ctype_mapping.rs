use co3::{CType, ReprC, rust_spec::RustSpec};
use core::num::NonZeroU32;
use core::{marker::PhantomData, mem::MaybeUninit};
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Plain(u32);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Generic<T>(T);

#[derive(RustSpec, ReprC)]
struct Unstable(u32);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct HasTrap(bool);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct NonCTypeField(MaybeUninit<NonZeroU32>);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct GenericNonCTypeField<T>(MaybeUninit<NonZeroU32>, PhantomData<T>);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct ConditionalField {
    value: u32,
    #[cfg(any())]
    missing: MissingType,
}

#[derive(RustSpec, ReprC)]
#[repr(C)]
#[rust_spec(custom_drop)]
struct HasDrop(u32);

impl Drop for HasDrop {
    fn drop(&mut self) {}
}

type Callback = extern "C" fn(u8) -> u8;

assert_impl_all!(Option<NonZeroU32>: CType);
assert_impl_all!(Result<NonZeroU32, ()>: CType);
assert_impl_all!(Result<(), NonZeroU32>: CType);
assert_impl_all!(Option<Callback>: CType);
assert_impl_all!(Option<&u32>: CType);
assert_impl_all!(Plain: CType);
assert_impl_all!(Generic<u32>: CType);
assert_impl_all!(ConditionalField: CType);

assert_not_impl_any!(Option<u32>: CType);
assert_not_impl_any!(Option<Box<u32>>: CType);
assert_not_impl_any!(Result<u32, u32>: CType);
assert_not_impl_any!(Unstable: CType);
assert_not_impl_any!(Generic<bool>: CType);
assert_not_impl_any!(HasTrap: CType);
assert_not_impl_any!(MaybeUninit<NonZeroU32>: CType);
assert_not_impl_any!(NonCTypeField: CType);
assert_not_impl_any!(GenericNonCTypeField<()>: CType);
assert_not_impl_any!(HasDrop: CType);
