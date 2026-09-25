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
