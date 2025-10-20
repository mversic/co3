I have base types:

1. Robust
2. Opaque
3. Transparent
4. Cloned
5. Extern

I have derivative types:

* &Transparent             => Transparent(Target = &inner)            |  DELEGATED |
* &Robust                  => Transparent(Target = *const R)          |  DELEGATED |
* &Opaque                  => Transparent(Target = *const R)          |  DELEGATED |
* &Extern                  =>                                         |   Cloned   |  ExternRef
* &S where C: Cloned                                                  |   Cloned   |  LocalRef

* &mut Transparent         => Transparent(Target = *mut R)            |  DELEGATED |
* &mut Robust              => Transparent(Target = *mut R)            |  DELEGATED |
* &mut Opaque              => Transparent(Target = *mut R)            |  DELEGATED |
* &mut Extern              =>                                         |            |  ExternRefMut
* DOESN'T EXIST

* &[Transparent]                                                      |   Cloned
* &[Robust]                                                           |   Cloned
* &[Opaque]                                                           |   Cloned
* &[Extern]                => &[Transparent]                          |   Cloned
* &[S] where S: Cloned                                                |   Cloned   |  LocalSlice

* &mut [Transparent]
* &mut [Robust]
* DOESN'T EXIST
* DOESN'T EXIST
* DOESN'T EXIST

* Box<Transparent>                                                    |  DELEGATED |
* Box<Robust>                                                         |     Not    |
* Box<Opaque>              => Transparent(Target = *mut R)            |     Not    |
* Box<Extern>                                                         |   Cloned   |
* Box<S> where S: Cloned                                              |   Cloned   |

* Box<[Transparent]>
* Box<[Robust]>
* Box<[Opaque]>
* Box<[Extern]>            => Box<[Transparent]>                      |
* Box<[S]> where S: Cloned

* Vec<Transparent>
* Vec<Robust>
* Vec<Opaque>
* Vec<S> where S: Cloned
* Vec<Extern>              => Vec<Transparent>                        |

* [Transparent; N]         => Transparent (Target = [inner; N])       |
* [Robust; N]              => Robust                                  |
* [Opaque; N]                                                         |   Cloned   |
* [Extern; N]                                                         |   Cloned   |
* [S; N] where S: Cloned                                              |   Cloned   |


// TODO:
* Option<WithoutNiche>
* Option<S> where S: Niche
