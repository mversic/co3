// Drive the generated foreign-call protocol through one wake and one completion.
use std::sync::{mpsc, OnceLock};

#[cfg(feature = "boltffi")]
use boltffi::__private::rustfuture::RustFuturePoll;
#[cfg(feature = "uniffi")]
use uniffi::RustFuturePoll;

static SIGNAL: OnceLock<mpsc::Sender<bool>> = OnceLock::new();

extern "C" fn wake(_: u64, poll: RustFuturePoll) {
    if let Some(sender) = SIGNAL.get() {
        let _ = sender.send(matches!(poll, RustFuturePoll::Ready));
    }
}

pub fn check() {
    let (sender, receiver) = mpsc::channel();
    SIGNAL.set(sender).unwrap();
    #[cfg(feature = "uniffi")]
    let handle = super::uniffi_co3_framework_comparisons_fn_func_answer();
    #[cfg(feature = "boltffi")]
    let handle = super::boltffi_function_12async_export6answer();

    for expected_ready in [false, true] {
        #[cfg(feature = "uniffi")]
        unsafe {
            super::ffi_co3_framework_comparisons_rust_future_poll_u32(handle.clone(), wake, 0)
        };
        #[cfg(feature = "boltffi")]
        unsafe {
            super::boltffi_async_function_12async_export6answer_poll(handle, 0, wake)
        };
        let ready = receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!(ready, expected_ready);
    }
    #[cfg(feature = "uniffi")]
    unsafe {
        let mut status = uniffi::RustCallStatus::default();
        let value = super::ffi_co3_framework_comparisons_rust_future_complete_u32(
            handle.clone(),
            &mut status,
        );
        super::ffi_co3_framework_comparisons_rust_future_free_u32(handle);
        assert_eq!(status.code, uniffi::RustCallStatusCode::Success);
        assert_eq!(value, 42);
    }
    #[cfg(feature = "boltffi")]
    unsafe {
        let mut status = boltffi::__private::FfiStatus::default();
        let value =
            super::boltffi_async_function_12async_export6answer_complete(handle, &mut status);
        super::boltffi_async_function_12async_export6answer_free(handle);
        assert!(status.is_ok());
        assert_eq!(value, 42);
    }
}
