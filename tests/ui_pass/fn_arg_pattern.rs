use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(Clone, RustSpec, ReprC)]
pub struct Hello {
    a: i32,
    b: i32,
}

impl Hello {
    #[expect(improper_ctypes_definitions)]
    pub extern "C" fn hello(Hello { a: a1, b: b1 }: Hello, Hello { a: a2, b: b2 }: Hello) -> i32 {
        a1 + b1 + a2 + b2
    }
}

ffi! {
    #![unsafe(export("C"))]

    impl Hello {
        #[symbol_name = "hello"]
        extern "C" fn hello(a: Hello, b: Hello) -> i32;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    #[symbol_name = "hello"]
    #[expect(improper_ctypes_definitions)]
    pub extern "C" fn hello2(a: Hello, b: Hello) -> i32;
}

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "hello"]
    #[expect(improper_ctypes_definitions)]
    pub extern "C" fn hello3(a: Hello, b: Hello) -> i32;
}

fn main() {
    let value = Hello { a: 1, b: 2 };
    Hello::hello(value.clone(), value.clone());

    hello2(value.clone(), value.clone());
    hello3(value.clone(), value.clone());
}
