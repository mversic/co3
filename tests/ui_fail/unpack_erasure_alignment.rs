use co3::{ReprC, ffi, rust_spec::RustSpec, slice::Unpack2};

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Abi(u32, u32);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Value(u64);

impl Unpack2<u8, u64> for Value {
    type Error = core::convert::Infallible;
    fn unpack(value: Self::CType) -> Result<(u8, u64), Self::Error> {
        Ok((0, value.0))
    }
}

ffi! {
    #![unsafe(extern("C"))]

    fn alignment_mismatch(
        #[unpack(u8, u64 => Abi)]
        value: move Value,
    );
}

fn main() {
    alignment_mismatch(Value(1));
}
