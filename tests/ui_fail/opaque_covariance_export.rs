use co3::ffi;

ffi! {
    #![unsafe(export("C"))]

    #[covariant('a)]
    type Exported<'a>;
}

fn main() {}
