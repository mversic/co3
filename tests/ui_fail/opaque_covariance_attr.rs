use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    #[covariant('missing)]
    type Unknown<'a>;
}

ffi! {
    #![unsafe(extern("C"))]

    #[covariant('a, 'a)]
    type Duplicate<'a>;
}

ffi! {
    #![unsafe(extern("C"))]

    #[covariant(T)]
    type TypeParameter<T>;
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(covariant('a))]
    type OldSyntax<'a>;
}

fn main() {}
