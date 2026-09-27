use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "bad_{T}"]
    fn bad<T>(value: ())
    where
        use<T> @ (<u8> | <u16>);
}

fn main() {}
