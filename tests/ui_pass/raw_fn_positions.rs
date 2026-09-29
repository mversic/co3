use co3::ffi;

macro_rules! macro_owned_type {
    (raw fn($($arg:tt)*)) => { u8 };
}

unsafe extern "C" fn callback(_: u8) {}

#[unsafe(export_name = "raw_fn_positions__CALLBACK")]
pub static CALLBACK_IMPL: Option<unsafe extern "C" fn(u8)> = Some(callback);

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "raw_fn_positions"]

    fn use_callback(callback: raw fn());
    fn use_explicit(callback: raw extern "system" fn(u8));
    fn return_callback() -> raw fn(u8);
    fn use_pair(pair: (u8, raw fn(u8)));
    fn use_option(callback: Option<raw fn(u8)>);
    fn use_macro(callback: macro_owned_type!(raw fn(u8)));

    type GenericDefault<T = raw fn()> = T;
    type InsideMacro = macro_owned_type!(raw fn());

    type Api;
    impl Api {
        fn use_callback(&self, callback: raw fn());
    }

    static CALLBACK: raw fn(u8);
}

fn main() {
    let _: Option<unsafe extern "C" fn(u8)> = CALLBACK.read();
    let _: GenericDefault = None;
    let _: InsideMacro = 0u8;
}
