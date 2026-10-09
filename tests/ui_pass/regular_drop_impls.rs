use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec)]
#[rust_spec(custom_drop)]
#[repr(C)]
struct ImportedNonIdentity(u8);

impl ReprC for ImportedNonIdentity {
    type CType = u8;
}

unsafe impl co3::transmute::CheckedTransmute for ImportedNonIdentity {
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}

ffi! {
    #![unsafe(extern("C"))]

    impl Drop for ImportedNonIdentity {
        fn drop(&mut self);
    }
}

#[derive(RustSpec)]
#[rust_spec(custom_drop)]
#[repr(C)]
struct ExportedDrop(u8);

impl ReprC for ExportedDrop {
    type CType = u8;
}

unsafe impl<'d> co3::stored::DecodeOwned<'d> for ExportedDrop {
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Option<Self> {
        Some(Self(source))
    }
}

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
