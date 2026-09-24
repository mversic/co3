use co3::ops::CFn1;

extern "C" fn identity(value: u8) -> u8 {
    value
}

fn main() {
    let callback: extern "C" fn(u8) -> u8 = identity;
    let _: Option<u8> = unsafe { callback.call(1_u8) };
}
