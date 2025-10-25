I have base types:

1. Robust
2. Opaque
3. Transparent
4. Cloned
5. Extern

6. Option<WithoutNiche>
7. Option<Transparent>

8. Box<Robust>

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
