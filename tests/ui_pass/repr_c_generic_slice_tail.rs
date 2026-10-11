use co3::{ReprC, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Packet<T> {
    id: u32,
    payload: [T],
}

fn main() {
    let _: Option<&Packet<u8>> = None;
}
