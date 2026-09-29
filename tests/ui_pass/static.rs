use co3::ffi;

#[unsafe(export_name = "static__VERSION")]
pub static VERSION: StaticAbi = 1;
#[unsafe(export_name = "static_test_flags")]
pub static mut FLAGS: StaticAbi = 1;

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "static"]

    type StaticAbi = core::num::NonZeroU8;

    pub static VERSION: core::num::NonZeroU8;
    #[symbol_name = "static_test_flags"]
    pub static mut FLAGS: core::num::NonZeroU8;
}

mod imported {
    use co3::ffi;

    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "static"]

        pub static VERSION: core::num::NonZeroU8;
        #[symbol_name = "static_test_flags"]
        pub static mut FLAGS: core::num::NonZeroU8;
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
    unsafe { imported::FLAGS.set(core::num::NonZeroU8::new(1).unwrap()) };
}
