use std::ops::Deref;

use co3::{ReprC, Tag, CType, ffi, rust_spec::RustSpec};

trait ExportDispatchTrait {
    fn dispatch(&self, arg: &u32);
}

struct ExportImpl;

#[derive(RustSpec, Tag, ReprC)]
#[tag(u8, unsafe(0))]
#[repr(transparent)]
struct DriftHandle(u32);

fn export_fn(_: &u32) {}

impl ExportImpl {
    fn method(_: &u32) {}
}

impl Deref for DriftHandle {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ExportDispatchTrait for DriftHandle {
    fn dispatch(&self, _: &u32) {}
}

ffi! {
    #![unsafe(export("C"))]

    impl ExportImpl {
        fn method(arg: &Box<u32>);
    }

    fn export_fn(arg: &Box<u32>);
}

ffi! {
    #![unsafe(export("C"))]

    impl<dyn(u8) T = DriftHandle> ExportDispatchTrait for T
    where
        use<T> @ <DriftHandle>,
    {
        fn dispatch(&self, arg: &T);
    }
}

fn main() {}
