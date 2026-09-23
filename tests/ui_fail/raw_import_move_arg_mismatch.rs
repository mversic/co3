use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    raw fn transform(move value: u8);
    fn transform(value: u8);
}

fn main() {}
