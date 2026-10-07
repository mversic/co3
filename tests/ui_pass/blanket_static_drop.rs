use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(Clone, RustSpec, ReprC)]
#[repr(C)]
#[repr_c(with_custom_drop)]
struct Regular(u32);

#[derive(Clone, RustSpec, ReprC)]
#[repr(C)]
#[repr_c(with_custom_drop)]
struct RegularExport(u32);

impl Drop for RegularExport {
    fn drop(&mut self) {}
}

struct ExportA(u8);
struct ExportB(u8);

impl Drop for ExportA {
    fn drop(&mut self) {}
}

impl Drop for ExportB {
    fn drop(&mut self) {}
}

ffi! {
    #![unsafe(export("C"))]

    type ExportA;
    type ExportB;

    impl<T> Drop for T
    where
        use<T> @ (<ExportA> | <ExportB> | <RegularExport>),
    {
        fn drop(&mut self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    type ImportA;
    type ImportB;

    impl<T> Drop for T
    where
        use<T> @ (<ImportA> | <ImportB> | <Regular>),
    {
        fn drop(&mut self);
    }
}

fn main() {}
