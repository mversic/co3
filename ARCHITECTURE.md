I have base types:

1. Robust
2. Opaque
3. Transparent
4. Cloned
5. Extern

I have derivative types:

* &Transparent                  => Transparent (Target = &inner)    |
* &Robust                       => Transparent (Target = *const R)  |
* &Opaque                       => Transparent (Target = *const R)  |
* &Extern                       =>                                  |            |  ExternRef
* &S where C: Cloned                                                |   Cloned   |  LocalRef

* &mut Transparent              => Transparent (Target = *mut R)    |
* &mut Robust                   => Transparent (Target = *mut R)    |
* &mut Opaque                   => Transparent (Target = *mut R)    |
* &mut Extern                   =>                                  |            |  ExternRefMut
* DOESN'T EXIST

* &[Transparent]                                                    |   Cloned
* &[Robust]                                                         |   Cloned
* &[Opaque]                                                         |   Cloned
* &[Extern]                     => TODO                             |   Cloned
* &[S] where S: Cloned                                              |   Cloned   |  LocalSlice

* &mut [Transparent]
* &mut [Robust]
* DOESN'T EXIST
* DOESN'T EXIST
* DOESN'T EXIST

* Box<Transparent>                                                  |  Sometimes |
* Box<Robust>                                                       |  Not       |
* Box<Opaque>                   => Transparent (Target = *mut R)    |  Not       |
* Box<Extern>                                                       |  Not       |
* Box<S> where S: Cloned                                            |  Cloned    |

* Box<[Transparent]>
* Box<[Robust]>
* Box<[Opaque]>
* Box<[Extern]>
* Box<[S]> where S: Cloned

* Vec<Transparent>
* Vec<Robust>
* Vec<Opaque>
* Vec<S> where S: Cloned
* Vec<Extern>                => Vec<Transparent>            // TODO: This is suspicious when compared to Opaque

* [Transparent; N]           => should Transparent
* [Robust; N]
* [Opaque; N]
* [Extern; N]
* [S; N] where S: Cloned

* Option<WithoutNiche>
* Option<S> where S: Niche
