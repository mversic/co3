use co3::ffi;

fn target() {}

ffi! {
    #![unsafe(extern("C"))]

    raw unsafe fn target();
}

ffi! {
    #![unsafe(extern("C"))]

    type Host;

    impl Host {
        raw unsafe fn target();
    }
}

fn main() {}
