use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    type ExplicitAbi = raw extern "system" fn(u8);
    type Regular = extern "C" fn(u8);
    type Nested = Option<extern "system" fn(u8)>;
    type RawNested = raw extern "C" fn(extern "system" fn(u8));
}

fn main() {
    let _: Option<ExplicitAbi> = None;
    static_assertions::assert_type_eq_all!(ExplicitAbi, unsafe extern "system" fn(u8));
    static_assertions::assert_type_eq_all!(Regular, extern "C" fn(u8));
    let _: Nested = None;
    let _: Option<RawNested> = None;
}
