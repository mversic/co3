use co3::{ReprC, Tag, ffi, rust_spec::RustSpec};

#[derive(Clone, RustSpec, Tag, ReprC)]
#[repr(C)]
#[repr_c(with_custom_drop)]
#[tag(u8, unsafe(2))]
struct Regular(u32);

ffi! {
    #![unsafe(extern("C"))]

    #[tag(u8, unsafe(1))]
    type Opaque;

    impl<dyn(u8) T = CRegular> Drop for T
    where
        use<T> @ (<Opaque> | <Regular>),
    {
        fn drop(&mut self);
    }
}

fn main() {}
