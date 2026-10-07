use co3::ReprC;

#[derive(ReprC)]
#[repr_c(as(u32))]
struct UnmarkedDrop(u32);

impl Drop for UnmarkedDrop {
    fn drop(&mut self) {}
}

impl From<UnmarkedDrop> for u32 {
    fn from(value: UnmarkedDrop) -> Self {
        value.0
    }
}

impl TryFrom<u32> for UnmarkedDrop {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Ok(Self(value))
    }
}

fn main() {}
