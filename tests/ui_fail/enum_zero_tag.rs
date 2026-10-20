use co3::{ReprC, rust_spec::RustSpec};

// A zero-tag variant must carry the attribute, whether its tag is implicit or explicit.
#[derive(RustSpec, ReprC)]
#[repr(u8)]
enum ImplicitZero {
    First,
    Second,
}

#[derive(RustSpec, ReprC)]
#[repr(u8)]
enum ExplicitZero {
    First = 0,
    Second,
}

#[derive(RustSpec, ReprC)]
#[repr(u8)]
enum LaterZero {
    First = 1,
    Second = 0,
    Third = 2,
}

#[derive(RustSpec, ReprC)]
#[repr(u8)]
enum DataFirstZero {
    A(u32),
    B,
}

#[derive(RustSpec, ReprC)]
#[repr(C, u8)]
enum DataLaterZero {
    A(u32) = 1,
    B = 0,
    C = 2,
}

// The attribute must be on a zero-tag variant.
#[derive(RustSpec, ReprC)]
#[repr(u8)]
enum MarkedWrongVariant {
    #[repr_c(allow_zero_tag)]
    First = 1,
    Second = 0,
}

#[derive(RustSpec, ReprC)]
#[repr(u8)]
enum NoZeroTag {
    #[repr_c(allow_zero_tag)]
    First = 1,
    Second = 2,
}

// The attribute belongs on a variant of an enum that can reserve a niche.
#[derive(ReprC)]
#[repr_c(allow_zero_tag)]
enum MarkerOnEnum {
    First,
    Second,
}

#[derive(ReprC)]
#[repr(transparent)]
enum TransparentPayload {
    #[repr_c(allow_zero_tag)]
    Only(u8),
}

#[derive(ReprC)]
enum SingleFieldless {
    #[repr_c(allow_zero_tag)]
    Only,
}

#[derive(ReprC)]
#[repr(u8)]
enum SingleFieldlessWithTag {
    #[repr_c(allow_zero_tag)]
    Only,
}

fn main() {}
