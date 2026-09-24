use co3::{Tag, ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec, ReprC, Tag)]
#[tag(u8, unsafe(1))]
#[repr(transparent)]
struct Value(u8);

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "layout_mismatch"]
    fn layout_mismatch<dyn(u8) T = u16>(value: move T)
    where
        use<T> @ <Value>;
}

fn main() {
    layout_mismatch(Value(1));
}
