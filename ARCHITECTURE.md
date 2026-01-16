# IR marker types:

1. Transparent (depends on `Transmute` trait)
- type that recursively delegates to the `Transmute::Target` type through transmutation
- `Transmute::Target` takes the ownership and must know how to handle conversion further

2. Box<Robust> (controlled through `owned_as_ref` feature flag)
- `Robust` types that carry ownership, i.e. heap-allocated types such as `Box<T>` and `Vec<T>`
- if enabled, `owned_as_ref` feature converts owned values into borrowed before handing them out

3. Robust (depends on `ReprC` trait)
- types that have a stable layout with no trap representations
- these types don't require conversion into C-compatible types

4. Opaque (always carries ownership)
- types that are not expected to be read on the other side of the FFI boundary
- opaque types are always heap-allocated and handed out as pointers with ownership

5. Cloned (marker trait, not a marker type)
- types that are not transmutable, i.e. types that execute some form of conversion logic
- types implementing `Cloned` are always first cloned, ergo the name of the trait

# Derivative marker types:
* &Transmuted              => Transmuted(Target = &R::Target)        |  DELEGATED |
* &Robust                  => Transmuted(Target = *const R)          |  DELEGATED |
* &Opaque                  => Transmuted(Target = *const R)          |  DELEGATED |
* &S where S: Cloned                                                  |   Cloned   |

* &mut Transmuted          => Transmuted(Target = &mut R::Target)    |  DELEGATED |
* &mut Robust              => Transmuted(Target = *mut R)            |  DELEGATED |
* &mut Opaque              => Transmuted(Target = *mut R)            |  DELEGATED |
* DOESN'T EXIST

* &[Transmuted]                                                      |   Cloned
* &[Robust]                                                           |   Cloned
* &[Opaque]                                                           |   Cloned
* &[S] where S: Cloned                                                |   Cloned   |

* &mut [Transmuted]
* &mut [Robust]            => &mut [Transmuted]
* DOESN'T EXIST
* DOESN'T EXIST
* DOESN'T EXIST

* Box<Transmuted>          => Transmuted(Target = Box<R::Target>)    |  DELEGATED |
* Box<Robust>                                                         |     Not    |
* Box<Opaque>              => Transmuted(Target = *mut R)            |  DELEGATED |
* Box<S> where S: Cloned                                              |   Cloned   |

* Box<[Transmuted]>
* Box<[Robust]>
* Box<[Opaque]>
* Box<[S]> where S: Cloned

* Vec<Transmuted>
* Vec<Robust>
* Vec<Opaque>
* Vec<S> where S: Cloned

* [Transmuted; N]          => Transmuted(Target = [R::Target; N])    |
* [Robust; N]              => Robust                                  |
* [Opaque; N]                                                         |   Cloned   |
* [S; N] where S: Cloned                                              |   Cloned   |

# Niche IR marker types:
1. Transmuted
- types that are Transmuted and have a stable niche value
2. Robust
- types that don't have a niche value
3. Opaque
- opaque types
4. Cloned
- types that have a niche value, but not a stable one

# Niche::IR marker types

1. WithStableNiche (depends on `Transmute` trait)
- has a single stable (compiler guaranteed) niche value

2. WithCustomNiche
- has a custom (defined by this crate) niche value

3. Robust (depends on `ReprC` trait)
- has no trap representations and consequently no niche value

* Option<Transmuted, Transmuted>         =>                           |    Not     |
* Option<Transmuted, Robust>             => Option<Robust>            |   Cloned   |
* Option<Transmuted, S> where S: Cloned  => Option<S>                 |   Cloned   |
* Option<Robust>                         =>                           |   Cloned   |
* Option<Opaque>                         => Option<Cloned>            |    Not     |
* Option<S, Robust> where S: Cloned      => Option<Robust>            |   Cloned   |
* Option<S, S> where S: Cloned           =>                           |   Cloned   |

* &Option<Transmuted, Transmuted>        => Option<Transmuted>       |  DELEGATED |
* &mut Option<Transmuted, Transmuted>    => Option<Transmuted>       |  DELEGATED |
* Box<Option<Transmuted, Transmuted>>    => Option<Transmuted>       |  DELEGATED |
* [Option<Transmuted, Transmuted>; N]    => Option<Transmuted>       |  DELEGATED |

# Derivative niche marker types:
// TODO

// TODO: There is special types like
ExternRef
ExternRefMut
