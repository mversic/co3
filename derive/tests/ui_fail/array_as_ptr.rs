#[co3::carbonate]
pub fn array_arg(_arr: [u32; 2]) {}

fn main() {
    __array_arg([12_u32, 42_u32]);
}
