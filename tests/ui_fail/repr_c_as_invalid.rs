use co3::ReprC;

#[derive(ReprC)]
#[repr_c(as(Self))]
struct UseIdentity(u32);

#[derive(ReprC)]
#[repr_c(u32)]
struct Positional(u32);

#[derive(ReprC)]
#[repr_c(into = u32)]
struct OldInto(u32);

#[derive(ReprC)]
#[repr_c(as = u32)]
struct OldKeyValue(u32);

#[derive(ReprC)]
#[repr_c(as(u32), identity)]
struct MixedModes(u32);

#[derive(ReprC)]
#[repr_c(as(MissingIntermediate))]
struct MissingCodec(u32);

struct MissingIntermediate(u32);

impl ReprC for MissingIntermediate {
    type CType = u32;
}

impl From<MissingCodec> for MissingIntermediate {
    fn from(value: MissingCodec) -> Self {
        Self(value.0)
    }
}

impl From<MissingIntermediate> for MissingCodec {
    fn from(value: MissingIntermediate) -> Self {
        Self(value.0)
    }
}

fn main() {}
