use co3::ffi;

#[derive(Debug)]
struct Exported;

ffi! {
    #![unsafe(extern("C"))]

    #[derive(Debug)]
    type Imported;
}

ffi! {
    #![unsafe(export("C"))]

    #[derive(Debug)]
    type Exported;
}

ffi! {
    #![unsafe(extern("C"))]

    #[cfg_attr(all(), derive(Debug))]
    type Conditional;
}

ffi! {
    #![unsafe(extern("C"))]
    #![feature(extern_types)]

    #[derive(Debug)]
    type ExternType;
}

fn main() {}
