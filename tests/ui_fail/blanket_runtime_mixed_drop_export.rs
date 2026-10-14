use co3::{ffi, rust_spec::RustSpec};

#[derive(RustSpec)]
#[rust_spec(opaque)]
struct Opaque(u8);
struct Regular(u8);

impl Drop for Opaque {
    fn drop(&mut self) {}
}

impl Drop for Regular {
    fn drop(&mut self) {}
}

ffi! {
    #![unsafe(export("C"))]

    #[tag(u8, unsafe(1))]
    type Opaque;

    impl<dyn(u8) T> Drop for T
    where
        use<T> @ (<Opaque> | <Regular>),
    {
        fn drop(&mut self);
    }
}

fn main() {}
