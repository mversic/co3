use co3::ExternC;

#[derive(ExternC)]
#[repr(C)]
struct Foo<T>(T);

#[derive(ExternC)]
#[repr(C)]
enum Bar {
    A
}

fn main() {
    let foo = Foo(());
    let bar = Bar::A;

    // Must fail for ZSTs
    assert!(foo.encode(&mut ()));
    assert!(bar.encode(&mut ()));
}
