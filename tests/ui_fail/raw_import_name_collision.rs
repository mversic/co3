use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    fn foreign(value: u8) -> u8;
    raw fn foreign(value: u8) -> u8;
}

fn main() {}
