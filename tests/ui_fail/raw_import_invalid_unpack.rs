use co3::ffi;

struct Host;

ffi! {
    #![unsafe(extern("C"))]

    raw fn missing_parts(#[unpack] value: (u8, u8));

    impl Host {
        raw fn missing_method_parts(#[unpack] value: (u8, u8));
    }
}

fn main() {}
