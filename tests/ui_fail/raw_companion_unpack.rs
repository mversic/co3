use co3::{ffi, raw};

struct Host;

fn process(value: (u8, u8)) {
    let _ = value;
}

raw! {
    fn process(#[unpack] value: (u8, u8));
}

ffi! {
    #![unsafe(extern("C"))]

    raw fn missing_parts(#[unpack] value: (u8, u8));

    impl Host {
        raw fn missing_method_parts(#[unpack] value: (u8, u8));
    }
}

fn main() {}
