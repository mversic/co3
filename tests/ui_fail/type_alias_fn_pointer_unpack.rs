use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    type Invalid = extern "C" fn(#[unpack] &mut [u8]);
}

fn main() {}
