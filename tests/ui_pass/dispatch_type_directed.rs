use co3::{ReprC, Tag, CType, ffi, rust_spec::RustSpec};

trait Kind {
    fn code() -> u8;
}

#[derive(RustSpec, Tag, ReprC)]
#[tag(u8, unsafe(1))]
#[repr(transparent)]
struct First(u8);

#[derive(RustSpec, Tag, ReprC)]
#[tag(u8, unsafe(2))]
#[repr(transparent)]
struct Second(u8);

impl Kind for First {
    fn code() -> u8 {
        1
    }
}

impl Kind for Second {
    fn code() -> u8 {
        2
    }
}

fn type_directed<T: Kind>() -> u8 {
    T::code()
}

fn unused_generic_export<T>() {}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Host(u8);

ffi! {
    #![unsafe(export("C"))]

    fn unused_generic_export<dyn(u8) T>()
    where
        use<T> @ <First>;

    fn type_directed<dyn(u8) T: Kind = u8>() -> u8
    where
        use<T> @ (<First> | <Second>);
}

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "dispatch_type_directed"]

    pub fn unused_generic_import<dyn(u8) T>()
    where
        use<T> @ <First>;

    pub fn unused_generic<dyn(u8) T = u8, dyn(u8) U = u8>()
    where
        use<T, U> @ (<First, First> | <Second, Second>);

    impl Host {
        pub fn unused_generic_method<dyn(u8) T = u8, dyn(u8) U = u8>(&self)
        where
            use<T, U> @ (<First, First> | <Second, Second>);
    }
}

fn main() {}
