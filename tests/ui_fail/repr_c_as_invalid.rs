use co3::ReprC;

#[derive(ReprC)]
#[repr_c(as(u16))]
struct Unsupported(u8);

fn main() {}
