use co3::{Tag, ReprC, ffi, rust_spec::RustSpec};

trait Dispatch {
    fn me(self);
}

#[derive(RustSpec, Tag, ReprC)]
#[tag(usize, unsafe(1))]
struct Handle(usize);

#[derive(RustSpec, Tag, ReprC)]
#[tag(usize, unsafe(2))]
struct Handle2(u64);

#[derive(RustSpec, Tag, ReprC)]
#[tag(usize, unsafe(3))]
struct Array([u8; 2]);

#[derive(RustSpec, Tag, ReprC)]
#[tag(usize, unsafe(4))]
struct Array2([u8; 8]);

#[derive(RustSpec)]
#[repr(C)]
struct StableButNotCStatic(u32);

mod provider {
    use super::*;

    #[derive(Clone)]
    struct OpaqueZst;

    impl Dispatch for Handle {
        fn me(self) {}
    }
    impl Dispatch for Handle2 {
        fn me(self) {}
    }

    #[expect(unused_variables)]
    pub extern "C" fn disallowed_args_by_ref(arg1: (), arg2: [u8; 2]) {}

    #[expect(unused_variables)]
    pub extern "C" fn disallowed_args_by_val(arg1: (), arg2: [u8; 2]) {}

    #[expect(unused_variables)]
    pub extern "C" fn disallowed_return1() -> [u8; 2] {
        [42, 42]
    }

    #[expect(unused_variables)]
    pub extern "C" fn disallowed_return2() -> Box<[u8]> {
        Box::new([42, 42])
    }

    #[expect(unused_variables)]
    pub extern "C" fn disallowed_opaque_args(arg1: Box<OpaqueZst>) {}
    pub extern "C" fn disallowed_opaque_return() -> Box<OpaqueZst> {
        Box::new(OpaqueZst)
    }

    ffi! {
        #![unsafe(export("C"))]

        type OpaqueZst;

        impl ToOwned for Box<OpaqueZst> {
            #[symbol_name = "my_type_new"]
            fn to_owned(&self) -> <Self as ToOwned>::Owned;
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        extern "C" fn disallowed_args_by_ref(arg1: (), arg2: [u8; 2]);
    }
    ffi! {
        #![unsafe(export("C"))]

        extern "C" fn disallowed_args_by_val(arg1: move (), arg2: move [u8; 2]);
    }

    ffi! {
        #![unsafe(export("C"))]

        extern "C" fn disallowed_return1() -> [u8; 2];
    }
    ffi! {
        #![unsafe(export("C"))]

        extern "C" fn disallowed_return2() -> Box<[u8]>;
    }

    ffi! {
        #![unsafe(export("C"))]

        extern "C" fn disallowed_opaque_args(arg1: Box<OpaqueZst>);
    }
    ffi! {
        #![unsafe(export("C"))]

        extern "C" fn disallowed_opaque_return() -> Box<OpaqueZst>;
    }

    ffi! {
        #![unsafe(export("C"))]

        impl<dyn(usize) T = Array> Dispatch for T
        where
            use<T> @ <Handle>,
        {
            fn me(self);
        }
    }
    ffi! {
        #![unsafe(export("C"))]

        impl<dyn(usize) T = Array2> Dispatch for T
        where
            use<T> @ <Handle2>,
        {
            fn me(self);
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        static UNSTABLE_STATIC: String = String::new();
        static mut NON_C_STATIC: StableButNotCStatic = StableButNotCStatic(0);
    }
}

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "kita"]

    type MyType;

    impl ToOwned for MyType {
        type Owned = OwnedMyType;

        #[symbol_name = "my_type_new"]
        fn to_owned(&self) -> move <Self as ToOwned>::Owned;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    pub extern "C" fn disallowed_args_by_ref(arg1: (), arg2: [u8; 2]);
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    pub extern "C" fn disallowed_args_by_val(arg1: move (), arg2: move [u8; 2]);
}

ffi! {
    #![unsafe(extern("C"))]

    pub extern "C" fn disallowed_return1() -> [u8; 2];
}
ffi! {
    #![unsafe(extern("C"))]

    pub extern "C" fn disallowed_return2() -> Box<[u8]>;
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    pub extern "C" fn disallowed_opaque_args(arg1: OwnedMyType);
}
ffi! {
    #![unsafe(extern("C"))]

    pub extern "C" fn disallowed_opaque_return() -> OwnedMyType;
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    impl<dyn(usize) T = Handle> Dispatch for T
    where
        use<T> @ <Array>,
    {
        fn me(id: <dyn T>::TAG, self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    impl<dyn(usize) T = Handle2> Dispatch for T
    where
        use<T> @ <Array2>,
    {
        fn me(id: <dyn T>::TAG, self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    static UNSTABLE_STATIC: String;
    static mut NON_C_STATIC: StableButNotCStatic;
}

fn main() {}
