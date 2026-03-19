use co3::{ReprC, export_C, extern_C};

#[derive(Clone, Debug, PartialEq, Eq, ReprC)]
#[repr(C)]
struct Value(Box<u32>);

mod provider {
    use super::*;

    impl Value {
        fn transform(self, other: &Value) -> Value {
            Self(Box::new(*self.0 + *other.0))
        }
    }

    fn combine(input: &Value, rhs: Value) -> Value {
        Value(Box::new(*input.0 + *rhs.0))
    }

    export_C! {
        impl Value {
            #[unsafe(export_name = "transform")]
            fn transform(move self, move other: &Value) -> Value;
        }

        #[unsafe(export_name = "combine")]
        fn combine(input: &Value, move rhs: Value) -> Value;
    }
}

extern_C! {
    #![expect(unused_doc_comments)]
    //! Documentation

    impl Value {
        /// Documentation
        #[link_name = "transform"]
        fn transform2(move self: Self, other: &Value) -> Value;
    }

    /// Documentation
    #[link_name = "combine"]
    fn combine(move input: &Value, move rhs: Value) -> Value;
}

fn main() {
    let lhs = Value(2.into());
    let rhs = Value(3.into());

    assert_eq!(lhs.clone().transform2(&rhs), Value(5.into()));
    assert_eq!(combine(&lhs, rhs), Value(5.into()));
}
