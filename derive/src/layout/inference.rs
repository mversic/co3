//! Syntactic facts used by recursive lowering and property inference.
//!
//! Built-in spelling is checked independently by type_checks. A failed
//! inference leaves the associated-type projection for the compiler to resolve.
use quote::quote;
use syn::{parse_quote, visit_mut::VisitMut};

use super::{
    attr::ReprKind,
    ctype::{TailClass, UnsizedTail, classify_tail, gen_ctype_name},
};
use crate::{
    known_type::{Kind, recognize},
    utils::peel_grouped_type,
};

fn is_self(ty: &syn::Type, own: &syn::Type) -> bool {
    let ty = peel_grouped_type(ty);
    ty == own || matches!(ty, syn::Type::Path(p) if p.path.is_ident("Self"))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Wrapper {
    Pointer,
    Reference,
    Slice,
    Named(Kind),
}

struct Link<'a> {
    wrapper: Wrapper,
    ty: &'a syn::Type,
    inner: &'a syn::Type,
}

/// A nonempty sequence of verified syntactic forms leading back to this exact
/// instantiation. Keep the original types for identity checks and bounds.
struct RecursivePath<'a> {
    links: Vec<Link<'a>>,
}

impl<'a> RecursivePath<'a> {
    fn recognize(mut ty: &'a syn::Type, own: &syn::Type) -> Option<Self> {
        let mut links = Vec::new();
        let mut indirect = false;
        while !is_self(ty, own) {
            ty = peel_grouped_type(ty);
            let (wrapper, inner) = match ty {
                syn::Type::Ptr(p) => (Wrapper::Pointer, &*p.elem),
                syn::Type::Reference(r) => (Wrapper::Reference, &*r.elem),
                syn::Type::Slice(s) if indirect => (Wrapper::Slice, &*s.elem),
                _ => {
                    let known = recognize(ty)?;
                    match known.kind {
                        Kind::Box | Kind::Vec | Kind::NonNull | Kind::Option => {
                            (Wrapper::Named(known.kind), known.inner?)
                        }
                        _ => return None,
                    }
                }
            };
            indirect |= !matches!(wrapper, Wrapper::Named(Kind::Option) | Wrapper::Slice);
            links.push(Link { wrapper, ty, inner });
            ty = inner;
        }
        indirect.then_some(Self { links })
    }

    fn owned(&self) -> bool {
        !self.links.iter().any(|link| {
            matches!(
                link.wrapper,
                Wrapper::Pointer | Wrapper::Reference | Wrapper::Named(Kind::NonNull)
            )
        })
    }

    fn allocation(&self) -> Option<(Kind, &'a syn::Type)> {
        let link = match self.links.as_slice() {
            [allocation] => allocation,
            [option, allocation] if option.wrapper == Wrapper::Named(Kind::Option) => allocation,
            _ => return None,
        };
        match link.wrapper {
            Wrapper::Named(kind @ (Kind::Box | Kind::Vec)) => Some((kind, link.ty)),
            _ => None,
        }
    }

    /// Source properties used to select each wrapper's carrier, from outside in.
    fn source_bounds(&self, own: &syn::Type, tail: TailClass) -> Vec<syn::WherePredicate> {
        self.links.iter().filter_map(|link| {
            let bound = match link.wrapper {
                Wrapper::Named(Kind::Box) => match pointee_class(link.inner, own, tail) {
                    TailClass::Sized => quote!(Size = co3::rust_spec::size::Sized<co3::rust_spec::Gt<co3::rust_spec::Zero>>),
                    TailClass::Unsized(UnsizedTail::SliceLike) => quote!(Size = co3::rust_spec::size::MetaSized<co3::rust_spec::size::SliceLike>),
                    _ => return None,
                },
                Wrapper::Named(Kind::Option) => {
                    let niche = recursive_niche(link.inner, own, tail)?;
                    quote!(Niche = #niche)
                }
                _ => return None,
            };
            let mut inner = link.inner.clone();
            crate::utils::ConcreteSelfType(own).visit_type_mut(&mut inner);
            Some(parse_quote!(#inner: co3::rust_spec::RustSpec<#bound>))
        }).collect()
    }

    /// Residual ABI premises for recursive wrapper checks. None delegates the
    /// entire field to the compiler. An empty list still checks the real field.
    fn compatibility(
        &self,
        generic: bool,
        own: &syn::Type,
        companion: &syn::Type,
        tail: TailClass,
    ) -> Option<Vec<syn::WherePredicate>> {
        if !generic
            && tail == TailClass::Sized
            && self
                .links
                .iter()
                .all(|link| !matches!(link.wrapper, Wrapper::Slice | Wrapper::Named(Kind::Vec)))
        {
            return Some(Vec::new());
        }
        let mut bounds = Vec::new();
        for (i, link) in self.links.iter().enumerate().rev() {
            let ty = link.ty;
            let stable = quote!(co3::rust_spec::RustSpec<Layout = co3::rust_spec::Specified>);
            match link.wrapper {
                Wrapper::Pointer | Wrapper::Named(Kind::NonNull) => continue,
                Wrapper::Slice => {
                    if generic {
                        bounds.push(parse_quote!(#ty: #stable));
                    }
                    continue;
                }
                Wrapper::Named(Kind::Box)
                    if pointee_class(link.inner, own, tail) == TailClass::Sized => {}
                Wrapper::Named(Kind::Option)
                    if self.links.get(i + 1).is_some_and(|next| {
                        matches!(next.wrapper, Wrapper::Named(Kind::Box | Kind::NonNull))
                    }) => {}
                _ => return None,
            }
            let carrier = lower_path(&self.links[i..], own, companion, tail)?;
            if generic {
                bounds.push(parse_quote!(#ty: #stable + co3::ReprC<CType = #carrier>));
            }
        }
        Some(bounds)
    }
}

/// The record takes its width from its tail. A wrapper takes its own width,
/// including a slice nested between a pointer and the recursive record.
fn pointee_class(ty: &syn::Type, own: &syn::Type, tail: TailClass) -> TailClass {
    if is_self(ty, own) {
        tail
    } else {
        super::ctype::classify_tail(ty, &syn::Generics::default())
    }
}

/// Decisions shared by the companion, its fixed header and the generated impls.
/// Store the source fields and record carriers in declaration order.
#[derive(Default)]
pub(super) struct RecursiveFields<'a> {
    pub wide_raw_pointer: Option<&'a syn::Type>,
    pub carriers: Vec<Option<syn::Type>>,
    pub header_prefix: Option<&'a [Option<syn::Type>]>,
    pub equalities: Vec<syn::WherePredicate>,
    pub source_bounds: Vec<syn::WherePredicate>,
    pub tail_bounds: Vec<syn::WherePredicate>,
    pub wide_header: bool,
    pub selectors: CarrierSelectors,
    pub fields: Vec<SourceField<'a>>,
    pub stores: RecursiveStores<'a>,
}

#[derive(Default)]
pub(super) struct CarrierSelectors {
    pub reference: bool,
    pub vec: bool,
    pub option: bool,
}

pub(super) struct SourceField<'a> {
    pub ty: &'a syn::Type,
    pub raw_self_pointer: bool,
    pub compatibility: Option<RecursiveCompatibility>,
}

pub(super) enum RecursiveCompatibility {
    Bounds(Vec<syn::WherePredicate>),
    Vector,
}

#[derive(Default)]
pub(super) struct RecursiveStores<'a> {
    pub has_owned: bool,
    pub marker_fields: Vec<&'a syn::Type>,
    pub codec_fields: Vec<&'a syn::Type>,
    pub allocations: Vec<(Kind, &'a syn::Type)>,
}

impl<'a> RecursiveStores<'a> {
    fn add(
        &mut self,
        ty: &'a syn::Type,
        path: Option<&RecursivePath<'a>>,
        generic: bool,
        sized_record: bool,
    ) {
        if path.is_some_and(RecursivePath::owned) {
            self.has_owned = true;
            if !generic {
                return;
            }
        }
        self.marker_fields.push(ty);
        if sized_record && let Some(allocation) = path.and_then(RecursivePath::allocation) {
            self.allocations.push(allocation);
        } else {
            self.codec_fields.push(ty);
        }
    }
}

impl<'a> RecursiveFields<'a> {
    pub fn analyze(
        input: &'a syn::DeriveInput,
        repr: Option<&ReprKind>,
        header_prefix: Option<&'a [Option<syn::Type>]>,
    ) -> Self {
        let (fields, record): (Vec<_>, _) = match &input.data {
            syn::Data::Struct(s) => (s.fields.iter().collect(), true),
            syn::Data::Enum(e) => (
                e.variants.iter().flat_map(|v| &v.fields).collect(),
                super::is_transparent_enum_repr(repr, &e.variants),
            ),
            syn::Data::Union(_) => unreachable!(),
        };
        let generics = &input.generics;
        let name = &input.ident;
        let (_, args, _) = generics.split_for_impl();
        let own = parse_quote!(#name #args);
        let companion = gen_ctype_name(name);
        let companion = parse_quote!(#companion #args);
        let last = fields.last().map(|f| &f.ty);
        let tail = last.map_or(TailClass::Sized, |ty| classify_tail(ty, generics));
        let mut result = Self {
            header_prefix,
            ..Self::default()
        };
        if let Some(prefix) = header_prefix {
            // Name Self in the context of the header's source fields. Its companion
            // reuses the enclosing record's verified carriers for these fields.
            result
                .source_bounds
                .extend(fields.iter().zip(prefix).filter_map(|(field, carrier)| {
                    let ty = &field.ty;
                    let carrier = carrier.as_ref()?;
                    super::is_parametrized(ty, generics)
                        .then(|| parse_quote!(#ty: co3::ReprC<CType = #carrier>))
                }));
        }
        let mut slice_element = false;
        if record {
            result.carriers.resize(fields.len(), None);
        }
        for (i, field) in fields.into_iter().enumerate() {
            let ty = &field.ty;
            let path = RecursivePath::recognize(ty, &own);
            if path.as_ref().is_some_and(|path| {
                path.links.iter().any(|link| {
                    matches!(
                        link.wrapper,
                        Wrapper::Pointer | Wrapper::Named(Kind::NonNull)
                    ) && matches!(pointee_class(link.inner, &own, tail), TailClass::Unsized(_))
                })
            }) {
                result.wide_raw_pointer.get_or_insert(ty);
            }
            let generic = path.is_some() && super::is_parametrized(ty, generics);
            result.stores.add(
                ty,
                path.as_ref(),
                generic,
                record && tail == TailClass::Sized,
            );
            result.fields.push(SourceField {
                ty,
                // CType for a raw pointer or slice only requires its pointee or element.
                raw_self_pointer: matches!(ty, syn::Type::Ptr(_))
                    && path.as_ref().is_some_and(|p| {
                        p.links
                            .iter()
                            .all(|l| matches!(l.wrapper, Wrapper::Pointer | Wrapper::Slice))
                    }),
                compatibility: path.as_ref().and_then(|p| match p.links.as_slice() {
                    [link] if record && link.wrapper == Wrapper::Named(Kind::Vec) => {
                        Some(RecursiveCompatibility::Vector)
                    }
                    _ => p
                        .compatibility(generic, &own, &companion, tail)
                        .map(RecursiveCompatibility::Bounds),
                }),
            });
            let Some(path) = path.filter(|_| record) else {
                continue;
            };
            let reference = recursive_reference_carrier(ty, &own, &companion, tail, generics);
            result.selectors.reference |= reference.is_some();
            let carrier = reference.or_else(|| lower_path(&path.links, &own, &companion, tail));
            if let Some(carrier) = carrier {
                for link in &path.links {
                    match link.wrapper {
                        Wrapper::Named(Kind::Vec) => result.selectors.vec = true,
                        Wrapper::Named(Kind::Option) => result.selectors.option = true,
                        _ => {}
                    }
                }
                let innermost = path
                    .links
                    .last()
                    .expect("a recursive path contains an indirection")
                    .wrapper;
                slice_element |= innermost == Wrapper::Slice;
                result.wide_header |= innermost == Wrapper::Named(Kind::Box)
                    && !generics.params.is_empty()
                    && tail == TailClass::Unsized(UnsizedTail::SliceLike);
                if generic {
                    result.source_bounds.extend(path.source_bounds(&own, tail));
                    result
                        .equalities
                        .push(parse_quote!(#ty: co3::ReprC<CType = #carrier>));
                }
                result.carriers[i] = Some(carrier);
            }
        }
        if let Some(last) = last.filter(|ty| super::is_parametrized(ty, generics)) {
            result.add_tail_bounds(last, generics, slice_element);
        }
        result
    }

    fn add_tail_bounds(&mut self, tail: &syn::Type, generics: &syn::Generics, slice_element: bool) {
        if slice_element && self.carriers.last().is_some_and(Option::is_none) {
            self.tail_bounds
                .push(parse_quote!(#tail: co3::ReprC<CType: Sized>));
        }
        if self.wide_header {
            if let syn::Type::Slice(s) = tail {
                let elem = &s.elem;
                self.tail_bounds
                    .push(parse_quote!(#elem: co3::ReprC<CType: Sized>));
            }
            let header = super::wide::header_repr_c_predicate(tail, generics);
            let lowered = parse_quote!(<#tail as co3::ReprC>::CType);
            let metadata = super::wide::wide_usize_predicate(&lowered, generics);
            self.tail_bounds
                .extend([header, parse_quote!(#tail: co3::ReprC), metadata]);
        }
    }

    pub fn types(&self) -> Vec<&'a syn::Type> {
        self.fields.iter().map(|f| f.ty).collect()
    }
}

/// Resolve only the pointer width syntactically. RustSpec selects layout and trap.
pub(super) fn recursive_reference_carrier(
    ty: &syn::Type,
    own: &syn::Type,
    companion: &syn::Type,
    tail: TailClass,
    generics: &syn::Generics,
) -> Option<syn::Type> {
    let syn::Type::Reference(reference) = ty else {
        return None;
    };
    if !is_self(&reference.elem, own) {
        return None;
    }
    // A generic layout projection can remain abstract in the struct definition.
    if generics.type_params().next().is_some() || generics.const_params().next().is_some() {
        return None;
    }
    let lifetime = reference.lifetime.as_ref()?;
    let mutable = reference.mutability.is_some();
    let (direct, soft) = match (tail, mutable) {
        (TailClass::Sized, false) => (
            quote!(co3::reference::abi::Ref),
            quote!(co3::reference::abi::SoftRef),
        ),
        (TailClass::Sized, true) => (
            quote!(co3::reference::abi::RefMut),
            quote!(co3::reference::abi::SoftRefMut),
        ),
        (TailClass::Unsized(UnsizedTail::SliceLike), false) => (
            quote!(co3::wide::abi::WideRef),
            quote!(co3::wide::abi::SoftWideRef),
        ),
        (TailClass::Unsized(UnsizedTail::SliceLike), true) => (
            quote!(co3::wide::abi::WideRefMut),
            quote!(co3::wide::abi::SoftWideRefMut),
        ),
        _ => return None,
    };
    let layout = quote!(<#own as co3::rust_spec::RustSpec>::Layout);
    let selector = if mutable {
        quote!((#layout, <#own as co3::rust_spec::RustSpec>::Trap))
    } else {
        layout
    };
    Some(
        parse_quote!(<#selector as __Co3SelectRef<#direct<#lifetime, #companion>, #soft<#companion>>>::Carrier),
    )
}

fn constructor_probe(ty: &syn::TypePath) -> Option<syn::TypePath> {
    let mut probe = ty.clone();
    probe.path.segments.last_mut()?.arguments =
        syn::PathArguments::AngleBracketed(parse_quote!(<()>));
    Some(probe)
}

/// Lower from the companion outwards so each wrapper receives its pointee's carrier.
fn lower_path(
    links: &[Link],
    own: &syn::Type,
    companion: &syn::Type,
    tail: TailClass,
) -> Option<syn::Type> {
    let mut carrier = companion.clone();
    for link in links.iter().rev() {
        let inner = link.inner;
        carrier = match (link.wrapper, link.ty) {
            (Wrapper::Slice, _) if pointee_class(inner, own, tail) == TailClass::Sized => {
                parse_quote!([#carrier])
            }
            (Wrapper::Reference, syn::Type::Reference(r))
                if r.mutability.is_none()
                    && is_self(inner, own)
                    && tail == TailClass::Unsized(UnsizedTail::NulTerminated) =>
            {
                parse_quote!(co3::reference::abi::SoftRef<#carrier>)
            }
            (Wrapper::Pointer, syn::Type::Ptr(ptr))
                if pointee_class(inner, own, tail) == TailClass::Sized =>
            {
                let mut ptr = ptr.clone();
                ptr.elem = Box::new(carrier);
                syn::Type::Ptr(ptr)
            }
            (Wrapper::Named(Kind::Box), _) => match pointee_class(inner, own, tail) {
                TailClass::Sized | TailClass::Unsized(UnsizedTail::NulTerminated) => {
                    parse_quote!(co3::boxed::abi::Box<#carrier>)
                }
                TailClass::Unsized(UnsizedTail::SliceLike) => {
                    parse_quote!(co3::wide::abi::WideBox<#carrier>)
                }
                _ => return None,
            },
            (Wrapper::Named(Kind::NonNull), _)
                if pointee_class(inner, own, tail) == TailClass::Sized =>
            {
                parse_quote!(co3::marker::NonZero<*mut #carrier>)
            }
            (Wrapper::Named(Kind::Vec), syn::Type::Path(path)) => {
                let probe = constructor_probe(path)?;
                parse_quote!(<#probe as __Co3SelectVec>::Carrier<#carrier>)
            }
            (Wrapper::Named(Kind::Option), syn::Type::Path(path)) => {
                let probe = constructor_probe(path)?;
                let mut inner = inner.clone();
                crate::utils::ConcreteSelfType(own).visit_type_mut(&mut inner);
                let niche = recursive_niche(&inner, own, tail)
                    .unwrap_or_else(|| quote!(<#inner as co3::rust_spec::RustSpec>::Niche));
                let tagged = quote!(co3::option::abi::Option<#carrier>);
                let some = quote!(co3::option::abi::OptionSome<#carrier>);
                parse_quote!(<#probe as __Co3SelectOption<#niche>>::Carrier<#tagged, __Co3Native<#tagged>, __Co3Native<#some>>)
            }
            _ => return None,
        };
    }
    Some(carrier)
}

#[cfg(test)]
pub(super) fn recursive_carrier(
    ty: &syn::Type,
    own: &syn::Type,
    companion: &syn::Type,
    tail: TailClass,
) -> Option<syn::Type> {
    lower_path(
        &RecursivePath::recognize(ty, own)?.links,
        own,
        companion,
        tail,
    )
}

/// Each supported pointer has a pointee-independent niche once its width is
/// known. Both vector families have a niche, with different layout guarantees.
fn recursive_niche(
    ty: &syn::Type,
    own: &syn::Type,
    tail: TailClass,
) -> Option<proc_macro2::TokenStream> {
    let ty = peel_grouped_type(ty);
    let pointee = match ty {
        syn::Type::Ptr(_) => return Some(quote!(co3::rust_spec::niche::WithoutNiche)),
        syn::Type::Reference(r) => &*r.elem,
        _ => {
            let known = recognize(ty)?;
            if known.kind == Kind::Vec {
                let syn::Type::Path(path) = ty else {
                    return None;
                };
                let probe = constructor_probe(path)?;
                return Some(quote!(<#probe as __Co3SelectVec>::Niche));
            }
            if !matches!(known.kind, Kind::Box | Kind::NonNull) {
                return None;
            }
            known.inner?
        }
    };
    let layout = match pointee_class(pointee, own, tail) {
        TailClass::Sized => quote!(co3::rust_spec::Specified),
        TailClass::Unsized(UnsizedTail::SliceLike | UnsizedTail::NulTerminated) => {
            quote!(__Co3Unstable)
        }
        _ => return None,
    };
    Some(quote!(co3::rust_spec::niche::WithNiche<#layout>))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recursive_slice_chooses_its_own_pointer_width() {
        let own = parse_quote!(Tree<T>);
        let companion = parse_quote!(TreeCType<T>);
        let source = parse_quote!(Box<[Self]>);
        let expected = parse_quote!(co3::wide::abi::WideBox<[TreeCType<T>]>);
        assert_eq!(
            recursive_carrier(&source, &own, &companion, TailClass::Sized),
            Some(expected)
        );
        assert_eq!(
            recursive_carrier(
                &parse_quote!(Box<Self>),
                &own,
                &companion,
                TailClass::Unknown
            ),
            None,
        );
    }

    #[test]
    fn recursive_wide_raw_pointers_are_rejected() {
        let fields: [syn::Type; 4] = [
            parse_quote!(*const [Self]),
            parse_quote!(*mut [Self]),
            parse_quote!(Option<core::ptr::NonNull<[Self]>>),
            parse_quote!(Box<*const [Self]>),
        ];
        for ty in fields {
            let input = parse_quote!(struct Node<T> { link: #ty, value: T });
            assert!(
                RecursiveFields::analyze(&input, None, None)
                    .wide_raw_pointer
                    .is_some()
            );
        }
        let input = parse_quote!(
            struct Node<T> {
                link: *mut Self,
                tail: [T],
            }
        );
        assert!(
            RecursiveFields::analyze(&input, None, None)
                .wide_raw_pointer
                .is_some()
        );
    }

    #[test]
    fn inference_requires_a_known_path_to_the_exact_instantiation() {
        let fields: Vec<syn::Type> = vec![
            parse_quote!(Box<Node<T>>),
            parse_quote!(Vec<Node<T>>),
            parse_quote!(*const Node<T>),
            parse_quote!(core::ptr::NonNull<Node<T>>),
            parse_quote!(Option<Box<Self>>),
            parse_quote!(Box<Node<u16>>),
            parse_quote!(Alias<Self>),
            parse_quote!(core::marker::PhantomData<Node<T>>),
            parse_quote!(T),
        ];
        let input = parse_quote!(struct Node<T>(#(#fields),*););
        let inferred = RecursiveFields::analyze(&input, None, None).carriers;
        assert_eq!(
            inferred.iter().map(Option::is_some).collect::<Vec<_>>(),
            [true, true, true, true, true, false, false, false, false]
        );
    }

    #[test]
    fn nul_terminated_pointer_niche_uses_the_pointee_size_class() {
        let niche = recursive_niche(
            &parse_quote!(Box<Node>),
            &parse_quote!(Node),
            TailClass::Unsized(UnsizedTail::NulTerminated),
        )
        .unwrap();
        assert_eq!(
            niche.to_string(),
            quote!(co3::rust_spec::niche::WithNiche<__Co3Unstable>).to_string()
        );
    }
}
