use co3::{ReprC, ffi};

ffi! {
    #![unsafe(extern("C"))]

    type ExplicitAbi = raw extern "system" fn(u8);
    type InheritedAbi = raw fn(u8);
    type Ordinary = core::num::NonZeroU8;
    type Regular = extern "C" fn(u8);
    type RegularUnpacked = extern "C" fn(#[unpack(_, _)] &mut [u8]) -> usize;
    type Composite = (String, raw fn(String));
    type Optional = Option<raw fn(u8)>;
    type CompositeReturn = (u8, raw fn() -> u8);
    type Nested = raw extern "C" fn(raw extern "system" fn(u8));
    type Returned = raw fn() -> raw fn(u8);
    type RawNested = raw extern "C" fn(extern "system" fn(u8));
}

fn main() {
    let _: Option<ExplicitAbi> = None;
    static_assertions::assert_type_eq_all!(ExplicitAbi, unsafe extern "system" fn(u8));
    static_assertions::assert_type_eq_all!(InheritedAbi, unsafe extern "C" fn(u8));
    static_assertions::assert_type_eq_all!(Ordinary, u8);
    static_assertions::assert_type_eq_all!(Regular, Option<extern "C" fn(u8)>);
    static_assertions::assert_type_eq_all!(
        RegularUnpacked,
        Option<unsafe extern "C" fn(*mut u8, usize) -> usize>
    );
    static_assertions::assert_type_eq_all!(
        Composite,
        <(String, unsafe extern "C" fn(co3::slice::CSlice<u8>)) as ReprC>::CType
    );
    static_assertions::assert_type_eq_all!(Optional, Option<unsafe extern "C" fn(u8)>);
    static_assertions::assert_type_eq_all!(
        CompositeReturn,
        <(u8, unsafe extern "C" fn() -> u8) as ReprC>::CType
    );
    static_assertions::assert_type_eq_all!(Nested, unsafe extern "C" fn(Option<unsafe extern "system" fn(u8)>));
    static_assertions::assert_type_eq_all!(Returned, unsafe extern "C" fn() -> Option<unsafe extern "C" fn(u8)>);
    let _: Option<RawNested> = None;
}
