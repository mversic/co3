use co3::ffi;

static WRONG_TYPE: u16 = 1;
static mut WRONG_MUTABILITY: u32 = 2;
static WRONG_MUTABILITY_2: u32 = 3;

ffi! {
    #![unsafe(export("C"))]

    static WRONG_TYPE: u32;
    static WRONG_MUTABILITY: u32;
    static mut WRONG_MUTABILITY_2: u32;
}

fn main() {}
