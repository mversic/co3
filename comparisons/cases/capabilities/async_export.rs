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

#[cfg(any(feature = "uniffi", feature = "boltffi"))]
#[path = "../support/async_caller.rs"]
mod caller;

#[cfg(any(
    feature = "co3",
    feature = "diplomat",
    feature = "interoptopus",
    feature = "safer_ffi",
    feature = "abi_stable",
    feature = "stabby"
))]
fn main() {}

#[cfg(any(feature = "uniffi", feature = "boltffi"))]
fn main() {
    caller::check();
}
