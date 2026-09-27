use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]
    raw extern "C" fn foreign();
}

ffi! {
    #![unsafe(extern("system"))]
    raw extern "C" fn other();
}

fn main() {}
