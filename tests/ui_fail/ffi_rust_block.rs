use co3::ffi;

ffi! {
    #![unsafe(export("Rust"))]
}

ffi! {
    #![unsafe(extern("Rust"))]
}

fn main() {}
