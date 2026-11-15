use co3::{
    ReprC,
    option::OptionSome,
    result::{ResultErr, ResultOk},
    rust_spec::{
        RustSpec, Stable,
        layout::{NonRobust, Robust},
    },
};
use core::num::NonZeroU8;
use static_assertions::{assert_impl_all, assert_type_eq_all, assert_type_ne_all};

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
#[rust_spec(custom_niche)]
#[repr_c(NICHE = CCustomNiche(0))]
struct CustomNiche(u8);

assert_impl_all!(OptionSome<u8>: RustSpec<Layout = Stable, Trap = Robust>);
assert_impl_all!(ResultOk<u8>: RustSpec<Layout = Stable, Trap = Robust>);
assert_impl_all!(ResultErr<u8>: RustSpec<Layout = Stable, Trap = Robust>);
assert_impl_all!(OptionSome<CustomNiche>: RustSpec<Layout = Stable, Trap = NonRobust>);
assert_impl_all!(ResultOk<CustomNiche>: RustSpec<Layout = Stable, Trap = NonRobust>);
assert_impl_all!(ResultErr<CustomNiche>: RustSpec<Layout = Stable, Trap = NonRobust>);
assert_type_eq_all!(
    <OptionSome<CustomNiche> as ReprC>::CType,
    OptionSome<<CustomNiche as ReprC>::CType>
);
assert_type_eq_all!(
    <ResultOk<CustomNiche> as ReprC>::CType,
    ResultOk<<CustomNiche as ReprC>::CType>
);
assert_type_eq_all!(
    <ResultErr<CustomNiche> as ReprC>::CType,
    ResultErr<<CustomNiche as ReprC>::CType>
);
assert_type_ne_all!(
    co3::option::COptionSome<CustomNiche>,
    OptionSome<<CustomNiche as ReprC>::CType>
);

fn option_payload(value: OptionSome<CustomNiche>) -> OptionSome<CustomNiche> {
    value
}
fn ok_payload(value: ResultOk<CustomNiche>) -> ResultOk<CustomNiche> {
    value
}
fn err_payload(value: ResultErr<CustomNiche>) -> ResultErr<CustomNiche> {
    value
}

co3::raw! {
    fn option_payload(value: move OptionSome<CustomNiche>) -> move OptionSome<CustomNiche>;
    fn ok_payload(value: move ResultOk<CustomNiche>) -> move ResultOk<CustomNiche>;
    fn err_payload(value: move ResultErr<CustomNiche>) -> move ResultErr<CustomNiche>;
}

#[test]
fn public_payload_wrappers_preserve_custom_niches_without_a_tag() {
    for value in [7, 255] {
        let input = OptionSome(CustomNiche(value));
        let output = unsafe { option_payload_raw(co3::encode(input)) };
        assert_eq!(
            unsafe { co3::decode::<OptionSome<CustomNiche>>(output) },
            Some(input)
        );
        let input = ResultOk(CustomNiche(value));
        let output = unsafe { ok_payload_raw(co3::encode(input)) };
        assert_eq!(
            unsafe { co3::decode::<ResultOk<CustomNiche>>(output) },
            Some(input)
        );
        let input = ResultErr(CustomNiche(value));
        let output = unsafe { err_payload_raw(co3::encode(input)) };
        assert_eq!(
            unsafe { co3::decode::<ResultErr<CustomNiche>>(output) },
            Some(input)
        );
    }
    assert_eq!(core::mem::size_of::<OptionSome<CustomNiche>>(), 1);
    assert_eq!(core::mem::size_of::<ResultOk<CustomNiche>>(), 1);
    assert_eq!(core::mem::size_of::<ResultErr<CustomNiche>>(), 1);
    assert!(core::mem::size_of::<Option<CustomNiche>>() > 1);
}

#[test]
fn public_payload_wrappers_preserve_nonzero_validity() {
    use co3::num::NonZero;
    assert_eq!(
        unsafe { co3::decode::<OptionSome<NonZeroU8>>(OptionSome(NonZero(0))) },
        None
    );
    assert_eq!(
        unsafe { co3::decode::<ResultOk<NonZeroU8>>(ResultOk(NonZero(0))) },
        None
    );
    assert_eq!(
        unsafe { co3::decode::<ResultErr<NonZeroU8>>(ResultErr(NonZero(0))) },
        None
    );
    let value = NonZeroU8::new(7).unwrap();
    assert_eq!(
        unsafe { co3::decode::<OptionSome<NonZeroU8>>(co3::encode(OptionSome(value))) },
        Some(OptionSome(value))
    );
    assert_eq!(
        unsafe { co3::decode::<ResultOk<NonZeroU8>>(co3::encode(ResultOk(value))) },
        Some(ResultOk(value))
    );
    assert_eq!(
        unsafe { co3::decode::<ResultErr<NonZeroU8>>(co3::encode(ResultErr(value))) },
        Some(ResultErr(value))
    );
}
