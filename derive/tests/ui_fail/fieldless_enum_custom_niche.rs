use co3::ReprC;

#[derive(ReprC)]
#[reprC(
    NICHE_VALUE = 42,
    unsafe(is_valid = |target: &Self::Target| *target < 2)
)]
#[repr(u8)]
pub enum PrimitiveFieldless {
    A,
    B,
}

fn main() {}
