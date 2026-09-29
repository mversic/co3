use co3::ffi;

ffi! {
    #![unsafe(export("C"))]
    type Alias = raw extern "Rust" fn();
}

fn main() {}
