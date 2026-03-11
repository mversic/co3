use co3::{ReprC, export, extern_, extern_C};

#[derive(Clone, ReprC)]
pub struct Hello {
    a: i32,
    b: i32,
}

#[export("C")]
impl Hello {
    #[export(name = "hello")]
    #[expect(improper_ctypes_definitions)]
    pub extern "C" fn hello(
        Hello { a: a1, b: b1 }: Hello,
        Hello { a: a2, b: b2 }: Hello,
    ) -> i32 {
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
    #![abi = "C"]

    #[link_name = "hello"]
    #[expect(improper_ctypes_definitions)]
    pub extern "C" fn hello3(a: Hello, b: Hello) -> i32;
}

fn main() {
    let value = Hello { a: 1, b: 2 };
    Hello::hello(value.clone(), value.clone());

    hello2(value.clone(), value.clone());
    hello3(value.clone(), value.clone());
}
