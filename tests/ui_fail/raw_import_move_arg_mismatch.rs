use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    raw fn transform(value: move u8);
    fn transform(value: u8);
}

fn main() {}
