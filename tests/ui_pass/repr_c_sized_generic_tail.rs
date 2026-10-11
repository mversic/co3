use co3::{ReprC, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Packet<T> {
    header: u32,
    payload: T,
}

fn main() {
    let packet = Packet {
        header: 1,
        payload: 2u32,
    };
    let _ = packet.header;
}
