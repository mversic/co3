use co3::{ReprC, rust_spec::RustSpec, wide::Wide};

#[derive(Debug, PartialEq, RustSpec, ReprC)]
#[repr(C)]
struct List<T> {
    value: T,
    next: Option<Box<List<T>>>,
}

#[allow(unused_parens)]
#[derive(Debug, PartialEq, RustSpec, ReprC)]
#[repr(C)]
struct ParenthesizedList<T> {
    value: T,
    next: Option<(Box<Self>)>,
}

#[test]
fn parenthesized_recursive_allocation_round_trips() {
    let list = || ParenthesizedList {
        value: 3u32,
        next: Some(Box::new(ParenthesizedList {
            value: 8,
            next: None,
        })),
    };
    assert_eq!(unsafe { co3::decode(co3::encode(list())) }, Some(list()));
}

impl<T> List<T> {
    fn pair(first: T, second: T) -> Self {
        Self {
            value: first,
            next: Some(Box::new(Self {
                value: second,
                next: None,
            })),
        }
    }
}

#[test]
fn owned_generic_lists() {
    let numbers = || List::pair(3u32, 8);
    assert_eq!(
        unsafe { co3::decode(co3::encode(numbers())) },
        Some(numbers())
    );
    let flags = || List::pair(true, false);
    assert_eq!(unsafe { co3::decode(co3::encode(flags())) }, Some(flags()));
}

#[test]
fn borrowed_generic_list() {
    let flags = || List::pair(false, true);
    let mut owner = Default::default();
    let carrier = co3::borrow::encode(flags(), &mut owner);
    assert_eq!(unsafe { co3::borrow::decode(carrier) }, Some(flags()));
}

#[derive(Debug, PartialEq, RustSpec, ReprC)]
struct Tree<T> {
    value: T,
    children: Vec<Tree<T>>,
}

#[test]
fn generic_vector_tree() {
    let tree = || Tree {
        value: 1u32,
        children: vec![Tree {
            value: 2,
            children: vec![],
        }],
    };
    assert_eq!(unsafe { co3::decode(co3::encode(tree())) }, Some(tree()));
}

#[test]
fn repr_c_vector_tree_round_trips() {
    #[derive(Debug, PartialEq, RustSpec, ReprC)]
    #[repr(C)]
    struct Node {
        weight: u32,
        children: Vec<Node>,
    }

    static_assertions::assert_not_impl_any!(Node: co3::marker::AbiCompatible);
    let tree = || Node {
        weight: 1,
        children: vec![Node {
            weight: 2,
            children: vec![],
        }],
    };
    assert_eq!(unsafe { co3::decode(co3::encode(tree())) }, Some(tree()));
}

#[test]
fn repr_c_stable_vector_tree_is_abi_compatible() {
    use co3::{marker::AbiCompatible, rust_spec, vec::Vec};

    #[derive(Debug, PartialEq, ReprC)]
    #[repr(C)]
    struct Node {
        weight: u32,
        children: Vec<Self>,
    }

    // SAFETY: Each node has the same repr(C) fields. The vector owns the
    // children and requires a non-null data pointer. Neither field contains
    // interior-mutable storage, including through the child allocations.
    unsafe impl RustSpec for Node {
        type Layout = rust_spec::Stable;
        type Size = <u32 as RustSpec>::Size;
        type __Alignment = <Vec<u32> as RustSpec>::__Alignment;
        type Trap = rust_spec::layout::NonRobust;
        type Niche = rust_spec::niche::WithNiche<rust_spec::Unstable>;
        type Drop = rust_spec::drop::AutoDrop;
        type Mutability = <u32 as RustSpec>::Mutability;
        type Opacity = rust_spec::NonOpaque;
        type __IndirectTrap = Self::Trap;
    }

    static_assertions::assert_impl_all!(Node: AbiCompatible);

    let tree = || Node {
        weight: 1u32,
        children: vec![Node {
            weight: 2,
            children: Vec::new(),
        }]
        .into(),
    };
    assert_eq!(unsafe { co3::decode(co3::encode(tree())) }, Some(tree()));
}

#[derive(Debug, PartialEq, RustSpec, ReprC)]
struct OptionalChildren<T> {
    value: T,
    children: Option<Vec<Self>>,
}

#[test]
fn recursive_optional_vector_preserves_none_and_empty() {
    let tree = || OptionalChildren {
        value: 1u32,
        children: Some(vec![
            OptionalChildren {
                value: 2,
                children: None,
            },
            OptionalChildren {
                value: 3,
                children: Some(vec![]),
            },
        ]),
    };
    assert_eq!(unsafe { co3::decode(co3::encode(tree())) }, Some(tree()));
}

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Links<T> {
    value: T,
    next: *const Self,
    previous: *mut Self,
    parent: Option<core::ptr::NonNull<Self>>,
}

#[test]
fn recursive_raw_pointers_preserve_addresses_and_nulls() {
    let leaf = Links {
        value: 1u32,
        next: core::ptr::null(),
        previous: core::ptr::null_mut(),
        parent: None,
    };
    let mut leaf = unsafe { co3::decode::<Links<u32>>(co3::encode(leaf)) }.unwrap();
    assert!(leaf.next.is_null());
    assert!(leaf.previous.is_null());
    assert!(leaf.parent.is_none());

    let address = core::ptr::NonNull::from_mut(&mut leaf);
    let links = Links {
        value: 2u32,
        next: address.as_ptr().cast_const(),
        previous: address.as_ptr(),
        parent: Some(address),
    };
    let decoded = unsafe { co3::decode::<Links<u32>>(co3::encode(links)) }.unwrap();
    assert_eq!(decoded.next, address.as_ptr().cast_const());
    assert_eq!(decoded.previous, address.as_ptr());
    assert_eq!(decoded.parent, Some(address));
}

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct WideNode<T> {
    next: Option<Box<Self>>,
    payload: [T],
}

#[test]
fn recursive_wide_box_headers_preserve_ownership_and_tail_length() {
    type Node = WideNode<u16>;
    static_assertions::assert_impl_all!(Option<Box<Node>>: ReprC);
    static_assertions::assert_type_eq_all!(
        <<Node as Wide>::Header as ReprC>::CType,
        <<Node as ReprC>::CType as Wide>::Header
    );

    fn node<const N: usize>(next: Option<Box<Node>>, payload: [u16; N]) -> Box<Node> {
        #[repr(C)]
        struct Storage<const N: usize> {
            next: Option<Box<Node>>,
            payload: [u16; N],
        }
        let allocation = Box::into_non_null(Box::new(Storage { next, payload }));
        // Both records have the same repr(C) prefix and N initialized tail elements.
        unsafe { Node::from_non_null(allocation.cast(), N) }
    }
    let leaf = node(None, []);
    let leaf_address = Node::as_ptr(&*leaf);
    let root = node(Some(leaf), [7, 11, 13]);
    let address = Node::as_ptr(&*root);
    let len = Node::metadata(&*root);
    let header = Node::into_non_null(root);
    assert_eq!(header.as_ptr().cast_const(), address);
    assert_eq!(len, 3);
    let root = unsafe { Node::from_non_null(header, len) };
    assert_eq!(&root.payload, &[7, 11, 13]);
    let leaf = root.next.as_deref().unwrap();
    assert_eq!(Node::as_ptr(leaf), leaf_address);
    assert_eq!(Node::metadata(leaf), 0);
    assert!(leaf.next.is_none());
}

#[test]
fn wide_indirections_to_compatible_recursive_elements_round_trip() {
    let items = vec![List::pair(3u32, 8), List::pair(5, 13)].into_boxed_slice();
    let ptr = std::ptr::from_ref(&*items);
    let decoded = unsafe { co3::decode::<*const [List<u32>]>(co3::encode(ptr)) }.unwrap();
    assert!(std::ptr::eq(decoded, ptr));

    let decoded = unsafe { co3::decode::<Box<[List<u32>]>>(co3::encode(items)) }.unwrap();
    assert!(std::ptr::eq(std::ptr::from_ref(&*decoded), ptr));
    assert_eq!(&*decoded, &[List::pair(3, 8), List::pair(5, 13)]);
}
