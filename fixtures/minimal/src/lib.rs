#![no_std]

pub fn round_trip(value: u32) -> Option<u32> {
    unsafe { co3::decode(co3::encode(value)) }
}

pub fn round_trip_nullable(value: Option<&u32>) -> Option<Option<&u32>> {
    unsafe { co3::decode(co3::encode(value)) }
}
