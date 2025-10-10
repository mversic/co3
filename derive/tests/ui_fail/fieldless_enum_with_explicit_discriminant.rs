use co3::ExternC;

#[derive(ExternC)]
#[repr(u8)]
pub enum EnumWithExplicitDiscriminant {
    A = 1,
    B,
    C,
    D,
}

fn main() {}
