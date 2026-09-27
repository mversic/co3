use co3::ffi;

fn borrowed_generic<'a, T>(value: &'a T) -> &'a T {
    value
}

ffi! {
    #![unsafe(export("C"))]

    fn borrowed_generic<'a, T>(value: &'a T) -> &'a T
    where
        use<T> @ <u8>;
}

fn main() {}
