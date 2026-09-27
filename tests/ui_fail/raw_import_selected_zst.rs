use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    raw fn selected<T>(value: move T)
    where
        use<T> @ <()>;
}

fn main() {}
