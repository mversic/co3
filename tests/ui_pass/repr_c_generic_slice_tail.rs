use co3::{ReprC, rust_spec::RustSpec, wide::Wide};

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Packet<T> {
    id: u32,
    payload: [T],
}

fn main() {
    fn assert_wide<T: Wide + ?Sized>() {}
    assert_wide::<Packet<u8>>();
    let _: Option<&Packet<u8>> = None;
}
