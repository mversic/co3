use co3::ReprC;

#[derive(ReprC)]
#[repr(u8)]
pub enum EnumWithExplicitDiscriminant {
    A = 1,
    B,
    C,
    D,
}

fn main() {}
