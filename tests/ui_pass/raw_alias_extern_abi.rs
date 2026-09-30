use co3::{ffi, ReprC};

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

ffi! {
    #![unsafe(extern("C"))]

    type BadArg = raw extern "C" fn(move [u8; 2]);
    type BadReturn = raw extern "C" fn() -> move [u8; 2];
}

ffi! {
    #![unsafe(export("C"))]

    type Moved<T> = raw extern "C" fn(move T) -> move T;
    type Borrowed<T> = raw extern "C" fn(T) -> T;
    type Array<const N: usize> = raw extern "C" fn(move [u8; N]) -> move [u8; N];
    type Combined<T, const N: usize> = raw extern "C" fn(move [T; N]);
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
    static_assertions::assert_type_eq_all!(
        Nested,
        unsafe extern "C" fn(Option<unsafe extern "system" fn(u8)>)
    );
    static_assertions::assert_type_eq_all!(
        Returned,
        unsafe extern "C" fn() -> Option<unsafe extern "C" fn(u8)>
    );
    let _: Option<RawNested> = None;

    static_assertions::assert_type_eq_all!(BadArg, unsafe extern "C" fn([u8; 2]));
    static_assertions::assert_type_eq_all!(BadReturn, unsafe extern "C" fn() -> [u8; 2]);
    static_assertions::assert_not_impl_any!(BadArg: ReprC);
    static_assertions::assert_not_impl_any!(BadReturn: ReprC);

    type StringC = <String as ReprC>::CType;
    static_assertions::assert_type_eq_all!(Moved<String>, unsafe extern "C" fn(StringC) -> StringC);
    static_assertions::assert_type_eq_all!(Borrowed<u8>, unsafe extern "C" fn(u8) -> u8);
    static_assertions::assert_type_eq_all!(Array<2>, unsafe extern "C" fn([u8; 2]) -> [u8; 2]);
    static_assertions::assert_type_eq_all!(Combined<u8, 2>, unsafe extern "C" fn([u8; 2]));
}
