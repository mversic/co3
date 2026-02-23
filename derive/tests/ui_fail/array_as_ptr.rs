use co3::carbonate;

#[carbonate(extern "C")]
#[unsafe(no_mangle)]
pub extern "C" fn array_arg(arr: [u32; 2]) -> [u32; 2] {
    arr
}

fn main() {
    unsafe extern "C" {
        fn array_arg(arr: [u32; 2]) -> [u32; 2];
    }
}
