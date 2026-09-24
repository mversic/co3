use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    type BadArg = raw fn(move [u8; 2]);
    type BadReturn = raw fn() -> move [u8; 2];
}

fn main() {}
