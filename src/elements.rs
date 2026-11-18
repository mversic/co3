//! Element traversal shared by structural conversions. Callers supply the
//! checked or unchecked conversion and retain its trait and access contracts.

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};

pub(crate) fn map_array<'a, T, U, S, const N: usize>(
    values: [T; N],
    stores: &'a mut [S; N],
    mut convert: impl FnMut(T, &'a mut S) -> U,
) -> [U; N] {
    let mut stores = stores.iter_mut();
    values.map(|value| convert(value, stores.next().unwrap()))
}

pub(crate) fn try_map_array<'a, T, U, S, const N: usize>(
    values: [T; N],
    stores: &'a mut [S; N],
    convert: impl FnMut(T, &'a mut S) -> Option<U>,
) -> Option<[U; N]> {
    let converted = map_array(values, stores, convert);
    if converted.iter().any(Option::is_none) {
        return None;
    }
    Some(converted.map(Option::unwrap))
}

#[cfg(feature = "alloc")]
fn zip_stores<I: ExactSizeIterator, S: Default>(
    values: I,
    stores: &mut Box<[S]>,
) -> impl ExactSizeIterator<Item = (I::Item, &mut S)> {
    *stores = core::iter::repeat_with(S::default)
        .take(values.len())
        .collect();
    values.zip(stores.iter_mut())
}

#[cfg(feature = "alloc")]
pub(crate) fn map_boxed_slice<'a, I: ExactSizeIterator, U, S: Default>(
    values: I,
    stores: &'a mut Box<[S]>,
    mut convert: impl FnMut(I::Item, &'a mut S) -> U,
) -> Box<[U]> {
    zip_stores(values, stores)
        .map(|(value, store)| convert(value, store))
        .collect()
}

#[cfg(feature = "alloc")]
pub(crate) fn map_vec<'a, I: ExactSizeIterator, U, S: Default>(
    values: I,
    capacity: usize,
    stores: &'a mut Box<[S]>,
    mut convert: impl FnMut(I::Item, &'a mut S) -> U,
) -> Vec<U> {
    let values = zip_stores(values, stores);
    let mut converted = Vec::with_capacity(capacity);
    converted.extend(values.map(|(value, store)| convert(value, store)));
    converted
}

/// Attempt every element, retaining successful values until all conversions
/// finish. A failed conversion drops those values and leaves every store alive.
#[cfg(feature = "alloc")]
pub(crate) fn try_map_vec<'a, I: ExactSizeIterator, U, S: Default>(
    values: I,
    capacity: usize,
    stores: &'a mut Box<[S]>,
    mut convert: impl FnMut(I::Item, &'a mut S) -> Option<U>,
) -> Option<Vec<U>> {
    let values = zip_stores(values, stores);
    let mut converted = Vec::with_capacity(capacity);
    let mut valid = true;
    for (value, store) in values {
        if let Some(value) = convert(value, store) {
            converted.push(value);
        } else {
            valid = false;
        }
    }
    valid.then_some(converted)
}

#[cfg(all(test, feature = "alloc"))]
mod tests {
    use core::cell::Cell;

    use super::*;

    struct CountDrop<'a>(&'a Cell<usize>);
    impl Drop for CountDrop<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn failure_finishes_conversion_before_dropping_successful_values() {
        let drops = Cell::new(0);
        let mut stores = Box::default();
        let result = try_map_vec(0..3, 8, &mut stores, |index, store| {
            assert_eq!(drops.get(), 0);
            *store = Some(index);
            (index != 1).then(|| CountDrop(&drops))
        });
        assert!(result.is_none());
        assert_eq!(&*stores, &[Some(0), Some(1), Some(2)]);
        assert_eq!(drops.get(), 2);
    }
}
