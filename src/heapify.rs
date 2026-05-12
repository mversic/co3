#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};
use disjoint_impls::disjoint_impls;

use crate::niche::{NicheFamily, WithNiche, WithoutNiche};

disjoint_impls! {
    pub trait Heapify: Sized {
        // TODO: Add this bound on Borrow::Borrowed
        type Kind; //: ExternC<CType: FnArg>;

        fn heapify(self) -> Self::Kind;
        fn unheapify(kind: Self::Kind) -> Self;
    }

    impl<R: Heapify> Heapify for Option<R>
    where
        R: NicheFamily<Kind = WithoutNiche>,
    {
        type Kind = Self;

        #[inline(always)]
        fn heapify(self) -> Self::Kind {
            self
        }

        #[inline(always)]
        fn unheapify(kind: Self::Kind) -> Self {
            kind
        }
    }

    impl<R: Heapify> Heapify for Option<R>
    where
        R: NicheFamily<Kind: WithNiche>,
    {
        type Kind = Option<<R as Heapify>::Kind>;

        #[inline(always)]
        fn heapify(self) -> Self::Kind {
            self.map(R::heapify)
        }

        #[inline(always)]
        fn unheapify(kind: Self::Kind) -> Self {
            kind.map(R::unheapify)
        }
    }
}

impl<R: ?Sized> Heapify for &R {
    type Kind = Self;

    #[inline(always)]
    fn heapify(self) -> Self::Kind {
        self
    }

    #[inline(always)]
    fn unheapify(kind: Self::Kind) -> Self {
        kind
    }
}

impl<R: ?Sized> Heapify for &mut R {
    type Kind = Self;

    #[inline(always)]
    fn heapify(self) -> Self::Kind {
        self
    }

    #[inline(always)]
    fn unheapify(kind: Self::Kind) -> Self {
        kind
    }
}

impl<R: ?Sized> Heapify for Box<R> {
    type Kind = Self;

    #[inline(always)]
    fn heapify(self) -> Self::Kind {
        self
    }

    #[inline(always)]
    fn unheapify(kind: Self::Kind) -> Self {
        kind
    }
}

impl<R: Heapify> Heapify for Vec<R> {
    // TODO: This is just a shortcircuit for what will happen anyways, maybe remove it?
    type Kind = Box<[R]>;

    #[inline(always)]
    fn heapify(self) -> Self::Kind {
        self.into_boxed_slice()
    }

    #[inline(always)]
    fn unheapify(kind: Self::Kind) -> Self {
        kind.into()
    }
}

impl<R, const N: usize> Heapify for [R; N] {
    type Kind = Box<Self>;

    #[inline(always)]
    fn heapify(self) -> Self::Kind {
        Box::new(self)
    }

    #[inline(always)]
    fn unheapify(kind: Self::Kind) -> Self {
        *kind
    }
}
