use co3::{CType, ReprC};

#[derive(ReprC)]
#[repr_c(identity)]
struct MissingRepr(i32);

#[derive(ReprC)]
#[repr_c(identity)]
#[repr(u8)]
enum Enum {
    Value,
}

#[derive(ReprC)]
#[repr_c(identity)]
#[repr(C)]
struct NonRobust(bool);

fn main() {}
