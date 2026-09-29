use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    fn foreign(value: u8) -> u8;
    raw fn foreign(value: u8) -> u8;
}

struct Api;

ffi! {
    #![unsafe(extern("C"))]

    impl Api {
        fn method();
        raw fn method();
    }
}

fn main() {}
