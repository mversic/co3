use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    type ExplicitAbi = raw extern "C" fn(u8);
}

fn main() {}
