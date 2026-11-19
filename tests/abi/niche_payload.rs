use core::num::NonZeroU8;

use co3::{
    ReprC,
    ops::CFn1,
    option::{self, OptionSome},
    result::{self, ResultErr, ResultOk},
    rust_spec::{
        RustSpec, Stable,
        layout::{NonRobust, Robust},
    },
};
use static_assertions::{assert_impl_all, assert_type_eq_all, assert_type_ne_all};

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
#[rust_spec(custom_niche)]
#[repr_c(NICHE = CustomNicheCType(0))]
struct CustomNiche(u8);

assert_impl_all!(OptionSome<u8>: RustSpec<Layout = Stable, Trap = Robust>);
assert_impl_all!(ResultOk<u8>: RustSpec<Layout = Stable, Trap = Robust>);
assert_impl_all!(ResultErr<u8>: RustSpec<Layout = Stable, Trap = Robust>);
assert_impl_all!(OptionSome<CustomNiche>: RustSpec<Layout = Stable, Trap = NonRobust>);
assert_impl_all!(ResultOk<CustomNiche>: RustSpec<Layout = Stable, Trap = NonRobust>);
assert_impl_all!(ResultErr<CustomNiche>: RustSpec<Layout = Stable, Trap = NonRobust>);
assert_type_eq_all!(
    <OptionSome<CustomNiche> as ReprC>::CType,
    co3::marker::Stable<option::abi::OptionSome<<CustomNiche as ReprC>::CType>>
);
assert_type_eq_all!(
    <ResultOk<CustomNiche> as ReprC>::CType,
    co3::marker::Stable<result::abi::ResultOk<<CustomNiche as ReprC>::CType, ()>>
);
assert_type_eq_all!(
    <ResultErr<CustomNiche> as ReprC>::CType,
    co3::marker::Stable<result::abi::ResultErr<(), <CustomNiche as ReprC>::CType>>
);
assert_type_ne_all!(
    co3::option::abi::OptionSome<<CustomNiche as co3::ReprC>::CType>,
    co3::marker::Stable<option::abi::OptionSome<<CustomNiche as ReprC>::CType>>
);

assert_type_eq_all!(
    <Option<CustomNiche> as ReprC>::CType,
    option::abi::OptionSome<<CustomNiche as ReprC>::CType>
);
assert_type_eq_all!(
    <Result<CustomNiche, ()> as ReprC>::CType,
    result::abi::ResultOk<<CustomNiche as ReprC>::CType, ()>
);
assert_type_eq_all!(
    <Result<(), CustomNiche> as ReprC>::CType,
    result::abi::ResultErr<(), <CustomNiche as ReprC>::CType>
);
assert_type_ne_all!(
    result::abi::ResultOk<<CustomNiche as ReprC>::CType, ()>,
    co3::marker::Stable<result::abi::ResultOk<<CustomNiche as ReprC>::CType, ()>>
);
assert_type_ne_all!(
    result::abi::ResultErr<(), <CustomNiche as ReprC>::CType>,
    co3::marker::Stable<result::abi::ResultErr<(), <CustomNiche as ReprC>::CType>>
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
        let callback: unsafe extern "C" fn(_) -> _ = option_payload_raw;
        let output: Option<OptionSome<CustomNiche>> = unsafe { callback.call(input) };
        assert_eq!(output, Some(input));
        let input = ResultOk(CustomNiche(value));
        let callback: unsafe extern "C" fn(_) -> _ = ok_payload_raw;
        let output: Option<ResultOk<CustomNiche>> = unsafe { callback.call(input) };
        assert_eq!(output, Some(input));
        let input = ResultErr(CustomNiche(value));
        let callback: unsafe extern "C" fn(_) -> _ = err_payload_raw;
        let output: Option<ResultErr<CustomNiche>> = unsafe { callback.call(input) };
        assert_eq!(output, Some(input));
    }
    assert_eq!(core::mem::size_of::<OptionSome<CustomNiche>>(), 1);
    assert_eq!(core::mem::size_of::<ResultOk<CustomNiche>>(), 1);
    assert_eq!(core::mem::size_of::<ResultErr<CustomNiche>>(), 1);
    assert!(core::mem::size_of::<Option<CustomNiche>>() > 1);
}

#[test]
fn public_payload_wrappers_preserve_nonzero_validity() {
    use co3::marker::NonZero;
    assert_eq!(
        unsafe {
            co3::decode::<OptionSome<NonZeroU8>>(co3::marker::Stable(option::abi::OptionSome::<_>(
                NonZero(0),
            )))
        },
        None
    );
    assert_eq!(
        unsafe {
            co3::decode::<ResultOk<NonZeroU8>>(co3::marker::Stable(result::abi::ResultOk::<_, ()>(
                NonZero(0),
                core::marker::PhantomData,
            )))
        },
        None
    );
    assert_eq!(
        unsafe {
            co3::decode::<ResultErr<NonZeroU8>>(co3::marker::Stable(
                result::abi::ResultErr::<(), _>(NonZero(0), core::marker::PhantomData),
            ))
        },
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
