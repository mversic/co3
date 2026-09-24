use co3::{Tag, ReprC, ffi, rust_spec::RustSpec};

trait Version {
    type Previous: Version;
}

enum Version1 {}

impl Version for Version1 {
    type Previous = Self;
}

trait Attribute {}

#[derive(Clone, Copy, RustSpec, ReprC, Tag)]
#[tag(u8, unsafe(1))]
#[repr(transparent)]
struct Attr(u8);

impl Attribute for Attr {}

ffi! {
    #![unsafe(extern("C"))]

    #[tag(u8, unsafe(2))]
    type Statement<'stmt, V: Version>;

    impl<V: Version> Drop for dyn Statement<'_, V> {
        fn drop(&mut self);
    }

    impl<'stmt, V> Statement<'stmt, V>
    where
        V: Version,
    {
        fn set_attr<dyn(u8) A: Attribute = u8>(&self, attr: move A)
        where
            use<A> @ <Attr>;
    }
}

fn main() {}
