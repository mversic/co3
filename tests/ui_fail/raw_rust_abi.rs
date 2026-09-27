use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]
    raw extern "Rust" fn imported();
}

ffi! {
    #![unsafe(export("C"))]
    type Alias = raw extern "Rust" fn();
}

ffi! {
    #![unsafe(export("C"))]
    type Callback = raw extern "Rust" fn();
}

fn main() {}
