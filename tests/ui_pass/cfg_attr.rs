use co3::{ReprC, Tag, ffi, rust_spec::RustSpec};

trait Attr {}

#[derive(Clone, RustSpec, ReprC)]
#[repr(transparent)]
struct Value(u8);

trait ByteValue {
    fn into_byte(self) -> u8;
}

#[derive(Clone, RustSpec, ReprC)]
#[repr(transparent)]
struct EnvAttr(usize);

#[derive(Clone, RustSpec, Tag, ReprC)]
#[tag(u16, unsafe(1))]
struct Custom(usize);

impl Attr for Custom {}

fn value_plain(value: Value) -> u8 {
    value.0
}

fn value_soft(value: &(u8,)) -> u8 {
    value.0
}

ffi! {
    #![unsafe(export("C"))]

    #![cfg(all())]
    #![cfg_attr(all(), cfg(all()))]
    #![cfg_attr(any(), symbol_prefix = "unused")]
    #![cfg_attr(all(), symbol_prefix = "cfg_attr")]

    #[cfg(all())]
    #[cfg_attr(any(), symbol_name = "unused")]
    #[cfg_attr(all(), symbol_name = "cfg_attr__value_plain")]
    fn value_plain(#[cfg(all())] value: Value) -> u8;
}

ffi! {
    #![unsafe(extern("C"))]

    #![cfg_attr(any(), symbol_prefix = "unused")]
    #![cfg_attr(all(), symbol_prefix = "cfg_attr")]

    #[cfg_attr(any(), symbol_name = "unused")]
    #[cfg_attr(all(), symbol_name = "cfg_attr__value_plain")]
    fn imported_value_plain(value: Value) -> u8;
}

ffi! {
    #![unsafe(extern("C"))]

    #[cfg(any())]
    fn disabled_static<T>()
    where
        use<T> @ <u8>;
}

ffi! {
    #![cfg_attr(all(), unsafe(export("C")))]

    #[symbol_name = "cfg_attr__value_soft"]
    fn value_soft(#[cfg_attr(all(), soft)] value: &(u8,)) -> u8;
}

ffi! {
    #![cfg_attr(all(), unsafe(extern("C")))]

    #[symbol_name = "cfg_attr__value_soft"]
    fn imported_value_soft(#[cfg_attr(all(), soft)] value: &(u8,)) -> u8;
}

struct ExportOpaque(u8);

ffi! {
    #![unsafe(export("C"))]

    #![symbol_prefix = "cfg_attr"]

    #[cfg(all())]
    #[cfg_attr(all(), tag(u8))]
    type ExportOpaque;
}

mod imported {
    use super::*;

    ffi! {
        #![unsafe(extern("C"))]

        #![symbol_prefix = "cfg_attr"]

        #[cfg_attr(all(), tag(u8))]
        type Opaque;
    }
}

mod provider {
    use super::*;

    #[derive(Clone, RustSpec, ReprC, Tag)]
    #[tag(u16, unsafe(1))]
    pub(super) struct Custom(usize);

    impl Attr for Custom {}

    impl ByteValue for Custom {
        fn into_byte(self) -> u8 {
            self.0 as u8
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #![symbol_prefix = "cfg_attr_dispatch"]

        #[cfg(all())]
        impl<dyn(u16) T: Attr = EnvAttr> ByteValue for T
        where
            use<T> @ <Custom>,
        {
            #[cfg(all())]
            fn into_byte(self) -> u8;
        }
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "cfg_attr_dispatch"]

    #[cfg(all())]
    impl<dyn(u16) T: Attr = EnvAttr> ByteValue for T
    where
        use<T> @ <Custom>,
    {
        #[cfg(all())]
        fn into_byte(tag_id: <dyn T>::TAG, self) -> u8;
    }
}

fn main() {
    assert_eq!(imported_value_plain(Value(3)), 3);
    assert_eq!(imported_value_soft(&(4,)), 4);
    assert_eq!(Custom(5).into_byte(), 5);
}
