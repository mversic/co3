use co3::ReprC;

#[derive(Debug, Clone, Copy, ReprC)]
#[repr(transparent)]
pub enum FieldlessTransparentEnum {
    A,
}

#[derive(Debug, Clone, Copy, ReprC)]
#[repr(transparent)]
pub struct UnitTransparentStruct;

fn main() {}
