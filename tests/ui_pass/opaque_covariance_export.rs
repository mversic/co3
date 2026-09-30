use co3::ffi;

struct Exported<'a, 'b>(&'a u8, &'b u8);

struct Bounded<'a, 'b>(&'a u8, &'b u8)
where
    'a: 'b;

ffi! {
    #![unsafe(export("C"))]

    #[covariant('a, 'b)]
    type Exported<'a, 'b>;

    #[covariant('a)]
    type Bounded<'a, 'b> where 'a: 'b;
}

ffi! {
    #![unsafe(extern("C"))]

    #[covariant('a)]
    type Borrowed<'a>;

    impl<'a> Drop for Borrowed<'a> {
        fn drop(&mut self);
    }
}

fn shorten_lifetime<'short>(
    value: &'short Borrowed<'static>,
    _: &'short u8,
) -> &'short Borrowed<'short> {
    value
}

fn main() {}
