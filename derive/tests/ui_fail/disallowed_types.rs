use co3::extern_C;

mod provider {
    use co3::export_C;

    #[derive(Clone)]
    struct MyType;

    #[allow(unused_variables)]
    pub extern "C" fn disallowed_types_by_ref(
        arg1: (),
        arg2: [u8; 2],
        arg3: (),
        arg4: [u8; 2],
    ) -> () {
    }

    pub extern "C" fn disallowed_types_by_val(
        #[allow(unused_variables)] arg1: (),
        arg2: [u8; 2],
    ) -> [u8; 2] {
        arg2
    }

    pub extern "C" fn disallowed_opaque_types(
        #[allow(unused_variables)] arg1: Box<MyType>,
        #[allow(unused_variables)] arg2: Box<MyType>,
    ) {
    }

    export_C! {
        type MyType;

        impl ToOwned for Box<MyType> {
            #[unsafe(export_name = "my_type_new")]
            fn to_owned(&self) -> <Self as ToOwned>::Owned;
        }

        pub extern "C" fn disallowed_types_by_ref(arg1: (), arg2: [u8; 2], move arg3: (), move arg4: [u8; 2]) -> ();
        pub extern "C" fn disallowed_types_by_val(move arg1: (), move arg2: [u8; 2]) -> [u8; 2];
        pub extern "C" fn disallowed_opaque_types(move arg1: Box<MyType>, arg2: Box<MyType>);
    }
}

extern_C! {
    #![link(crate = "kita")]

    type MyType;

    impl ToOwned for MyType {
        type Owned = OwnedMyType;

        #[link_name = "my_type_new"]
        fn to_owned(&self) -> <Self as ToOwned>::Owned;
    }

    pub extern "C" fn disallowed_types_by_ref(arg1: (), arg2: [u8; 2]) -> ();
    pub extern "C" fn disallowed_types_by_val(move arg1: (), move arg2: [u8; 2]) -> [u8; 2];
    pub extern "C" fn disallowed_opaque_types(move arg1: OwnedMyType, arg2: OwnedMyType);
}

fn main() {}
