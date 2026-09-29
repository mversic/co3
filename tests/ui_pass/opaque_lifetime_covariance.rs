use co3::ffi;

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
