use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    type Callback = raw "system" fn(u8);
}

ffi! {
    #![unsafe(extern("C"))]

    raw "system" fn foreign(value: u8);
}

fn main() {}
