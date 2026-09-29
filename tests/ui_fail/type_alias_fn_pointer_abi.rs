use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]
    type NestedBare = raw extern "C" fn(fn(u8));
}

ffi! {
    #![unsafe(extern("C"))]
    type NestedRust = raw extern "C" fn() -> extern "Rust" fn(u8);
}

ffi! {
    #![unsafe(extern("C"))]
    type CompositeBare = (u8, raw extern "C" fn(fn(u8)));
}

ffi! {
    #![unsafe(extern("C"))]
    type CompositeRust = (u8, raw extern "Rust" fn(u8));
}

fn main() {}
