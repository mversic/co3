use core::cell::{Cell, UnsafeCell};

use co3::{ffi, raw, reference::CRef};

fn overwrite(bytes: &mut [u8]) -> usize {
    bytes.fill(7);
    bytes.len()
}

fn narrow(value: u8) -> u8 {
    value + 1
}

fn sum_cells(values: &[Cell<u8>]) -> u8 {
    values.iter().map(Cell::get).sum()
}

raw! {
    fn overwrite(#[unpack(_, _)] bytes: &mut [u8]) -> usize;
    fn narrow(#[unpack(u16)] value: u8) -> u8;
    fn sum_cells(#[unpack(*mut UnsafeCell<u8>, usize)] values: &[Cell<u8>]) -> u8;
}

#[test]
fn raw_companion_reconstructs_unpacked_slice() {
    let callback: unsafe extern "C" fn(*mut u8, usize) -> usize = overwrite_raw;
    let mut bytes = [0_u8; 3];
    assert_eq!(
        unsafe { callback(bytes.as_mut_ptr(), bytes.len()) },
        3
    );
    assert_eq!(bytes, [7; 3]);
    let narrow_callback: unsafe extern "C" fn(u16) -> u8 = narrow_raw;
    assert_eq!(unsafe { narrow_callback(41) }, 42);
}

#[unsafe(export_name = "raw_wrapper__value")]
extern "C" fn value_source(value: u8) -> u8 {
    value + 1
}

#[unsafe(export_name = "raw_wrapper__Host__method")]
extern "C" fn method_source(value: u8) -> u8 {
    value + 2
}

#[unsafe(export_name = "raw_wrapper__borrowed")]
extern "C" fn borrowed_source(value: *const u8) -> *const u8 {
    value
}

#[unsafe(export_name = "raw_wrapper__interior")]
extern "C" fn interior_source(data: *mut UnsafeCell<u8>, len: usize) -> u8 {
    sum_cells(unsafe { core::slice::from_raw_parts(data.cast::<Cell<u8>>(), len) })
}

struct Host;

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "raw_wrapper"]

    raw extern "Rust" fn value(value: u8) -> u8;
    raw fn borrowed<'a>(value: &'a u8) -> &'a u8;

    type InteriorUnpacked = raw extern "C" fn(#[unpack(*mut UnsafeCell<u8>, usize)] &[Cell<u8>]) -> u8;
    fn interior(#[unpack(*mut UnsafeCell<u8>, usize)] values: &[Cell<u8>]) -> u8;

    impl Host {
        raw extern "system" fn method(value: u8) -> u8;
    }
}

#[test]
fn interior_slice_unpack_uses_plain_mutable_pointer() {
    let values = [Cell::new(7), Cell::new(11)];
    let callback: InteriorUnpacked = sum_cells_raw;
    assert_eq!(
        unsafe {
            callback(
                values.as_ptr().cast_mut().cast::<UnsafeCell<u8>>(),
                values.len(),
            )
        },
        18
    );
    assert_eq!(interior(&values), 18);
    assert_eq!(values.each_ref().map(Cell::get), [7, 11]);
}

#[test]
fn explicit_abi_wraps_raw_imports() {
    let _: unsafe extern "Rust" fn(u8) -> u8 = value;
    let _: unsafe extern "system" fn(u8) -> u8 = Host::method;
    fn assert_borrowed_signature<'a>() {
        let _: unsafe extern "C" fn(CRef<'a, u8>) -> CRef<'a, u8> = borrowed;
    }
    assert_borrowed_signature();
    assert_eq!(unsafe { value(4) }, 5);
    assert_eq!(unsafe { Host::method(4) }, 6);
    let input = 7;
    assert_eq!(
        unsafe { borrowed(CRef::from_ref(&input)) },
        CRef::from_ref(&input)
    );
}
