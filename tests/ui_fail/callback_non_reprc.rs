use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct NotReprC(u8);

type BadArgument = extern "C" fn(NotReprC);
type BadReturn = extern "C" fn() -> NotReprC;

ffi! {
    #![unsafe(extern("C"))]

    fn callback_with_bad_argument(callback: BadArgument);
    fn callback_with_bad_return(callback: BadReturn);
}

fn main() {}
