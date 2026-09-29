use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]
    type Bare = fn(u8);
}

ffi! {
    #![unsafe(extern("C"))]
    type Implicit = extern fn(u8);
}

ffi! {
    #![unsafe(extern("C"))]
    type Rust = extern "Rust" fn(u8);
}

ffi! {
    #![unsafe(extern("C"))]
    type Nested = Option<fn(u8)>;
}

ffi! {
    #![unsafe(extern("C"))]
    type NestedRust = Option<extern "Rust" fn(u8)>;
}

ffi! {
    #![unsafe(extern("C"))]
    type RawNested = raw extern "C" fn(fn(u8));
}

fn main() {}
