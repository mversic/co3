use co3::{rust_spec::RustSpec, ReprC};

#[derive(ReprC)]
#[repr_c(identity)]
struct MissingRepr(i32);

#[derive(ReprC)]
#[repr_c(identity)]
#[repr(u8)]
enum Enum {
    Value,
}

#[derive(RustSpec, ReprC)]
#[repr_c(identity)]
#[repr(C)]
struct NonRobust(bool);

#[derive(ReprC)]
#[repr(C)]
#[repr_c(identity, NICHE = 42)]
struct IdentityWithNiche(u32);

#[derive(ReprC)]
#[repr(C)]
#[repr_c(identity, is_valid = |value| *value != 0)]
struct IdentityWithValidation(u32);

#[derive(ReprC)]
#[repr(C)]
#[repr_c(Self)]
struct RemovedSelf(u32);

fn main() {}
