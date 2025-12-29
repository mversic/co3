use co3::ExternC;

#[derive(Debug, Clone, Copy, ExternC)]
#[repr(transparent)]
pub enum FieldlessTransparentEnum {
    A,
}

#[derive(Debug, Clone, Copy, ExternC)]
#[repr(transparent)]
pub struct UnitTransparentStruct;

fn main() {}
