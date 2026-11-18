use co3::{ReprC, ffi, rust_spec::RustSpec, tuple::Tuple1};

#[derive(Clone, Debug, PartialEq, Eq, RustSpec, ReprC)]
#[repr(C)]
struct Value(Box<u32>);

mod provider {
    use super::*;

    impl Value {
        fn transform(self, other: &Self) -> Self {
            Self(Box::new(*self.0 + *other.0))
        }
    }

    fn combine(input: &Value, rhs: Value) -> Value {
        Value(Box::new(*input.0 + *rhs.0))
    }

    fn sum_soft(values: Vec<&Tuple1<u8>>) -> u8 {
        values.into_iter().map(|value| value.0).sum()
    }

    ffi! {
        #![unsafe(export("C"))]

        impl Value {
            #[symbol_name = "transform"]
            fn transform(#[soft] self: move Self, other: move &Self) -> move Self;
        }

        #[symbol_name = "combine"]
        fn combine(input: &Value, rhs: move Value) -> move Value;

        #[symbol_name = "sum_soft"]
        fn sum_soft(#[soft] values: move Vec<&Tuple1<u8>>) -> u8;
    }
}

ffi! {
    //! Documentation
    #![unsafe(extern("C"))]

    impl Value {
        /// Documentation
        #[symbol_name = "transform"]
        fn transform2(#[soft] self: move Self, other: &Self) -> move Self;
    }

    /// Documentation
    #[symbol_name = "combine"]
    fn combine(input: &Value, rhs: move Value) -> move Value;

    #[symbol_name = "sum_soft"]
    fn sum_soft(#[soft] values: move Vec<&Tuple1<u8>>) -> u8;
}

fn main() {
    let lhs = Value(2.into());
    let rhs = Value(3.into());

    assert_eq!(lhs.clone().transform2(&rhs), Value(5.into()));
    assert_eq!(combine(&lhs, rhs), Value(5.into()));

    let first = Tuple1(2);
    let second = Tuple1(3);
    assert_eq!(sum_soft(vec![&first, &second]), 5);
}
