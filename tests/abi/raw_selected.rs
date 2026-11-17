use co3::{ReprC, Tag, ffi, ops::CFn1, rust_spec::RustSpec};

struct Host<T>(core::marker::PhantomData<T>);

struct DynHost<T>(core::marker::PhantomData<T>);

#[derive(Clone, Copy, ReprC, RustSpec, Tag)]
#[tag(u8, unsafe(1))]
#[repr_c(identity)]
#[repr(transparent)]
struct First(u8);

#[derive(Clone, Copy, ReprC, RustSpec, Tag)]
#[tag(u8, unsafe(2))]
#[repr_c(identity)]
#[repr(transparent)]
struct Second(u8);

trait Selected<T> {
    unsafe extern "C" fn selected(value: T) -> T;
}

trait RegularSelected<T> {
    fn regular(value: T) -> T;
}

trait RuntimeSelected: Sized + ReprC<CType: Sized> {
    unsafe extern "C" fn runtime(value: <Self as co3::ReprC>::CType)
    -> <Self as co3::ReprC>::CType;
    unsafe extern "C" fn runtime_second(
        value: <Self as co3::ReprC>::CType,
    ) -> <Self as co3::ReprC>::CType;
}

#[unsafe(export_name = "raw_selected_inherent_u8")]
extern "C" fn inherent_u8(value: u8) -> u8 {
    value + 1
}

#[unsafe(export_name = "raw_selected_inherent_u16")]
extern "C" fn inherent_u16(value: u16) -> u16 {
    value + 2
}

#[unsafe(export_name = "raw_selected_trait_u8")]
extern "C" fn trait_u8(value: u8) -> u8 {
    value + 3
}

#[unsafe(export_name = "raw_selected_trait_u16")]
extern "C" fn trait_u16(value: u16) -> u16 {
    value + 4
}

#[unsafe(export_name = "regular_selected_trait_u8")]
extern "C" fn regular_trait_u8(value: u8) -> u8 {
    value + 5
}

#[unsafe(export_name = "regular_selected_trait_u16")]
extern "C" fn regular_trait_u16(value: u16) -> u16 {
    value + 6
}

#[unsafe(export_name = "raw_selected_dynamic")]
extern "C" fn dynamic(tag: u8, value: u8) -> u8 {
    value + tag
}

#[unsafe(export_name = "raw_selected_dynamic_second")]
extern "C" fn dynamic_second(tag: u8, value: u8) -> u8 {
    value + tag + 10
}

ffi! {
    #![unsafe(extern("C"))]
    impl<T> Host<T>
    where
        use<T> @ (<u8> | <u16>),
    {
        #[symbol_name = "raw_selected_inherent_{T}"]
        raw fn selected(value: move T) -> move T;
    }

    impl<T> Selected<T> for Host<T>
    where
        use<T> @ (<u8> | <u16>),
    {
        #[symbol_name = "raw_selected_trait_{T}"]
        raw fn selected(value: move T) -> move T;
    }

    impl<T> RegularSelected<T> for Host<T>
    where
        use<T> @ (<u8> | <u16>),
    {
        #[symbol_name = "regular_selected_trait_{T}"]
        fn regular(value: move T) -> move T;
    }

    impl<dyn(u8) T = u8> DynHost<T>
    where
        use<T> @ (<First> | <Second>),
    {
        #[symbol_name = "raw_selected_dynamic"]
        raw fn dispatch(value: move T) -> move T;

        #[symbol_name = "raw_selected_dynamic_second"]
        raw fn dispatch_second(value: move T) -> move T;
    }

    impl<dyn(u8) T = u8> RuntimeSelected for T
    where
        use<T> @ (<First> | <Second>),
    {
        #[symbol_name = "raw_selected_dynamic"]
        raw fn runtime(value: move Self) -> move Self;

        #[symbol_name = "raw_selected_dynamic_second"]
        raw fn runtime_second(value: move Self) -> move Self;
    }
}

#[test]
fn selected_raw_methods_call_the_matching_foreign_symbol() {
    let first: unsafe extern "C" fn(_) -> _ = Host::<u8>::selected;
    let second: unsafe extern "C" fn(_) -> _ = Host::<u16>::selected;
    let first_trait: unsafe extern "C" fn(_) -> _ = <Host<u8> as Selected<u8>>::selected;
    let second_trait: unsafe extern "C" fn(_) -> _ = <Host<u16> as Selected<u16>>::selected;
    assert_eq!(unsafe { first.call(10u8) }, Some(11u8));
    assert_eq!(unsafe { second.call(10u16) }, Some(12u16));
    assert_eq!(unsafe { first_trait.call(10u8) }, Some(13u8));
    assert_eq!(unsafe { second_trait.call(10u16) }, Some(14u16));
    assert_eq!(<Host<u8> as RegularSelected<u8>>::regular(10), 15);
    assert_eq!(<Host<u16> as RegularSelected<u16>>::regular(10), 16);
    let callback: unsafe extern "C" fn(_) -> _ = DynHost::<First>::dispatch;
    let result: First = unsafe { callback.call(First(10)) }.unwrap();
    assert_eq!(result.0, 11);
    let callback: unsafe extern "C" fn(_) -> _ = DynHost::<Second>::dispatch;
    let result: Second = unsafe { callback.call(Second(10)) }.unwrap();
    assert_eq!(result.0, 12);
    let callback: unsafe extern "C" fn(_) -> _ = DynHost::<First>::dispatch_second;
    let result: First = unsafe { callback.call(First(10)) }.unwrap();
    assert_eq!(result.0, 21);
    let callback: unsafe extern "C" fn(_) -> _ = DynHost::<Second>::dispatch_second;
    let result: Second = unsafe { callback.call(Second(10)) }.unwrap();
    assert_eq!(result.0, 22);
    let callback: unsafe extern "C" fn(_) -> _ = First::runtime;
    let result: First = unsafe { callback.call(First(10)) }.unwrap();
    assert_eq!(result.0, 11);
    let callback: unsafe extern "C" fn(_) -> _ = Second::runtime;
    let result: Second = unsafe { callback.call(Second(10)) }.unwrap();
    assert_eq!(result.0, 12);
    let callback: unsafe extern "C" fn(_) -> _ = First::runtime_second;
    let result: First = unsafe { callback.call(First(10)) }.unwrap();
    assert_eq!(result.0, 21);
    let callback: unsafe extern "C" fn(_) -> _ = Second::runtime_second;
    let result: Second = unsafe { callback.call(Second(10)) }.unwrap();
    assert_eq!(result.0, 22);
}
