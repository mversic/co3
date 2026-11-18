use co3::{ffi, ops::CFn1, raw, reference::Ref};

ffi! {
    #![unsafe(extern("C"))]

    fn direct(#[soft] value: &mut bool) -> &bool;

    fn shortened<'long: 'short, 'short>(
        #[soft] value: &'long mut bool,
        unrelated: &'short (),
    ) -> &'short bool;

    fn from_static<'a>(
        #[soft] value: &'static mut bool,
        unrelated: &'a (),
    ) -> &'a bool;
}

// Both directions use co3's built-in tuple conversion. The returned u8
// reference itself needs no conversion storage.
fn first(value: &(u8, bool)) -> &u8 {
    &value.0
}

fn explicit_first(value: &(u8, bool)) -> &u8 {
    &value.0
}

fn static_first(value: &(u8, bool)) -> &u8 {
    &value.0
}

raw! {
    fn first(#[soft] value: &(u8, bool)) -> &u8;
}

raw! {
    fn explicit_first<'a>(#[soft] value: &'a (u8, bool)) -> &'a u8;
}

// Claiming an unrelated lifetime must not bypass the source signature check.
raw! {
    fn static_first(#[soft] value: &(u8, bool)) -> &'static u8;
}

fn echo(value: &u32) -> &u32 {
    value
}

raw! {
    fn echo(value: &u32) -> &u32;
}

fn escape() -> &'static u32 {
    let value = 42_u32;
    type Callback<'a> = unsafe extern "C" fn(Ref<'a, u32>) -> Ref<'a, u32>;
    let callback: Callback<'_> = echo_raw;
    unsafe { callback.call(&value) }.unwrap()
}

fn main() {}
