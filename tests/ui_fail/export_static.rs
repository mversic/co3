use co3::ffi;

static mut FLAGS: u32 = 0;

ffi! {
    #![unsafe(export("C"))]

    static mut FLAGS: u32 = 0;
}

fn main() {}
