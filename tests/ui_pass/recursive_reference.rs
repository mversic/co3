// RustSpec supports the recursive box. ReprC should preserve that support.

use co3::lang;

#[derive(co3::rust_spec::RustSpec)]
#[repr(C)]
struct RustNode {
    next: Box<RustNode>,
}

fn assert_rust_spec<T: co3::rust_spec::RustSpec>() {}

#[derive(co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(C)]
struct Node {
    next: Box<Node>,
}

#[derive(Debug, PartialEq, co3::rust_spec::RustSpec, co3::ReprC)]
#[repr(C)]
pub struct List {
    value: u8,
    next: Option<Box<Self>>,
}

#[derive(Debug, PartialEq, co3::rust_spec::RustSpec, co3::ReprC)]
struct Tree {
    value: u8,
    children: Vec<Tree>,
}

fn main() {
    assert_rust_spec::<RustNode>();

    let list = || List {
        value: 1,
        next: Some(Box::new(List {
            value: 2,
            next: None,
        })),
    };
    let encoded = unsafe { co3::encode::<_, lang::C>(list()) };
    assert_eq!(unsafe { co3::decode::<_, lang::C>(encoded) }, Some(list()));

    let mut owner = Default::default();
    let encoded = unsafe { co3::borrow::encode::<_, lang::C>(list(), &mut owner) };
    assert_eq!(
        unsafe { co3::borrow::decode::<_, lang::C>(encoded) },
        Some(list())
    );

    let tree = || Tree {
        value: 1,
        children: vec![
            Tree {
                value: 2,
                children: vec![],
            },
            Tree {
                value: 3,
                children: vec![],
            },
        ],
    };
    let encoded = unsafe { co3::encode::<_, lang::C>(tree()) };
    assert_eq!(unsafe { co3::decode::<_, lang::C>(encoded) }, Some(tree()));
}
