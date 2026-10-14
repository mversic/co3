use co3::{ffi, rust_spec::RustSpec};

unsafe impl co3::tag::Tagged for OpaqueSlice<u8> {
    const TAG: u8 = 0;
}

#[derive(RustSpec)]
#[rust_spec(opaque)]
struct OpaqueSlice<T>([T]);

ffi! {
    #![unsafe(export("C"))]

    #[tag(u8)]
    type OpaqueSlice<T>;

    impl<T> Drop for dyn OpaqueSlice<T>
    where
        use<T> @ <u8>,
    {
        fn drop(&mut self);
    }
}

fn main() {}
