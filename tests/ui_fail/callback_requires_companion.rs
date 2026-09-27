use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct CallbackValue(u8);

type BadArgument = extern "C" fn(CallbackValue);
type BadReturn = extern "C" fn() -> CallbackValue;

ffi! {
    #![unsafe(extern("C"))]

    fn callback_with_bad_argument(callback: BadArgument);
    fn callback_with_bad_return(callback: BadReturn);
}

fn main() {}
