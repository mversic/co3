use co3::ffi;

ffi! {
    #![unsafe(export("C"))]

    static VERSION: u32;
    static mut FLAGS: u32 = 0;
}

fn main() {}
