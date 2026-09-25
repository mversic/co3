#[unsafe(export_name = "static__VERSION")]
pub static VERSION: <u32 as co3::ReprC>::CType = 1;
#[unsafe(export_name = "static_test_flags")]
pub static mut FLAGS: <u32 as co3::ReprC>::CType = 0;

mod imported {
    use co3::ffi;

    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "static"]

        pub static VERSION: u32;
        #[symbol_name = "static_test_flags"]
        pub static mut FLAGS: u32;
    }
}

fn main() {
    let _ = VERSION;
    let _ = imported::VERSION.get();
    let _ = imported::VERSION.read();
    let _ = unsafe { imported::VERSION.get_unchecked() };
    let _ = unsafe { FLAGS };
    unsafe { FLAGS = 1 };
    let _ = unsafe { imported::FLAGS.read() };
    unsafe { imported::FLAGS.set(1) };
    let _ = unsafe { imported::FLAGS.take() };
}
