use co3::{export, export_C, extern_C, handles, handle::HandleFamily};

trait Trait {}
struct Kita;

pub struct GenericHandle<'a, T, const N: usize>(&'a [T; N]);

impl HandleFamily for Kita {
    type Kind = u32;
}

handles! {
    for<'a> GenericHandle<'a, u32, 23>,
    Kita,
}

export_C! {
    #[id(u32)]
    pub type GenericHandle<'a, T, const N: usize>;

    #[dispatch(<Kita, 23>)]
    impl<'a, dyn(u32) U, const K: usize> Drop for GenericHandle<'a, U, K> {
        fn drop(&mut self);
    }
}

#[export("C")]
impl GenericHandle<'static, u32, 12> {
    pub fn export1<'a>(self) {}
}

#[export("C")]
impl<'a> GenericHandle<'a, u32, 12> {
    pub fn export2(self) {}
}

#[export("C")]
impl<T> GenericHandle<'static, T, 12> {
    pub fn export3(self) {}
}

#[export("C")]
impl<const N: usize> GenericHandle<'static, u32, N> {
    pub fn handle3(self) {}
}

#[export("C")]
pub extern "C" fn export1<'a>(v: &'a u32) -> &'a u32 {
    v
}

#[export("C")]
pub extern "C" fn export2<T>(v: T) -> T {
    v
}

#[export("C")]
pub extern "C" fn export3<const N: usize>(v: [u32; N]) -> [u32; N] {
    v
}

export_C! {
    #[dispatch(<Kita, 23>)]
    impl<'a, dyn(u32) U, const K: usize> Trait for GenericHandle<'a, U, K> {}
}

extern_C! {
    #![link(crate = "kita")]

    impl GenericHandle<'static, u32, 12> {
        pub fn extern1<'a>(self);
    }
    impl<'a> GenericHandle<'a, u32, 12> {
        pub fn extern2(self);
    }
    impl<T> GenericHandle<'static, T, 12> {
        pub fn extern3(self);
    }
    impl<const N: usize> GenericHandle<'static, u32, N> {
        pub fn handle3(self);
    }

    pub extern "C" fn extern1<'a>(v: &'a u32) -> &'a u32;
    pub extern "C" fn extern2<T>(v: T) -> T;
    pub extern "C" fn extern3<const N: usize>(v: [u32; N]) -> [u32; N];

    #[dispatch(<Kita, 23>)]
    impl<'a, dyn(u32) U, const K: usize> Trait for GenericHandle<'a, U, K> {
        fn drop(self_id: <dyn U>::ID, &mut self);
    }
}

fn main() {}
