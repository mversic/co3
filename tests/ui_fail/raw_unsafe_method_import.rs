use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    type Host;

    impl Host {
        raw unsafe fn target();
    }
}

fn main() {}
