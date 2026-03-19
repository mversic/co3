use co3::{export, export_C, extern_C, handles};

pub struct GenericHandle<'a, T, const N: usize>(&'a [T; N]);

handles! {
    for<'a> GenericHandle<'a, u32, 23>,
}

export_C! {
    #[id(u32)]
    pub type GenericHandle<'a, T, const N: usize>;

    #[dispatch(<u32, 23>)]
    impl<'a, U, const K: usize> Drop for GenericHandle<'a, U, K> {
        fn drop(self_id: Self::ID, &mut dyn self);
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
}

// FIXME: Check test output .stderr, there is an issue about lifetimes that shouldn't be there
fn main() {}
