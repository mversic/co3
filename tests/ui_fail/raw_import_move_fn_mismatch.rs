use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    raw fn transform(value: u8) -> move u8;
    fn transform(value: u8) -> u8;
}

fn main() {}
