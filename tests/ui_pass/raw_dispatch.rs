use co3::{ReprC, Tag, ffi, ops::CFn1, rust_spec::RustSpec};

struct Host;

struct GenericHost<T>(core::marker::PhantomData<T>);

#[derive(Clone, Copy, ReprC, RustSpec, Tag)]
#[tag(u8, unsafe(1))]
#[repr_c(identity)]
#[repr(transparent)]
struct First(u8);

#[derive(Clone, Copy, ReprC, RustSpec, Tag)]
#[tag(u8, unsafe(2))]
#[repr_c(identity)]
#[repr(transparent)]
struct Second(u8);

#[derive(Tag)]
#[tag(u8, unsafe(3))]
struct TagOnly;

#[unsafe(export_name = "raw_dispatch_dynamic")]
extern "C" fn dynamic_provider(tag: u8, value: u8) -> u8 {
    value + tag
}

#[unsafe(export_name = "raw_dispatch_static_u8")]
extern "C" fn static_u8_provider(value: u8) -> u8 {
    value + 1
}

#[unsafe(export_name = "raw_dispatch_static_u16")]
extern "C" fn static_u16_provider(value: u16) -> u16 {
    value + 2
}

#[unsafe(export_name = "raw_dispatch_owned_string")]
extern "C" fn owned_string_provider(
    value: co3::boxed::CBoxedSlice<u8>,
) -> co3::boxed::CBoxedSlice<u8> {
    value
}

#[unsafe(export_name = "raw_dispatch_owned_bytes")]
extern "C" fn owned_bytes_provider(
    value: co3::boxed::CBoxedSlice<u8>,
) -> co3::boxed::CBoxedSlice<u8> {
    value
}

#[unsafe(export_name = "raw_dispatch_method")]
extern "C" fn method_provider(tag: u8, value: u8) -> u8 {
    value + tag + 10
}

#[unsafe(export_name = "raw_dispatch_static_method_u8")]
extern "C" fn static_method_u8(value: u8) -> u8 {
    value + 20
}

#[unsafe(export_name = "raw_dispatch_static_method_u16")]
extern "C" fn static_method_u16(value: u16) -> u16 {
    value + 30
}

#[unsafe(export_name = "raw_dispatch_tag_only")]
extern "C" fn tag_only_provider(tag: u8) -> u8 {
    tag
}

#[unsafe(export_name = "raw_dispatch_mixed_u8")]
extern "C" fn mixed_u8_provider(tag: u8, value: u8, other: u8) -> u8 {
    tag + value + other
}

#[unsafe(export_name = "raw_dispatch_mixed_u16")]
extern "C" fn mixed_u16_provider(tag: u8, value: u8, other: u16) -> u8 {
    tag + value + other as u8
}

#[unsafe(export_name = "raw_dispatch_impl_u8")]
extern "C" fn impl_u8_provider(value: u8) -> u8 {
    value + 40
}

#[unsafe(export_name = "raw_dispatch_impl_u16")]
extern "C" fn impl_u16_provider(value: u16) -> u16 {
    value + 50
}

#[unsafe(export_name = "raw_dispatch_dynamic_impl")]
extern "C" fn dynamic_impl_provider(tag: u8, value: u8) -> u8 {
    value + tag + 70
}

#[unsafe(export_name = "raw_dispatch_const_1")]
extern "C" fn const_1_provider() -> u8 {
    1
}

#[unsafe(export_name = "raw_dispatch_const_2")]
extern "C" fn const_2_provider() -> u8 {
    2
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_fragments {
        String = "string",
        Vec<u8> = "bytes"
    }]

    type StringRaw = raw extern "C" fn(move String) -> move String;
    type BytesRaw = raw extern "C" fn(move Vec<u8>) -> move Vec<u8>;

    #[symbol_name = "raw_dispatch_dynamic"]
    pub raw fn dynamic<dyn(u8) T = u8>(value: move T) -> move T
    where
        use<T> @ (<First> | <Second>);

    #[symbol_name = "raw_dispatch_static_{T}"]
    pub raw fn static_value<T>(value: move T) -> move T
    where
        use<T> @ (<u8> | <u16>);

    #[symbol_name = "raw_dispatch_owned_{T}"]
    pub raw fn owned_value<T>(value: move T) -> move T
    where
        use<T> @ (<String> | <Vec<u8>>);

    #[symbol_name = "raw_dispatch_tag_only"]
    pub raw fn type_directed<dyn(u8) T>() -> u8
    where
        use<T> @ <TagOnly>;

    #[symbol_name = "raw_dispatch_mixed_{U}"]
    pub raw fn mixed<dyn(u8) T = u8, U>(value: move T, other: move U) -> u8
    where
        use<T> @ (<First> | <Second>),
        use<U> @ (<u8> | <u16>);

    #[symbol_name = "raw_dispatch_const_{N}"]
    pub raw fn const_selected<const N: usize>() -> u8
    where
        use<N> @ (<1> | <2>);

    impl Host {
        #[symbol_name = "raw_dispatch_method"]
        pub raw fn method<dyn(u8) T = u8>(value: move T) -> move T
        where
            use<T> @ (<First> | <Second>);

        #[symbol_name = "raw_dispatch_static_method_{T}"]
        pub raw fn static_method<T>(value: move T) -> move T
        where
            use<T> @ (<u8> | <u16>);
    }

    impl<T> GenericHost<T>
    where
        use<T> @ (<u8> | <u16>)
    {
        #[symbol_name = "raw_dispatch_impl_{T}"]
        pub raw fn from_value(value: move T) -> move T;
    }

    impl<dyn(u8) T = u8> GenericHost<T>
    where
        use<T> @ (<First> | <Second>),
    {
        #[symbol_name = "raw_dispatch_dynamic_impl"]
        pub raw fn dynamic_from_value(value: move T) -> move T;
    }
}

// The selected raw impls must leave other instantiations available for ordinary methods.
impl GenericHost<u32> {
    fn from_value(value: u32) -> u32 {
        value + 60
    }
}

fn main() {
    let string_raw: StringRaw = owned_value::<String>;
    let bytes_raw: BytesRaw = owned_value::<Vec<u8>>;

    let str_input = String::from("hello");
    let vec_input = vec![1_u8, 2, 3];

    let string: String = unsafe { string_raw.call(str_input) }.unwrap();
    let bytes: Vec<u8> = unsafe { bytes_raw.call(vec_input) }.unwrap();

    assert_eq!(string, "hello");
    assert_eq!(bytes, [1, 2, 3]);

    assert_eq!(unsafe { dynamic::<First>(First(3)) }.0, 4);
    assert_eq!(unsafe { dynamic::<Second>(Second(3)) }.0, 5);
    assert_eq!(unsafe { static_value::<u8>(3) }, 4);
    assert_eq!(unsafe { static_value::<u16>(3) }, 5);

    assert_eq!(unsafe { type_directed::<TagOnly>() }, 3);
    assert_eq!(unsafe { mixed::<First, u8>(First(2), 3) }, 6);
    assert_eq!(unsafe { mixed::<Second, u16>(Second(2), 3) }, 7);
    assert_eq!(unsafe { const_selected::<1>() }, 1);
    assert_eq!(unsafe { const_selected::<2>() }, 2);
    assert_eq!(unsafe { Host::method::<First>(First(3)) }.0, 14);
    assert_eq!(unsafe { Host::method::<Second>(Second(3)) }.0, 15);
    assert_eq!(unsafe { Host::static_method::<u8>(3) }, 23);
    assert_eq!(unsafe { Host::static_method::<u16>(3) }, 33);
    assert_eq!(unsafe { GenericHost::<u8>::from_value(3) }, 43);
    assert_eq!(unsafe { GenericHost::<u16>::from_value(3) }, 53);
    assert_eq!(GenericHost::<u32>::from_value(3), 63);
    assert_eq!(unsafe { GenericHost::<First>::dynamic_from_value(First(3)) }.0, 74);
    assert_eq!(unsafe { GenericHost::<Second>::dynamic_from_value(Second(3)) }.0, 75);
}
