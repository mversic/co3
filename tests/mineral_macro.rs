//use co3::{ir::Ir, mineral, transmute::Transmute, ReprC};
//
//#[repr(transparent)]
//#[derive(Clone, Copy, Debug)]
//struct MultiBound<'a, T: Clone + core::fmt::Debug + 'a>(&'a T);
//
//mineral! {
//    unsafe impl<'a, T> Transparent for MultiBound<'a, T>
//    where
//        T: Clone + core::fmt::Debug + 'a,
//        'a: 'static,
//    {
//        type Target = &'a T;
//    }
//}
//
//#[repr(transparent)]
//#[derive(Clone, Copy)]
//struct FuncHolder<F, const N: usize>([F; N]);
//
//mineral! {
//    unsafe impl<F, const N: usize,> Transparent for FuncHolder<F, N>
//    where
//        F: FnMut(usize) -> usize + Copy,
//    {
//        type Target = [F; N];
//    }
//}
//
//#[repr(C)]
//#[derive(Clone, Copy)]
//struct RefHolder<'a, T: ?Sized + 'a>(&'a T);
//
//// SAFETY: `RefHolder` is `#[repr(C)]` and only contains a reference which is `Copy`
//unsafe impl<'a, T: ?Sized + 'a> ReprC for RefHolder<'a, T> {}
//
//mineral! {
//    impl<'a, T: ?Sized + 'a> Robust for RefHolder<'a, T> {}
//}
//
//#[test]
//#[webassembly_test::webassembly_test]
//fn transparent_allows_multiple_bounds_and_lifetimes() {
//    static VALUE: u32 = 7;
//    let wrapper = MultiBound(&VALUE);
//
//    assert!(<MultiBound<'static, u32> as Transmute>::is_valid(&wrapper.0));
//}
//
//#[test]
//#[webassembly_test::webassembly_test]
//fn transparent_supports_const_generics_and_trailing_comma() {
//    fn double(x: usize) -> usize {
//        x * 2
//    }
//
//    let funcs = [double as fn(usize) -> usize; 1];
//    let holder = FuncHolder(funcs);
//
//    assert!(<FuncHolder<_, 1> as Transmute>::is_valid(&holder.0));
//}
//
//#[test]
//#[webassembly_test::webassembly_test]
//fn robust_allows_qsized_and_lifetime_bounds() {
//    fn assert_robust<T: Ir<Type = co3::ir::Robust>>() {}
//
//    assert_robust::<RefHolder<'static, str>>();
//}
