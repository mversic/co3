use co3::{ReprC, Tag, raw, rust_spec::RustSpec};

#[derive(Clone, Copy, ReprC, RustSpec, Tag)]
#[tag(u8, unsafe(1))]
#[repr_c(identity)]
#[repr(transparent)]
struct First(u8);

fn source<T>(value: T) -> T {
    value
}

raw! {
    fn source<dyn(u8) T = u8>(value: move T) -> u8
    where use<T> @ <First>;
}

fn main() {}
