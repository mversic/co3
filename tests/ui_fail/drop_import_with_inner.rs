use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(C)]
#[rust_spec(custom_drop)]
struct CustomAndInner(Box<u8>);

ffi! {
    #![unsafe(extern("C"))]

    impl Drop for CustomAndInner {
        fn drop(&mut self);
    }
}

fn main() {}
