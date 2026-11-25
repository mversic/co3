#![no_std]

use co3::lang;

pub fn round_trip(value: u32) -> Option<u32> {
    unsafe { co3::decode::<_, lang::C>(co3::encode::<_, lang::C>(value)) }
}

pub fn round_trip_nullable(value: Option<&u32>) -> Option<Option<&u32>> {
    unsafe { co3::decode::<_, lang::C>(co3::encode::<_, lang::C>(value)) }
}
