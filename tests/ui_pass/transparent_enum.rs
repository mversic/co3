use co3::{ReprC, rust_spec::RustSpec};

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(transparent)]
enum TransparentTupleEnum {
    A(u8),
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(transparent)]
enum TransparentNamedEnum {
    A { value: u8 },
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(transparent)]
enum TransparentMultipleFieldsEnum {
    A((), u8),
}

#[derive(Clone, Copy, RustSpec, ReprC)]
enum ImplicitTransparentTupleEnum {
    A(u8),
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr_c(transparent)]
enum ExplicitTransparentTupleEnum {
    A(u8),
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr_c(transparent)]
enum ExplicitTransparentNamedEnum {
    A { value: u8 },
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr_c(transparent)]
enum ExplicitTransparentUnitEnum {
    A,
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr_c(transparent)]
#[repr(u8)]
enum ExplicitTransparentFieldlessEnum {
    #[repr_c(allow_zero_tag)]
    A = 0,
    B = 1,
}

static_assertions::assert_type_eq_all!(<ExplicitTransparentTupleEnum as ReprC>::CType, u8);
static_assertions::assert_type_eq_all!(<ExplicitTransparentNamedEnum as ReprC>::CType, u8);
static_assertions::assert_type_eq_all!(<ExplicitTransparentUnitEnum as ReprC>::CType, ());
static_assertions::assert_type_eq_all!(<ExplicitTransparentFieldlessEnum as ReprC>::CType, u8);

const _: () = assert!(
    core::mem::size_of::<<TransparentTupleEnum as ReprC>::CType>() == core::mem::size_of::<u8>()
);

const _: () = assert!(
    core::mem::size_of::<<TransparentNamedEnum as ReprC>::CType>() == core::mem::size_of::<u8>()
);

const _: () = assert!(
    core::mem::size_of::<<TransparentMultipleFieldsEnum as ReprC>::CType>()
        == core::mem::size_of::<u8>()
);

const _: () = assert!(
    core::mem::size_of::<<ImplicitTransparentTupleEnum as ReprC>::CType>()
        == core::mem::size_of::<u8>()
);

fn main() {}
