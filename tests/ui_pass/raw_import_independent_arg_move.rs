use co3::ffi;

// Imports of the same symbol choose their argument ownership independently.

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "co3"]

    #[symbol_name = "co3__transform"]
    raw fn transform_raw(value: move u8);
    fn transform(value: u8);
}

fn main() {
    let _ = transform_raw;
    let _ = transform;
}
