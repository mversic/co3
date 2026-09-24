use co3::ffi;

trait Custom {
    fn inc(&mut self, by: u8);
}

trait CustomDynSelf {
    fn set(&mut self, value: u8);
}

unsafe impl co3::tag::Tagged for Opaque<bool, u8> {
    const TAG: u8 = 1;
}
unsafe impl co3::tag::Tagged for Opaque<bool, u32> {
    const TAG: u8 = 2;
}
unsafe impl co3::tag::Tagged for Opaque<u8, bool> {
    const TAG: u8 = 3;
}

ffi! {
    #![unsafe(extern("C"))]

    #[tag(u8)]
    type Opaque<T, U>;

    impl<T, U> Drop for dyn Opaque<T, U>
    where
        use<T, U> @ (<bool, u8> | <u8, bool>),
    {
        #[symbol_name = "handles_drop"]
        fn drop(&mut self);
    }

    impl Default for OwnedOpaque<bool, u8> {
        #[symbol_name = "handles_default_bool_u8"]
        fn default() -> move Self;
    }

    impl Default for OwnedOpaque<u8, bool> {
        #[symbol_name = "handles_default_u8_bool"]
        fn default() -> move Self;
    }

    impl Clone for OwnedOpaque<bool, u8> {
        #[symbol_name = "handles_clone_bool_u8"]
        fn clone(&self) -> move Self;
    }

    impl<dyn(u8) T> PartialEq for T
    where
        use<T> @ (<Opaque<bool, u8>> | <Opaque<u8, bool>>),
    {
        #[symbol_name = "handles_eq"]
        fn eq(t_id: <dyn T>::TAG, &self, other: &Self) -> bool;
    }

    impl<dyn(u8) T, dyn(u8) U> PartialEq<U> for T
    where
        use<T, U> @ <Opaque<bool, u8>, Opaque<u8, bool>>,
    {
        #[symbol_name = "handles_cross_eq"]
        fn eq(t_id: <dyn T>::TAG, u_id: <dyn U>::TAG, &self, other: &U) -> bool;
    }

    impl<dyn(u8) T> Custom for T
    where
        use<T> @ (<Opaque<bool, u8>> | <Opaque<u8, bool>>),
    {
        #[symbol_name = "handles_inc"]
        fn inc(t_id: <dyn T>::TAG, &mut self, by: u8);
    }

    impl<T, U> CustomDynSelf for dyn Opaque<T, U>
    where
        use<T, U> @ (<bool, u8> | <u8, bool>),
    {
        #[symbol_name = "handles_set"]
        fn set(&mut self, value: u8);
    }
}

mod provider {
    use core::marker::PhantomData;
    use std::sync::{
        Mutex, MutexGuard,
        atomic::{AtomicUsize, Ordering},
    };

    use co3::ffi;

    use super::*;

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
    static DROP_TEST_LOCK: Mutex<()> = Mutex::new(());

    pub(super) fn lock_drop_test() -> MutexGuard<'static, ()> {
        DROP_TEST_LOCK.lock().unwrap()
    }

    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    struct Opaque<T, U> {
        value: u8,
        marker: PhantomData<(T, U)>,
    }

    impl<T, U> Drop for Opaque<T, U> {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(super) fn reset_drop_count() {
        DROP_COUNT.store(0, Ordering::Relaxed);
    }

    pub(super) fn drop_count() -> usize {
        DROP_COUNT.load(Ordering::Relaxed)
    }

    unsafe impl co3::tag::Tagged for Opaque<bool, u8> {
        const TAG: u8 = 1;
    }
    unsafe impl co3::tag::Tagged for Opaque<bool, u32> {
        const TAG: u8 = 2;
    }
    unsafe impl co3::tag::Tagged for Opaque<u8, bool> {
        const TAG: u8 = 3;
    }

    impl PartialEq<Opaque<u8, bool>> for Opaque<bool, u8> {
        fn eq(&self, other: &Opaque<u8, bool>) -> bool {
            self.value == other.value
        }
    }

    impl<T, U> Custom for Opaque<T, U> {
        fn inc(&mut self, by: u8) {
            self.value += by;
        }
    }

    impl<T, U> CustomDynSelf for Opaque<T, U> {
        fn set(&mut self, value: u8) {
            self.value = value;
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #[tag(u8)]
        type Opaque<T, U>;

        impl<T, U> Drop for dyn Opaque<T, U>
        where
            use<T, U> @ (<bool, u8> | <u8, bool>),
        {
            #[symbol_name = "handles_drop"]
            fn drop(&mut self);
        }

        impl Default for Box<Opaque<bool, u8>> {
            #[symbol_name = "handles_default_bool_u8"]
            fn default() -> move Self;
        }

        impl Default for Box<Opaque<u8, bool>> {
            #[symbol_name = "handles_default_u8_bool"]
            fn default() -> move Self;
        }

        impl Clone for Box<Opaque<bool, u8>> {
            #[symbol_name = "handles_clone_bool_u8"]
            fn clone(&self) -> move Self;
        }

        impl<dyn(u8) T> PartialEq for T
        where
            use<T> @ (<Opaque<bool, u8>> | <Opaque<u8, bool>>),
        {
            #[symbol_name = "handles_eq"]
            fn eq(&self, other: &Self) -> bool;
        }

        impl<dyn(u8) T, dyn(u8) U> PartialEq<U> for T
        where
            use<T, U> @ <Opaque<bool, u8>, Opaque<u8, bool>>,
        {
            #[symbol_name = "handles_cross_eq"]
            fn eq(&self, other: &U) -> bool;
        }

        impl<dyn(u8) T> Custom for T
        where
            use<T> @ (<Opaque<bool, u8>> | <Opaque<u8, bool>>),
        {
            #[symbol_name = "handles_inc"]
            fn inc(&mut self, by: u8);
        }

        impl<T, U> CustomDynSelf for dyn Opaque<T, U>
        where
            use<T, U> @ (<bool, u8> | <u8, bool>),
        {
            #[symbol_name = "handles_set"]
            fn set(&mut self, value: u8);
        }
    }
}

#[test]
fn erased_handle_dispatch() {
    let _lock = provider::lock_drop_test();
    provider::reset_drop_count();
    {
        let mut handle: OwnedOpaque<bool, u8> = Default::default();
        let cloned = Clone::clone(&handle);
        assert!(PartialEq::eq(&*handle, &*cloned));

        let other: OwnedOpaque<u8, bool> = Default::default();
        assert!(PartialEq::<Opaque<u8, bool>>::eq(&*handle, &*other));

        Custom::inc(&mut *handle, 2);
        assert!(!PartialEq::<Opaque<u8, bool>>::eq(&*handle, &*other));
        assert!(!PartialEq::eq(&*handle, &*cloned));
    }
    assert_eq!(provider::drop_count(), 3);
}

#[test]
fn static_parameters_behind_dyn_self_do_not_bind_symbols() {
    let _lock = provider::lock_drop_test();
    let mut handle: OwnedOpaque<bool, u8> = Default::default();
    let other: OwnedOpaque<u8, bool> = Default::default();

    CustomDynSelf::set(&mut *handle, 7);

    assert!(!PartialEq::<Opaque<u8, bool>>::eq(&*handle, &*other));
}
