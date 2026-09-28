use co3::{Tag, ffi};

trait Value {
    type Item;
    fn value();
}

struct Host<T>(core::marker::PhantomData<T>);

impl Value for Host<u8> {
    type Item = u8;
    fn value() {}
}

impl Value for Host<u16> {
    type Item = u16;
    fn value() {}
}

trait Family {
    type Item<T>;
    fn value();
}

struct FamilyHost;

impl Family for FamilyHost {
    type Item<T> = (T, u8);
    fn value() {}
}

trait LifetimeFamily {
    type Item<'a>
    where
        Self: 'a;
    fn value();
}

impl LifetimeFamily for FamilyHost {
    type Item<'a> = &'a u8;
    fn value() {}
}

trait Marker {
    type Item;
}

struct MarkerHost;

impl Marker for MarkerHost {
    type Item = u8;
}

#[derive(Tag)]
#[tag(u8, unsafe(1))]
struct Tagged;

impl Marker for Host<Tagged> {
    type Item = Tagged;
}

trait Projection {
    type Base;
    type Item;
}

struct ProjectionHost;

impl Projection for ProjectionHost {
    type Base = u8;
    type Item = u8;
}

trait Unsized {
    type Item: ?Sized;
}

struct UnsizedHost;

impl Unsized for UnsizedHost {
    type Item = str;
}

trait WhereProjection {
    type Base;
    type Item<T>
    where
        Self::Base: Sized;
}

struct WhereProjectionHost;

impl WhereProjection for WhereProjectionHost {
    type Base = u8;
    type Item<T>
        = (T, u8)
    where
        Self::Base: Sized;
}

ffi! {
    #![unsafe(export("C"))]

    impl<T> Value for Host<T>
    where
        use<T> @ (<u8> | <u16>),
    {
        type Item = T;
        fn value();
    }

    impl Family for FamilyHost {
        type Item<T> = (T, u8);
        fn value();
    }

    impl LifetimeFamily for FamilyHost {
        type Item<'a> = &'a u8;
        fn value();
    }

    impl Marker for MarkerHost {
        type Item = u8;
    }

    impl<dyn(u8) T> Marker for Host<T>
    where
        use<T> @ <Tagged>,
    {
        type Item = T;
    }

    impl Projection for ProjectionHost {
        type Base = u8;
        type Item = Self::Base;
    }

    impl Unsized for UnsizedHost {
        type Item = str;
    }

    impl WhereProjection for WhereProjectionHost {
        type Base = u8;
        type Item<T> = (T, u8) where Self::Base: Sized;
    }
}

fn main() {}
