use co3::{ReprC, wide::Wide};

mod public {
    use co3::{ReprC, rust_spec::RustSpec};

    // These names must not collide with any of the generated items.
    pub struct CValue;
    pub struct PacketData;
    pub struct CPacketData;
    pub struct CPacket;
    pub struct CChoice;
    pub struct COwnedHandle;

    co3::ffi! {
        #![unsafe(extern("C"))]
        pub type Handle;
    }

    #[derive(RustSpec, ReprC)]
    pub struct Value<T>(pub T);

    #[derive(RustSpec, ReprC)]
    #[repr(C)]
    pub struct Packet {
        pub tag: u8,
        pub tail: [u8],
    }

    #[derive(RustSpec, ReprC)]
    #[repr(u8)]
    pub enum Choice {
        A(u32) = 1,
        B,
    }
}

fn main() {
    let _ = (
        public::CValue,
        public::PacketData,
        public::CPacketData,
        public::CPacket,
        public::CChoice,
        public::COwnedHandle,
    );
    static_assertions::assert_impl_all!(<public::OwnedHandle as ReprC>::CType: co3::CType);
    let carrier: <public::Value<u32> as ReprC>::CType = co3::encode(public::Value(7));
    let decoded = unsafe { co3::decode::<public::Value<u32>>(carrier) }.unwrap();
    assert_eq!(decoded.0, 7);

    let header = unsafe { <public::Packet as Wide>::Header::from_fields(3, []) };
    let (tag, tail) = header.into_fields();
    assert_eq!(tag, 3);
    assert!(tail.is_empty());
    let header = unsafe { <public::Packet as Wide>::Header::from_fields(3, []) };
    let lowered: <<public::Packet as Wide>::Header as ReprC>::CType = co3::encode(header);
    assert_eq!(lowered.into_fields().0, 3);

    let carrier = <public::Choice as ReprC>::CType::A(42);
    assert!(matches!(
        unsafe { co3::decode::<public::Choice>(carrier) },
        Some(public::Choice::A(42))
    ));
}
