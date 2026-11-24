include!("../support/mod.rs");

#[cfg(any(
    feature = "uniffi",
    feature = "boltffi",
    feature = "safer_ffi",
    feature = "co3",
    feature = "abi_stable"
))]
#[cfg_attr(feature = "abi_stable", abi_stable::sabi_trait)]
#[cfg_attr(feature = "uniffi", uniffi::export(callback_interface))]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::derive_ReprC(dyn))]
pub trait Source: Send + Sync {
    fn value(&self) -> u32;
}

#[cfg(feature = "stabby")]
#[stabby::stabby(checked)]
pub trait Source {
    extern "C" fn value(&self) -> u32;
}

#[cfg(feature = "abi_stable")]
type SourceRef<'a> = Source_TO<'a, abi_stable::sabi_types::RRef<'a, ()>>;
#[cfg(feature = "stabby")]
type SourceRef<'a> = stabby::DynRef<'a, stabby::vtable!(Source + Send + Sync)>;

#[cfg(any(feature = "abi_stable", feature = "stabby"))]
#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle, stabby::stabby)]
pub fn read_source(input: SourceRef<'_>) -> u32 {
    input.value()
}

#[cfg(feature = "abi_stable")]
abi_stable_signature!(read_source: (SourceRef<'_>) -> u32);

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

#[cfg(not(any(feature = "interoptopus", feature = "stabby")))]
impl Source for Number {
    fn value(&self) -> u32 {
        self.0
    }
}

#[cfg(feature = "stabby")]
impl Source for Number {
    extern "C" fn value(&self) -> u32 {
        self.0
    }
}

fn main() {
    for expected in [7, 42] {
        let number = Number(expected);
        #[cfg(any(feature = "uniffi", feature = "boltffi"))]
        let actual = read_source(Box::new(number));
        #[cfg(feature = "interoptopus")]
        let actual = read_source(SourceValue::from_fn(move || number.0));
        #[cfg(feature = "safer_ffi")]
        let actual = read_source((&number).into());
        #[cfg(feature = "diplomat")]
        let actual = read_source(number);
        #[cfg(feature = "co3")]
        let actual = read_source(&number);
        #[cfg(feature = "abi_stable")]
        let actual = read_source(Source_TO::from_const(
            &number,
            abi_stable::sabi_trait::TD_Opaque,
        ));
        #[cfg(feature = "stabby")]
        let actual = read_source((&number).into());

        assert_eq!(actual, expected);
    }
}
