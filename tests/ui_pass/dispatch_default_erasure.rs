use co3::{Tag, ReprC, ffi, rust_spec::RustSpec, slice::Unpack2, tuple::ReprCTuple2};

#[derive(RustSpec, ReprC, Tag)]
#[tag(u8, unsafe(1))]
#[repr(transparent)]
struct Value(u16);

#[derive(RustSpec, ReprC, Tag)]
#[tag(u8, unsafe(2))]
#[repr(transparent)]
struct Pair(ReprCTuple2<u8, u8>);

impl Unpack2<u16, u16> for Pair {
    type Error = core::convert::Infallible;
    fn unpack(value: Self::CType) -> Result<(u16, u16), Self::Error> {
        Ok((value.0.0.into(), value.0.1.into()))
    }
}

mod symbols {
    #[unsafe(no_mangle)]
    extern "C" fn erased(_: u8, _: u16) {}

    #[unsafe(no_mangle)]
    extern "C" fn unpack_wins(_: u8, _: u16, _: u16) {}
}

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "erased"]
    fn erased<dyn(u8) T = u16>(value: move T)
    where
        use<T> @ <Value>;

    #[symbol_name = "unpack_wins"]
    fn unpack_wins<dyn(u8) T = (u16, u16)>(#[unpack(u16, u16)] value: move T)
    where
        use<T> @ <Pair>;
}

fn main() {
    erased(Value(2));
    unpack_wins(Pair(ReprCTuple2(1, 2)));
}
