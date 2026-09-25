use co3::{ReprC, Tag, CType, ffi, rust_spec::RustSpec};

#[derive(Tag, ReprC, RustSpec)]
#[tag(u8, unsafe(1))]
#[repr(transparent)]
struct Target(u8);

ffi! {
    #![unsafe(extern("C"))]

    #[tag(u8, unsafe(2))]
    type Host<'buf>;

    impl<'buf> Host<'buf> {
        fn bind<dyn(u8) T>(&'buf mut self, value: &'buf mut u8)
        where
            use<T> @ <Target>;

        fn bind_static<T>(&'buf mut self, value: &'buf mut u8)
        where
            use<T> @ <Target>;
    }
}

fn main() {}
