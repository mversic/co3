use co3::ReprC;

#[derive(ReprC)]
#[repr(C, packed)]
struct Packed(u32);

#[derive(ReprC)]
#[repr(C, packed(2))]
struct PackedTwo(u32);

fn main() {}
