use co3::ffi;

fn select(value: u8) -> u8 {
    value + 1
}

fn select_second(value: u8) -> u8 {
    value + 2
}

fn optional_disabled(value: u8) -> u8 {
    value + 1
}

fn optional_enabled(value: u8, extra: u8) -> u8 {
    value + extra
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "abi_cfg_args"]

    fn select(
        #[cfg(all())] value: u8,
        #[cfg(any())] value: MissingType,
    ) -> u8;

    fn select_second(
        #[cfg(any())] value: MissingType,
        #[cfg(all())] value: u8,
    ) -> u8;

    fn optional_disabled(value: u8, #[cfg(any())] extra: MissingType) -> u8;
    fn optional_enabled(value: u8, #[cfg(all())] extra: u8) -> u8;
}

mod imported {
    use co3::ffi;

    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "abi_cfg_args"]

        pub fn select(
            #[cfg(all())] value: u8,
            #[cfg(any())] value: MissingType,
        ) -> u8;
        pub fn select_second(value: u8) -> u8;
        pub fn optional_disabled(value: u8, #[cfg(any())] extra: MissingType) -> u8;
        pub fn optional_enabled(value: u8, #[cfg(all())] extra: u8) -> u8;
    }
}

#[test]
fn conditional_parameter_keeps_exported_function() {
    assert_eq!(imported::select(41), 42);
    assert_eq!(imported::select_second(40), 42);
    assert_eq!(imported::optional_disabled(41), 42);
    assert_eq!(imported::optional_enabled(40, 2), 42);
}
