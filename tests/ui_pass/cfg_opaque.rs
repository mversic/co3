use co3::{ffi, raw};

fn increment(value: u8) -> u8 {
    value + 1
}

ffi! {
    #![cfg_attr(all(), unsafe(export("C")))]
    #![cfg_attr(all(), cfg_attr(all(), symbol_prefix = "opaque_cfg",),)]

    // These outer attributes are forwarded, not interpreted by preprocessing.
    #[cfg(all())]
    #[cfg_attr(all(), doc = "kept", doc = "kept")]
    fn increment(
        #[cfg(all())] value: u8,
        #[cfg(any())] extra: MissingType,
    ) -> u8;
}

mod imported {
    use co3::ffi;
    ffi! {
        #![cfg_attr(all(), unsafe(extern("C")))]
        #![symbol_prefix = "opaque_cfg"]
        pub fn increment(value: u8) -> u8;
    }
}

raw! {
    #![cfg_attr(all(), failure = "panic")]
    fn increment(
        #[cfg(any())] disabled: MissingType,
        value: u8,
    ) -> u8;
}

// Inner cfg can gate the entire block, including its direction and declarations.
ffi! {
    #![cfg(any())]
    #![unsupported]
    fn missing(#[soft] arg: move MissingType);
}

fn main() {
    assert_eq!(imported::increment(41), 42);
    assert_eq!(unsafe { increment_raw(41) }, 42);
}
