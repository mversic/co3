use co3::{Tag, ffi};

mod concrete {
    use super::ffi;

    trait Assoc {
        type Item;
    }

    struct Host;

    impl Assoc for Host {
        type Item = u16;
    }

    ffi! {
        #![unsafe(export("C"))]

        impl Assoc for Host {
            type Item = u8;
        }
    }
}

mod static_selection {
    use super::ffi;

    trait Assoc {
        type Item;
    }

    struct Host<T>(core::marker::PhantomData<T>);

    impl Assoc for Host<u8> {
        type Item = u8;
    }

    impl Assoc for Host<u16> {
        type Item = u16;
    }

    ffi! {
        #![unsafe(export("C"))]

        impl<T> Assoc for Host<T>
        where
            use<T> @ (<u8> | <u16>),
        {
            type Item = u8;
        }
    }
}

mod runtime_selection {
    use super::{Tag, ffi};

    #[derive(Tag)]
    #[tag(u8, unsafe(1))]
    struct First;

    #[derive(Tag)]
    #[tag(u8, unsafe(2))]
    struct Second;

    trait Assoc {
        type Item;
    }

    struct Host<T>(core::marker::PhantomData<T>);

    impl Assoc for Host<First> {
        type Item = First;
    }

    impl Assoc for Host<Second> {
        type Item = First;
    }

    ffi! {
        #![unsafe(export("C"))]

        impl<dyn(u8) T> Assoc for Host<T>
        where
            use<T> @ (<First> | <Second>),
        {
            type Item = T;
        }
    }
}

mod generic_associated_type {
    use super::ffi;

    trait Assoc {
        type Item<T>;
    }

    struct Host;

    impl Assoc for Host {
        type Item<T> = (T, u16);
    }

    ffi! {
        #![unsafe(export("C"))]

        impl Assoc for Host {
            type Item<T> = (T, u8);
        }
    }
}

mod lifetime_associated_type {
    use super::ffi;

    trait Assoc {
        type Item<'a>
        where
            Self: 'a;
    }

    struct Host;

    impl Assoc for Host {
        type Item<'a> = &'static u8;
    }

    ffi! {
        #![unsafe(export("C"))]

        impl Assoc for Host {
            type Item<'a> = &'a u8;
        }
    }
}

mod const_generic_associated_type {
    use super::ffi;

    trait Assoc {
        type Item<const N: usize>;
    }

    struct Host;

    impl Assoc for Host {
        type Item<const N: usize> = [u16; N];
    }

    ffi! {
        #![unsafe(export("C"))]

        impl Assoc for Host {
            type Item<const N: usize> = [u8; N];
        }
    }
}

mod generic_where_self_projection {
    use super::ffi;

    trait Assoc {
        type Base;
        type Item<T>
        where
            Self::Base: Sized;
    }

    struct Host;

    impl Assoc for Host {
        type Base = u8;
        type Item<T>
            = (T, u16)
        where
            Self::Base: Sized;
    }

    ffi! {
        #![unsafe(export("C"))]

        impl Assoc for Host {
            type Base = u8;
            type Item<T> = (T, u8) where Self::Base: Sized;
        }
    }
}

mod self_projection {
    use super::ffi;

    trait Assoc {
        type Base;
        type Item;
    }

    struct Host;

    impl Assoc for Host {
        type Base = u8;
        type Item = u16;
    }

    ffi! {
        #![unsafe(export("C"))]

        impl Assoc for Host {
            type Base = u8;
            type Item = <Self as Assoc>::Base;
        }
    }
}

mod unqualified_self_projection {
    use super::ffi;

    trait Assoc {
        type Base;
        type Item;
        type Nested;
    }

    struct Host;

    impl Assoc for Host {
        type Base = u8;
        type Item = u16;
        type Nested = Option<u16>;
    }

    ffi! {
        #![unsafe(export("C"))]

        impl Assoc for Host {
            type Base = u8;
            type Item = Self::Base;
            type Nested = Option<Self::Base>;
        }
    }
}

mod unsized_associated_type {
    use super::ffi;

    trait Assoc {
        type Item: ?Sized;
    }

    struct Host;

    impl Assoc for Host {
        type Item = [u8];
    }

    ffi! {
        #![unsafe(export("C"))]

        impl Assoc for Host {
            type Item = str;
        }
    }
}

mod missing_impl {
    use super::ffi;

    trait Assoc {
        type Item;
    }

    struct Host;

    ffi! {
        #![unsafe(export("C"))]

        impl Assoc for Host {
            type Item = u8;
        }
    }
}

mod wrong_associated_name {
    use super::ffi;

    trait Assoc {
        type Item;
    }

    struct Host;

    impl Assoc for Host {
        type Item = u8;
    }

    ffi! {
        #![unsafe(export("C"))]

        impl Assoc for Host {
            type Other = u8;
        }
    }
}

mod cfg_disabled {
    use super::ffi;

    trait Assoc {
        type Item;
    }

    struct Host;

    impl Assoc for Host {
        type Item = u16;
    }

    ffi! {
        #![unsafe(export("C"))]

        impl Assoc for Host {
            #[cfg(any())]
            type Item = u8;
        }
    }
}

fn main() {}
