use co3::{ExternC, ReprC, encode, ffi, rust_spec::RustSpec, slice::Unpack2};

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Logical(u16);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Value(u16);

impl Unpack2<u8, <Logical as ExternC>::CType> for Value {
    type Error = core::convert::Infallible;
    fn unpack(value: Self::CType) -> Result<(u8, <Logical as ExternC>::CType), Self::Error> {
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
