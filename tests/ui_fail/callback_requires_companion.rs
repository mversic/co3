use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
struct NoRepr(u8);

type BadArgument = extern "C" fn(());
type BadReturn = extern "C" fn() -> [u32; 2];
type NonStable = extern "C" fn(NoRepr);

ffi! {
    #![unsafe(extern("C"))]

    fn callback_with_bad_argument(callback: BadArgument);
    fn callback_with_bad_return(callback: BadReturn);
    fn non_stable_callback(callback: NonStable);
}

fn main() {}
