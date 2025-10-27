Ir type markers:

1. Transparent (depends on `Transmute` trait)
- type that recursively delegates to the `Transmute::Target` type through transmutation
- `Transmute::Target` takes the ownership and must know how to handle conversion further

2. Box<Robust> (depends on `owned_as_ref` feature)
- `Robust` types that carry ownership, i.e. heap-allocated types such as `Box<T>` and `Vec<T>`
- the `owned_as_ref` feature converts owned values into borrowed ones before handing them out

3. Robust (depends on `ReprC` trait)
- types that have a stable layout with no trap representations
- these types don't require conversion into C-compatible types

4. Opaque (always carries ownership)
- types that are not expected to be read on the other side of the FFI boundary
- opaque types are always heap-allocated and handed out as pointers with ownership

5. Cloned (a trait, not a marker type)
- types that are not transmutable, i.e. types that execute some form of conversion logic
- references types implementing `Cloned` are always first cloned, ergo the name of the trait

6. Option<WithoutNiche>
7. Option<Transparent>
8. Option<S> where S: Cloned


I have derivative types:

* &Transparent             => Transparent(Target = &R::Target)        |  DELEGATED |
* &Robust                  => Transparent(Target = *const R)          |  DELEGATED |
* &Opaque                  => Transparent(Target = *const R)          |  DELEGATED |
* &Extern                  =>                                         |   Cloned   |  ExternRef
* &S where S: Cloned                                                  |   Cloned   |

* &mut Transparent         => Transparent(Target = &mut R::Target)    |  DELEGATED |
* &mut Robust              => Transparent(Target = *mut R)            |  DELEGATED |
* &mut Opaque              => Transparent(Target = *mut R)            |  DELEGATED |
* &mut Extern              =>                                         |            |  ExternRefMut
* DOESN'T EXIST

* &[Transparent]                                                      |   Cloned
* &[Robust]                                                           |   Cloned
* &[Opaque]                                                           |   Cloned
* &[Extern]                => &[Transparent]                          |   Cloned
* &[S] where S: Cloned                                                |   Cloned   |

* &mut [Transparent]
* &mut [Robust]
* DOESN'T EXIST
* DOESN'T EXIST
* DOESN'T EXIST

* Box<Transparent>         => Transparent(Target = Box<R::Target>)    |  DELEGATED |
* Box<Robust>                                                         |     Not    |
* Box<Opaque>              => Transparent(Target = *mut R)            |  DELEGATED |
* Box<Extern>                                                         |   Cloned   |
* Box<S> where S: Cloned                                              |   Cloned   |

* Box<[Transparent]>
* Box<[Robust]>
* Box<[Opaque]>
* Box<[Extern]>            => Box<[Transparent(*mut Extern)]>         |
* Box<[S]> where S: Cloned

* Vec<Transparent>
* Vec<Robust>
* Vec<Opaque>
* Vec<S> where S: Cloned
* Vec<Extern>              => Vec<Transparent>                        |

* [Transparent; N]         => Transparent(Target = [R::Target; N])    |
* [Robust; N]              => Robust                                  |
* [Opaque; N]                                                         |   Cloned   |
* [Extern; N]                                                         |   Cloned   |
* [S; N] where S: Cloned                                              |   Cloned   |


# Option<T>

* Option<Transparent, Transparent>       =>                           |    Not     |
* Option<Transparent, Robust>            => Option<Robust>            |   Cloned   |
* Option<Robust>                         =>                           |   Cloned   |
* Option<Opaque>                         =>                           |    Not     |
* Option<Extern>                         =>                           |    Not     |
* Option<S, Robust> where S: Cloned      => Option<Robust>            |   Cloned   |
* Option<S, S> where S: Cloned =>                                     |   Cloned   |

* &Option<Transparent, Transparent>      => Option<Transparent>       |  DELEGATED |
* &mut Option<Transparent, Transparent>  => Option<Transparent>       |  DELEGATED |
* Box<Option<Transparent, Transparent>>  => Option<Transparent>       |  DELEGATED |
* [Option<Transparent, Transparent>; N]  => Option<Transparent>       |  DELEGATED |

* &Option<Opaque>                        => Option<Transparent>       |  DELEGATED |
* &mut Option<Opaque>                    => Option<Transparent>       |  DELEGATED |
* Box<Option<Opaque>>                    => Option<Transparent>       |  DELEGATED |
* [Option<Opaque>; N]                    => Option<Transparent>       |  DELEGATED |

* &Option<Extern>                        => Option<Transparent>       |  DELEGATED |
* &mut Option<Extern>                    => Option<Transparent>       |  DELEGATED |
* Box<Option<Extern>>                    => Option<Transparent>       |  DELEGATED |
* [Option<Extern>; N]                    => Option<Transparent>       |  DELEGATED |


// TODO: There is special types like
ExternRef
ExternRefMut
