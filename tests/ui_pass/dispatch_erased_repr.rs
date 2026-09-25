use co3::{ReprC, Tag, CType, ffi, rust_spec::RustSpec};

trait Attribute {}

trait ByteValue {
    fn into_byte(self) -> u8;
    fn add_ref(&self, rhs: &Self) -> u8;
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct EnvAttr(usize);

#[derive(Clone, RustSpec, Tag, ReprC)]
#[tag(u16, unsafe(1))]
struct Custom1(usize);

#[derive(RustSpec, ReprC, Tag)]
#[tag(u16, unsafe(2))]
#[repr(transparent)]
struct Custom2<'a>(&'a u8);

impl Attribute for Custom1 {}
impl Attribute for Custom2<'_> {}

mod provider {
    use super::{Tag, ffi};

    use super::*;

    #[derive(RustSpec, Tag, ReprC)]
    #[tag(u16, unsafe(1))]
    struct Custom1(usize);

    #[derive(RustSpec, Tag, ReprC)]
    #[tag(u16, unsafe(2))]
    #[repr(transparent)]
    struct Custom2<'a>(&'a u8);

    impl Attribute for Custom1 {}
    impl Attribute for Custom2<'_> {}

    impl ByteValue for u8 {
        fn into_byte(self) -> u8 {
            self
        }

        fn add_ref(&self, rhs: &Self) -> u8 {
            *self + *rhs
        }
    }

    impl ByteValue for Custom1 {
        fn into_byte(self) -> u8 {
            self.0 as u8
        }

        fn add_ref(&self, rhs: &Self) -> u8 {
            (self.0 + rhs.0) as u8
        }
    }

    impl ByteValue for Custom2<'_> {
        fn into_byte(self) -> u8 {
            *self.0
        }

        fn add_ref(&self, rhs: &Self) -> u8 {
            self.0 + rhs.0
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #![symbol_prefix = "kita"]

        impl<dyn(u16) T: Attribute = EnvAttr> ByteValue for T
        where
            use<T> @ (<Custom1> | <Custom2<'_>>),
        {
            fn into_byte(self) -> u8;
            fn add_ref(#[soft] &self, #[soft] rhs: &Self) -> u8;
        }
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    impl<dyn(u16) T: Attribute = EnvAttr> ByteValue for T
    where
        use<T> @ (<Custom1> | <Custom2<'_>>),
    {
        fn into_byte(self) -> u8;
        fn add_ref(#[soft] &self, #[soft] rhs: &Self) -> u8;
    }
}

fn main() {
    assert_eq!(Custom1(2).into_byte(), 2);
    assert_eq!(Custom1(2).add_ref(&Custom1(5)), 7);

    assert_eq!(Custom2(&3).into_byte(), 3);
    assert_eq!(Custom2(&3).add_ref(&Custom2(&4)), 7);
}
