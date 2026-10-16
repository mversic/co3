use co3::ffi;

trait Projected {
    type Output;

    fn projected(&self) -> Vec<Self::Output>;
}

ffi! {
    #![unsafe(extern("C"))]

    #[tag(u8, unsafe(1))]
    type First;

    #[tag(u8, unsafe(2))]
    type Second;

    impl ToOwned for First {
        type Owned = OwnedFirst;

        fn to_owned(&self) -> move <Self as ToOwned>::Owned;
    }

    impl ToOwned for Second {
        type Owned = OwnedSecond;

        fn to_owned(&self) -> move <Self as ToOwned>::Owned;
    }

    impl<dyn(u8) T: ToOwned> Projected for T
    where
        use<T> @ (<First> | <Second>),
    {
        type Output = <T as ToOwned>::Owned;

        fn projected(&self) -> move Vec<<Self as Projected>::Output>;
    }
}

fn main() {}
