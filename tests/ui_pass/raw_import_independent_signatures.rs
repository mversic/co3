use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "shared"]
    raw fn raw_one(value: u8);

    #[symbol_name = "shared"]
    fn rust_one(value: u16, extra: u16) -> u16;
}

fn main() {
    let _ = raw_one;
    let _ = rust_one;
}
