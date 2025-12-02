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
* &Transparent             => Transparent(Target = &R::Target)        |  DELEGATED |
* &Robust                  => Transparent(Target = *const R)          |  DELEGATED |
* &Opaque                  => Transparent(Target = *const R)          |  DELEGATED |
* &S where S: Cloned                                                  |   Cloned   |

* &mut Transparent         => Transparent(Target = &mut R::Target)    |  DELEGATED |
* &mut Robust              => Transparent(Target = *mut R)            |  DELEGATED |
* &mut Opaque              => Transparent(Target = *mut R)            |  DELEGATED |
* DOESN'T EXIST

* &[Transparent]                                                      |   Cloned
* &[Robust]                                                           |   Cloned
* &[Opaque]                                                           |   Cloned
* &[S] where S: Cloned                                                |   Cloned   |

* &mut [Transparent]
* &mut [Robust]
* DOESN'T EXIST
* DOESN'T EXIST
* DOESN'T EXIST

* Box<Transparent>         => Transparent(Target = Box<R::Target>)    |  DELEGATED |
* Box<Robust>                                                         |     Not    |
* Box<Opaque>              => Transparent(Target = *mut R)            |  DELEGATED |
* Box<S> where S: Cloned                                              |   Cloned   |

* Box<[Transparent]>
* Box<[Robust]>
* Box<[Opaque]>
* Box<[S]> where S: Cloned

* Vec<Transparent>
* Vec<Robust>
* Vec<Opaque>
* Vec<S> where S: Cloned

* [Transparent; N]         => Transparent(Target = [R::Target; N])    |
* [Robust; N]              => Robust                                  |
* [Opaque; N]                                                         |   Cloned   |
* [S; N] where S: Cloned                                              |   Cloned   |

# Niche IR marker types:
1. Transparent
- types that are transparent and have a stable niche value
2. Robust
- types that don't have a niche value
3. Opaque
- opaque types
4. Cloned
- types that have a niche value, but not a stable one

# Option<T>

* Option<Transparent, Transparent>       =>                           |    Not     |
* Option<Transparent, Robust>            => Option<Robust>            |   Cloned   |
* Option<Transparent, S> where S: Cloned => Option<S>                 |   Cloned   |
* Option<Robust>                         =>                           |   Cloned   |
* Option<Opaque>                         =>                           |    Not     |
* Option<S, Robust> where S: Cloned      => Option<Robust>            |   Cloned   |
* Option<S, S> where S: Cloned           =>                           |   Cloned   |

* &Option<Transparent, Transparent>      => Option<Transparent>       |  DELEGATED |
* &mut Option<Transparent, Transparent>  => Option<Transparent>       |  DELEGATED |
* Box<Option<Transparent, Transparent>>  => Option<Transparent>       |  DELEGATED |
* [Option<Transparent, Transparent>; N]  => Option<Transparent>       |  DELEGATED |

* &Option<Opaque>                        => Option<Transparent>       |  DELEGATED |
* &mut Option<Opaque>                    => Option<Transparent>       |  DELEGATED |
* Box<Option<Opaque>>                    => Option<Transparent>       |  DELEGATED |
* [Option<Opaque>; N]                    => Option<Transparent>       |  DELEGATED |

# Derivative niche marker types:
// TODO

// TODO: There is special types like
ExternRef
ExternRefMut
