// FIXME: Support opaque last field DST
//use co3::{ReprC, Tag, ffi, rust_spec::RustSpec};
//
//#[derive(RustSpec, Tag, ReprC)]
//#[tag(u8, unsafe(1))]
//#[repr(transparent)]
//struct Attribute(u32);
//
//#[derive(RustSpec, ReprC)]
//#[repr(C)]
//struct Prefix<T: ?Sized> {
//    head: u32,
//    tail: T,
//}
//
//fn prefix_head<T: ?Sized>(prefix: &Prefix<T>) -> u32 {
//    prefix.head
//}
//
//ffi! {
//    #![unsafe(export("C"))]
//
//    #[symbol_name = "dispatch_opaque_tail_head"]
//    fn prefix_head<'a, dyn(u8) T: ?Sized>(prefix: &'a Prefix<T>) -> u32
//    where
//        use<T> @ <Attribute>;
//}
//
//ffi! {
//    #![unsafe(extern("C"))]
//
//    #[symbol_name = "dispatch_opaque_tail_head"]
//    fn imported_prefix_head<'a, dyn(u8) T: ?Sized>(
//        tag_id: <dyn T>::TAG,
//        prefix: &'a Prefix<T>,
//    ) -> u32
//    where
//        use<T> @ <Attribute>;
//}
//
//fn main() {
//    let prefix = Box::leak(Box::new(Prefix {
//        head: 7,
//        tail: Attribute(9),
//    }));
//
//    assert_eq!(imported_prefix_head(prefix), 7);
//}

fn main() {}
