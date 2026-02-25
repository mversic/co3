use co3::{ReprC, export, extern_, extern_C};

#[derive(ReprC)]
pub struct Hello {
    a: i32,
    b: i32,
}

#[export("C")]
impl Hello {
    #[export(name = "hello")]
    #[expect(improper_ctypes_definitions)]
    pub extern "C" fn hello(Hello { a: a1, b: b1 }: Hello, Hello { a: a2, b: b2 }: Hello) -> i32 {
        a1 + b1 + a2 + b2
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[link_name = "hello"]
    #[expect(improper_ctypes_definitions)]
    pub extern "C" fn hello2(a: Hello, b: Hello) -> i32;
}

extern_! {
    #![link(crate = "kita")]
    #![abi = "kita"]

    #[link_name = "hello"]
    #[expect(improper_ctypes_definitions)]
    pub extern "C" fn hello3(a: Hello, b: Hello) -> i32;
}

extern_! {
    #![abi = "kita"]
    #![link(crate = "kita")]

    #[link_name = "hello"]
    #[expect(improper_ctypes_definitions)]
    pub extern "C" fn hello4(a: Hello, b: Hello) -> i32;
}

extern_! {
    #![abi = "kita"]
    #![link(crate = "kita")]

    #[link_name = "hello"]
    #[expect(improper_ctypes_definitions)]
    pub extern "C" fn hello5(a: Hello, b: Hello) -> i32;
}

extern_! {
    #![abi = "kita"]
    #![link(crate = "kita")]

    #[link_name = "hello"]
    #[expect(improper_ctypes_definitions)]
    pub extern "C" fn hello6(a: Hello, b: Hello) -> i32;
}

fn main() {
    Hello::hello(Hello { a: 1, b: 2 }, Hello { a: 1, b: 2 });
    hello2(Hello { a: 1, b: 2 }, Hello { a: 1, b: 2 });
    hello3(Hello { a: 1, b: 2 }, Hello { a: 1, b: 2 });
    hello4(Hello { a: 1, b: 2 }, Hello { a: 1, b: 2 });
    hello5(Hello { a: 1, b: 2 }, Hello { a: 1, b: 2 });
    hello6(Hello { a: 1, b: 2 }, Hello { a: 1, b: 2 });
}
