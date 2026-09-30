use co3::ffi;

struct Exported<'a>(std::cell::Cell<&'a ()>);

ffi! {
    #![unsafe(export("C"))]

    #[covariant('a)]
    type Exported<'a>;
}

fn main() {}
