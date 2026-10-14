use co3::{ReprC, ffi, rust_spec::RustSpec};

ffi! {
    #![unsafe(extern("C"))]

    type Opaque;
}

#[derive(RustSpec, ReprC)]
struct Kita1(Opaque);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Kita2(Opaque);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Kita3(Opaque);

#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
#[repr(transparent)]
struct Kita4(Opaque);

#[derive(RustSpec, ReprC)]
#[repr_c(identity)]
#[repr(C)]
struct Kita5(u32, Opaque);

fn main() {}
