use co3::{ffi, rust_spec::RustSpec};

fn local_function() {}

extern "system" fn local_system(value: u8) -> u8 {
    value
}

#[derive(RustSpec)]
#[rust_spec(opaque)]
struct Local(u8);

impl Local {
    fn method(&self) {}
}

ffi! {
    #![unsafe(export("C"))]

    raw fn local_function();
    raw extern "system" fn local_system(value: u8) -> u8;

    type Local;

    impl Local {
        raw fn method(&self);
    }
}

fn main() {}
