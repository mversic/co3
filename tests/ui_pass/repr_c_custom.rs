use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr_c(as(u32))]
struct Handle(u32);

impl From<Handle> for u32 {
    fn from(value: Handle) -> Self {
        value.0
    }
}

impl TryFrom<u32> for Handle {
    type Error = ();

    fn try_from(raw: u32) -> Result<Self, Self::Error> {
        (raw != 0).then_some(Self(raw)).ok_or(())
    }
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr_c(as(u8))]
enum Status {
    Ready,
    Busy,
}

impl From<Status> for u8 {
    fn from(value: Status) -> Self {
        match value {
            Status::Ready => 1,
            Status::Busy => 2,
        }
    }
}

impl TryFrom<u8> for Status {
    type Error = ();

    fn try_from(raw: u8) -> Result<Self, Self::Error> {
        match raw {
            1 => Ok(Self::Ready),
            2 => Ok(Self::Busy),
            _ => Err(()),
        }
    }
}

#[derive(ReprC)]
#[repr_c(as(u32))]
struct Wrapped<T>(T);

impl<T: Into<u32>> From<Wrapped<T>> for u32 {
    fn from(value: Wrapped<T>) -> Self {
        value.0.into()
    }
}

impl<T: TryFrom<u32>> TryFrom<u32> for Wrapped<T> {
    type Error = T::Error;

    fn try_from(raw: u32) -> Result<Self, Self::Error> {
        T::try_from(raw).map(Self)
    }
}

#[derive(RustSpec, ReprC)]
struct Intermediate(u32);

#[derive(ReprC)]
#[repr_c(as(Intermediate))]
struct Indirect(u32);

impl From<Indirect> for Intermediate {
    fn from(value: Indirect) -> Self {
        Self(value.0)
    }
}

impl From<Intermediate> for Indirect {
    fn from(value: Intermediate) -> Self {
        Self(value.0)
    }
}

#[derive(RustSpec, ReprC)]
struct GenericIntermediate<T>(T);

#[derive(ReprC)]
#[repr_c(as(GenericIntermediate<T>))]
struct GenericIndirect<T>(T);

impl<T> From<GenericIndirect<T>> for GenericIntermediate<T> {
    fn from(value: GenericIndirect<T>) -> Self {
        Self(value.0)
    }
}

impl<T> From<GenericIntermediate<T>> for GenericIndirect<T> {
    fn from(value: GenericIntermediate<T>) -> Self {
        Self(value.0)
    }
}

static_assertions::assert_type_eq_all!(<Handle as ReprC>::CType, u32);
static_assertions::assert_type_eq_all!(<Status as ReprC>::CType, u8);
static_assertions::assert_type_eq_all!(
    <Indirect as ReprC>::CType,
    <Intermediate as ReprC>::CType
);
static_assertions::assert_not_impl_any!(Handle: co3::transmute::CheckedTransmute);

ffi! {
    #![unsafe(export("C"))]
    fn round_trip(value: move Handle) -> move Handle;
}

fn round_trip(value: Handle) -> Handle {
    value
}

fn main() {
    assert_eq!(co3::encode(Handle(7)), 7);
    assert_eq!(unsafe { co3::decode::<Handle>(7) }.unwrap().0, 7);
    assert!(unsafe { co3::decode::<Handle>(0) }.is_none());
    assert_eq!(co3::encode(Status::Busy), 2);
    assert!(unsafe { co3::decode::<Status>(3) }.is_none());
    assert_eq!(co3::encode(Wrapped(7_u16)), 7);
    assert_eq!(unsafe { co3::decode::<Wrapped<u16>>(7) }.unwrap().0, 7);
    let indirect = Indirect(9);
    let raw = co3::encode(indirect);
    assert_eq!(unsafe { co3::decode::<Indirect>(raw) }.unwrap().0, 9);
    let raw = co3::encode(GenericIndirect(11_u16));
    assert_eq!(unsafe { co3::decode::<GenericIndirect<u16>>(raw) }.unwrap().0, 11);
    validation::check();
}

mod validation {
    use co3::{ReprC, rust_spec::RustSpec};

    #[derive(ReprC)]
    #[repr_c(as(u32), is_valid = |value| value.is_power_of_two())]
    struct Named {
        value: u32,
    }

    impl From<Named> for u32 {
        fn from(value: Named) -> Self {
            value.value
        }
    }

    impl From<u32> for Named {
        fn from(value: u32) -> Self {
            Self { value }
        }
    }

    #[derive(ReprC)]
    #[repr_c(as(u32))]
    enum NamedVariant {
        #[repr_c(is_valid = |value| value.is_power_of_two())]
        Power { value: u32 },
        Zero,
    }

    impl From<NamedVariant> for u32 {
        fn from(value: NamedVariant) -> Self {
            match value {
                NamedVariant::Power { value } => value,
                NamedVariant::Zero => 0,
            }
        }
    }

    impl From<u32> for NamedVariant {
        fn from(value: u32) -> Self {
            if value == 0 {
                Self::Zero
            } else {
                Self::Power { value }
            }
        }
    }

    #[derive(RustSpec, ReprC)]
    #[repr_c(is_valid = |value| value.is_power_of_two())]
    struct PowerOfTwo(u32);

    #[derive(RustSpec, ReprC)]
    #[repr(u8)]
    enum Number {
        #[repr_c(is_valid = |value| value.is_power_of_two())]
        PowerOfTwo(u32),
        Other,
    }

    pub(super) fn check() {
        let _ = PowerOfTwo(2);
        let _ = Number::PowerOfTwo(2);
        assert!(unsafe { co3::decode::<Named>(3) }.is_none());
        assert!(unsafe { co3::decode::<NamedVariant>(3) }.is_none());
    }
}
