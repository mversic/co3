use core::num::NonZeroU8;

use co3::{Error, ReprC, ffi, rust_spec::RustSpec};

#[derive(Debug, PartialEq, Eq, RustSpec, ReprC)]
#[repr(u8)]
enum CustomStatus {
    Ok,
    TrapValue,
    UnknownHandle,
    SoftSyncError,
}

impl Error for CustomStatus {
    fn trap_value() -> Self {
        Self::TrapValue
    }

    fn unknown_tag() -> Self {
        Self::UnknownHandle
    }

    fn soft_sync_error() -> Self {
        Self::SoftSyncError
    }
}

fn export_error_input(value: NonZeroU8) -> CustomStatus {
    if value.get() == 11 {
        CustomStatus::UnknownHandle
    } else {
        CustomStatus::Ok
    }
}

ffi! {
    #![unsafe(export("C"))]
    #![failure = "error"]
    #![symbol_prefix = "kita_failure"]

    fn export_error_input(value: NonZeroU8) -> CustomStatus;
}

#[unsafe(export_name = "kita_failure__extern_error_invalid_return")]
unsafe extern "C" fn extern_error_invalid_return() -> <CustomStatus as ReprC>::CType {
    CCustomStatus(7)
}

mod import {
    use co3::ffi;

    ffi! {
        #![unsafe(extern("C"))]
        #![failure = "error"]
        #![symbol_prefix = "kita_failure"]

        pub fn extern_error_invalid_return() -> super::CustomStatus;
    }
}

unsafe extern "C" {
    #[link_name = "kita_failure__export_error_input"]
    fn export_error_input_raw(value: u8) -> <CustomStatus as ReprC>::CType;
}

fn decode_status(source: <CustomStatus as ReprC>::CType) -> CustomStatus {
    unsafe { co3::decode(source) }.expect("status should decode")
}

#[test]
fn error_failure_mode_returns_status_on_export_decode_failure() {
    let status = unsafe { export_error_input_raw(0) };

    assert_eq!(CustomStatus::trap_value(), decode_status(status));
}

#[test]
fn error_failure_mode_preserves_export_success_value() {
    let status = unsafe { export_error_input_raw(11) };

    assert_eq!(CustomStatus::UnknownHandle, decode_status(status));
}

#[test]
fn error_failure_mode_returns_status_on_extern_return_decode_failure() {
    assert_eq!(
        CustomStatus::trap_value(),
        import::extern_error_invalid_return()
    );
}
