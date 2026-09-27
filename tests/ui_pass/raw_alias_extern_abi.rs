use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    type ExplicitAbi = raw extern "system" fn(u8);
}

fn main() {
    let _: Option<ExplicitAbi> = None;
    static_assertions::assert_type_eq_all!(ExplicitAbi, unsafe extern "system" fn(u8));
}
