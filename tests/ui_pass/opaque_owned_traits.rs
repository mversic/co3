use co3::ffi;

trait Marker {}

ffi! {
    #![unsafe(extern("C"))]

    type Thing;
    type SendOnly;
    type SyncOnly;
    type Neither;
    type Generic<T>;
    type Value;
    type Numbers;

    impl Marker for Thing {}

    impl<T> Drop for Generic<T> {
        fn drop(&mut self);
    }

    impl PartialEq for Value {
        fn eq(&self, other: &Self) -> bool;
    }

    impl Eq for Value {}

    impl PartialOrd for Value {
        fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering>;
    }

    impl Ord for Value {
        fn cmp(&self, other: &Self) -> core::cmp::Ordering;
    }

    impl Iterator for Numbers {
        type Item = u8;

        fn next(&mut self) -> Option<u8>;
        fn size_hint(&self) -> (usize, Option<usize>);
    }

    impl DoubleEndedIterator for Numbers {
        fn next_back(&mut self) -> Option<u8>;
    }

    impl ExactSizeIterator for Numbers {
        fn len(&self) -> usize;
    }

    impl core::iter::FusedIterator for Numbers {}
}

unsafe impl Send for SendOnly {}
unsafe impl Sync for SyncOnly {}

fn assert_marker<T: Marker + ?Sized>() {}
fn assert_sync<T: Sync>() {}

fn compares_owned(left: OwnedValue, right: OwnedValue) {
    let _ = left == right;
    let _ = left < right;
    let _ = left.cmp(&right);
    fn assert_eq<T: Eq>(_: &T) {}
    assert_eq(&left);
    let _ = format!("{:p}", left);
}

fn consumes_owned(numbers: OwnedNumbers) {
    let _: Vec<u8> = numbers.collect();
}

fn uses_iterator_traits(mut numbers: OwnedNumbers) {
    let _ = numbers.next_back();
    let _ = numbers.len();
    fn fused<T: core::iter::FusedIterator>(_: &T) {}
    fused(&numbers);
    let _ = format!("{:p}", numbers);
}

fn main() {
    assert_marker::<Thing>();
    assert_sync::<OwnedSyncOnly>();

    static_assertions::assert_not_impl_any!(OwnedSendOnly: Send);
    static_assertions::assert_not_impl_any!(OwnedSendOnly: Sync);
    static_assertions::assert_not_impl_any!(OwnedSyncOnly: Send);
    static_assertions::assert_not_impl_any!(OwnedNeither: Send, Sync);
    static_assertions::assert_impl_all!(
        OwnedGeneric<u8>: Unpin, std::panic::UnwindSafe, std::panic::RefUnwindSafe
    );
    static_assertions::assert_not_impl_any!(OwnedGeneric<&mut u8>: std::panic::UnwindSafe);
    static_assertions::assert_not_impl_any!(OwnedGeneric<std::cell::UnsafeCell<u8>>: std::panic::RefUnwindSafe);

    let _ = compares_owned as fn(OwnedValue, OwnedValue);
    let _ = consumes_owned as fn(OwnedNumbers);
    let _ = uses_iterator_traits as fn(OwnedNumbers);
}
