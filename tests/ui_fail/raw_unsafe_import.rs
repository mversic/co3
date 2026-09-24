use co3::ffi;

fn target() {}

ffi! {
    #![unsafe(extern("C"))]

    raw unsafe fn target();
}

fn main() {}
