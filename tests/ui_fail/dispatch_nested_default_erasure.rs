use co3::{Tag, ffi};

struct Wrapper<T>(T);

#[derive(Tag)]
#[tag(u8, unsafe(1))]
struct Value;

ffi! {
    #![unsafe(extern("C"))]

    fn nested<dyn(u8) T = u16>(value: move Wrapper<T>)
    where
        use<T> @ <Value>;
}

fn main() {}
