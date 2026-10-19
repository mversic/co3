use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[rust_spec(custom_drop)]
#[repr(C)]
struct ImportedSoft(u8);

ffi! {
    #![unsafe(extern("C"))]

    impl Drop for ImportedSoft {
        fn drop(#[soft] &mut self);
    }
}

#[derive(RustSpec, ReprC)]
#[rust_spec(custom_drop)]
#[repr(C)]
struct ExportedSoft(u8);

impl Drop for ExportedSoft {
    fn drop(&mut self) {}
}

ffi! {
    #![unsafe(export("C"))]

    impl Drop for ExportedSoft {
        fn drop(#[soft] &mut self);
    }
}

fn main() {}
