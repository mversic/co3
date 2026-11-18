use co3::{ReprC, encode, ffi, pack::Unpack2, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Logical(u16);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Value(u16);

impl Unpack2<u8, <Logical as ReprC>::CType> for Value {
    type Error = core::convert::Infallible;
    fn unpack(value: Self::CType) -> Result<(u8, <Logical as ReprC>::CType), Self::Error> {
        Ok((0, encode(Logical(value.0))))
    }
}

ffi! {
    #![unsafe(extern("C"))]

    fn size_mismatch(
        #[unpack(u8, Logical => u32)]
        value: move Value,
    );
}

fn main() {
    size_mismatch(Value(1));
}
