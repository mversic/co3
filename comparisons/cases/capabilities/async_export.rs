// Async export is a capability test, not an equivalent-type spelling test.
include!("../support/mod.rs");

// Force a pending poll and a wake before the export can return its value.
async fn deferred_answer() -> u32 {
    let mut first = true;
    std::future::poll_fn(|cx| {
        if std::mem::take(&mut first) {
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(42)
        }
    })
    .await
}

#[cfg(not(feature = "safer_ffi"))]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
pub async fn answer() -> u32 {
    crate::deferred_answer().await
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub async fn answer() -> u32 {
        crate::deferred_answer().await
    }
}

#[cfg(feature = "safer_ffi")]
compile_error!("This fixture needs a safer-ffi async executor and ffi_await! adapter.");

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]
    async fn answer() -> u32;
}

#[cfg(any(
    feature = "co3",
    feature = "diplomat",
    feature = "interoptopus",
    feature = "safer_ffi"
))]
fn main() {}

#[cfg(feature = "uniffi")]
fn main() {
    use std::sync::{mpsc, OnceLock};

    use uniffi::RustFuturePoll;

    static SIGNAL: OnceLock<mpsc::Sender<bool>> = OnceLock::new();

    extern "C" fn wake(_: u64, poll: RustFuturePoll) {
        if let Some(sender) = SIGNAL.get() {
            let _ = sender.send(matches!(poll, RustFuturePoll::Ready));
        }
    }

    let (sender, receiver) = mpsc::channel();
    SIGNAL.set(sender).unwrap();
    let handle = crate::uniffi_co3_framework_comparisons_fn_func_answer();

    validate_poll(|| {
        unsafe {
            crate::ffi_co3_framework_comparisons_rust_future_poll_u32(handle.clone(), wake, 0)
        };

        let ready = receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        ready
    });
    validate_result(|| unsafe {
        let mut status = uniffi::RustCallStatus::default();
        let value = crate::ffi_co3_framework_comparisons_rust_future_complete_u32(
            handle.clone(),
            &mut status,
        );
        crate::ffi_co3_framework_comparisons_rust_future_free_u32(handle);
        assert_eq!(status.code, uniffi::RustCallStatusCode::Success);
        value
    });
}

#[cfg(feature = "boltffi")]
fn main() {
    use std::sync::{mpsc, OnceLock};

    use boltffi::__private::rustfuture::RustFuturePoll;

    static SIGNAL: OnceLock<mpsc::Sender<bool>> = OnceLock::new();

    extern "C" fn wake(_: u64, poll: RustFuturePoll) {
        if let Some(sender) = SIGNAL.get() {
            let _ = sender.send(matches!(poll, RustFuturePoll::Ready));
        }
    }

    let (sender, receiver) = mpsc::channel();
    SIGNAL.set(sender).unwrap();

    let handle = crate::boltffi_function_12async_export6answer();

    validate_poll(|| {
        unsafe { crate::boltffi_async_function_12async_export6answer_poll(handle, 0, wake) };
        let ready = receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        ready
    });

    validate_result(|| unsafe {
        let mut status = boltffi::__private::FfiStatus::default();
        let value =
            crate::boltffi_async_function_12async_export6answer_complete(handle, &mut status);
        crate::boltffi_async_function_12async_export6answer_free(handle);
        assert!(status.is_ok());
        value
    });
}

fn validate_poll(mut poll: impl FnMut() -> bool) {
    for expected_ready in [false, true] {
        assert_eq!(poll(), expected_ready);
    }
}

fn validate_result(complete: impl FnOnce() -> u32) {
    assert_eq!(complete(), 42);
}
