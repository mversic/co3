// A custom dynamically sized type with a slice tail.

include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "boltffi", boltffi::data)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::derive_ReprC)]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec, co3::ReprC))]
#[repr(C)]
pub struct Packet {
    pub kind: u32,
    pub payload: [u8],
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn kind(packet: &Packet) -> u32 {
    packet.kind
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {

    #[repr(C)]
    pub struct Packet {
        pub kind: u32,
        pub payload: [u8],
    }

    pub fn kind(packet: &Packet) -> u32 {
        packet.kind
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn kind(packet: &Packet) -> u32;
}

fn main() {}
