use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(Clone, RustSpec, ReprC)]
#[rust_spec(custom_drop)]
#[repr(C)]
struct ImportedNonIdentity(u8);

ffi! {
    #![unsafe(extern("C"))]

    impl Drop for ImportedNonIdentity {
        fn drop(&mut self);
    }
}

#[derive(Clone, RustSpec, ReprC)]
#[rust_spec(custom_drop)]
#[repr(C)]
struct ExportedDrop(u8);

impl Drop for ExportedDrop {
    fn drop(&mut self) {
        let _ = self.0;
    }
}

ffi! {
    #![unsafe(export("C"))]

    impl Drop for ExportedDrop {
        fn drop(&mut self);
    }
}

fn main() {}
