use co3::ffi;

ffi! {
    #![unsafe(export("system"))]

    fn callback_with_c_abi(callback: extern "C" fn(u8) -> u8);
}

ffi! {
    #![unsafe(export("system"))]

    fn return_c_abi() -> extern "C" fn(u8) -> u8;
}

ffi! {
    #![unsafe(extern("system"))]

    fn callback_with_c_abi(callback: extern "C" fn(u8) -> u8);
}

ffi! {
    #![unsafe(extern("system"))]

    fn return_c_abi() -> extern "C" fn(u8) -> u8;
}

fn main() {}
