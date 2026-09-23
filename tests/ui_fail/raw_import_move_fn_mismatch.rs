use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    raw move fn transform(value: u8) -> u8;
    fn transform(value: u8) -> u8;
}

fn main() {}
