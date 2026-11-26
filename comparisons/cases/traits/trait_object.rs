include!("../support/mod.rs");

#[cfg(any(
    feature = "uniffi",
    feature = "boltffi",
    feature = "safer_ffi",
    feature = "co3"
))]
#[cfg_attr(feature = "uniffi", uniffi::export(callback_interface))]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::derive_ReprC(dyn))]
pub trait Source: Send + Sync {
    fn value(&self) -> u32;
}

#[cfg(any(feature = "uniffi", feature = "boltffi"))]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
pub fn read_source(input: Box<dyn Source>) -> u32 {
    input.value()
}

#[cfg(feature = "interoptopus")]
use interoptopus::callback;
#[cfg(feature = "interoptopus")]
callback!(SourceValue() -> u32);

#[cfg(feature = "interoptopus")]
#[interoptopus::ffi]
pub fn read_source(input: SourceValue) -> u32 {
    input.call()
}

#[cfg(feature = "safer_ffi")]
#[safer_ffi::ffi_export]
pub fn read_source(input: safer_ffi::prelude::VirtualPtr<dyn '_ + Source>) -> u32 {
    input.value()
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub trait Source {
        fn value(&self) -> u32;
    }

    pub fn read_source(input: impl Source) -> u32 {
        input.value()
    }
}

#[cfg(feature = "co3")]
pub fn read_source(input: &dyn Source) -> u32 {
    input.value()
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn read_source(input: &dyn Source) -> u32;
}

#[cfg(feature = "diplomat")]
use diplomat_case::{read_source, Source};

struct Number(u32);

#[cfg(not(feature = "interoptopus"))]
impl Source for Number {
    fn value(&self) -> u32 {
        self.0
    }
}

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "trait_object__read_source"]
        fn imported_read_source(input: &dyn Source) -> u32;
    }

    validate(|number| imported_read_source(&number));
}

#[cfg(any(feature = "uniffi", feature = "boltffi"))]
fn main() {
    validate(|number| {
        let actual = read_source(Box::new(number));

        actual
    });
}

#[cfg(feature = "diplomat")]
fn main() {
    validate(|number| {
        let actual = read_source(number);

        actual
    });
}

#[cfg(feature = "interoptopus")]
fn main() {
    validate(|number| {
        let actual = read_source(SourceValue::from_fn(move || number.0));

        actual
    });
}

#[cfg(feature = "safer_ffi")]
fn main() {
    validate(|number| {
        let actual = read_source((&number).into());

        actual
    });
}

fn validate(mut call: impl FnMut(Number) -> u32) {
    for expected in [7, 42] {
        assert_eq!(call(Number(expected)), expected);
    }
}
