use co3::{ReprC, ffi};

ffi! {
    #![unsafe(extern("C"))]

    type BadArg = raw extern "C" fn(move [u8; 2]);
    type BadReturn = raw extern "C" fn() -> move [u8; 2];
}

fn main() {
    static_assertions::assert_type_eq_all!(BadArg, unsafe extern "C" fn([u8; 2]));
    static_assertions::assert_type_eq_all!(BadReturn, unsafe extern "C" fn() -> [u8; 2]);
    static_assertions::assert_not_impl_any!(BadArg: ReprC);
    static_assertions::assert_not_impl_any!(BadReturn: ReprC);
}
