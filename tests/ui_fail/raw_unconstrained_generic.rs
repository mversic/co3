use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    raw fn unconstrained<T>(value: move T);
}

fn main() {}
