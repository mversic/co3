//! This module implements a visitor that walks over an impl block and collects information to generate the FFI functions
//!
//! It also defines descriptors - types that are used for the codegen step

use manyhow::emit;
use proc_macro2::Span;
use quote::ToTokens;
use syn::{
    Attribute, Expr, Ident, Path, Type, Visibility, parse_quote,
    visit::{Visit, visit_signature},
    visit_mut::VisitMut,
};

use crate::{emitter::Emitter, utils::unwrap_result_type, wrapper::is_self_ty};

pub struct Arg {
    self_ty: Option<Path>,
    name: Ident,
    type_: Type,
    is_handle: bool,
}

impl Arg {
    pub fn new(self_ty: Option<Path>, name: Ident, type_: Type) -> Self {
        Self {
            self_ty,
            name,
            type_,
            is_handle: false,
        }
    }
    pub fn new_with_handle(
        self_ty: Option<Path>,
        name: Ident,
        type_: Type,
        is_handle: bool,
    ) -> Self {
        Self {
            self_ty,
            name,
            type_,
            is_handle,
        }
    }
    pub fn name(&self) -> &Ident {
        &self.name
    }
    pub fn src_type(&self) -> &Type {
        &self.type_
    }
    pub fn is_handle(&self) -> bool {
        self.is_handle
    }
    pub fn src_type_is_empty_tuple(&self) -> bool {
        matches!(self.src_type_resolved(), Type::Tuple(syn::TypeTuple { ref elems, .. }) if elems.is_empty())
    }
    pub fn src_type_resolved(&self) -> Type {
        resolve_type(self.self_ty.as_ref(), self.type_.clone())
    }
}

fn resolve_type(self_type: Option<&Path>, mut arg_type: Type) -> Type {
    TypeImplTraitResolver.visit_type_mut(&mut arg_type);

    if let Some(self_ty) = self_type {
        SelfResolver::new(self_ty).visit_type_mut(&mut arg_type);
    }
    if let Some((ok, _)) = unwrap_result_type(&arg_type) {
        arg_type = ok.clone();
    }

    arg_type
}

pub struct ForeignArgProcessor<'a> {
    self_ty: Option<&'a Path>,
}

impl VisitMut for ForeignArgProcessor<'_> {
    fn visit_type_mut(&mut self, node: &mut Type) {
        if is_self_ty(node, self.self_ty) {
            return;
        }

        match node {
            Type::Path(path_ty) => {
                let last_seg = path_ty.path.segments.last().unwrap();

                if last_seg.ident == "Box"
                    && let syn::PathArguments::AngleBracketed(bracketed) = &last_seg.arguments
                    && bracketed.args.len() == 1
                    && let syn::GenericArgument::Type(boxed) = &bracketed.args[0]
                    && is_self_ty(boxed, self.self_ty)
                {
                    *node = parse_quote!(Self);
                    return;
                }
            }
            Type::Reference(ref_ty) if is_self_ty(&ref_ty.elem, self.self_ty) => {
                *node = if ref_ty.mutability.is_some() {
                    parse_quote!(co3::external::ExternRefMut<'_, Self>)
                } else {
                    parse_quote!(co3::external::ExternRef<'_, Self>)
                };

                return;
            }
            _ => (),
        }

        syn::visit_mut::visit_type_mut(self, node);
    }
}

pub struct ImplDescriptor<'ast> {
    /// Trait name
    pub trait_name: Option<&'ast Path>,
    /// Associated types
    pub associated_types: Vec<(&'ast Ident, &'ast Type)>,
    /// Associated constants
    pub associated_consts: Vec<(&'ast Ident, &'ast Type, &'ast Expr)>,
    pub generics: &'ast syn::Generics,
    /// Functions in the impl block
    pub fns: Vec<FnDescriptor<'ast>>,
}

pub struct FnDescriptor<'ast> {
    /// Function attributes
    pub attrs: Vec<&'ast Attribute>,
    /// Resolved type of the `Self` type
    pub self_ty: Option<Path>,

    /// Function documentation
    // TODO: Could just be a part of all attrs?
    pub doc: Vec<&'ast Attribute>,
    /// Original signature of the method
    pub sig: syn::Signature,

    /// Receiver argument, i.e. `self`
    pub receiver: Option<Arg>,
    /// Input fn arguments
    pub input_args: Vec<Arg>,
    /// Output fn argument
    pub output_arg: Option<Arg>,
}

struct ImplVisitor<'ast, 'emitter> {
    emitter: &'emitter mut Emitter,
    allow_non_lifetime_impl_generics: bool,
    fatal: bool,
    trait_name: Option<&'ast Path>,
    /// Resolved type of the `Self` type
    self_ty: Option<&'ast Path>,
    generics: Option<&'ast syn::Generics>,
    associated_types: Vec<(&'ast Ident, &'ast Type)>,
    associated_consts: Vec<(&'ast Ident, &'ast Type, &'ast Expr)>,
    fns: Vec<FnDescriptor<'ast>>,
}

struct FnVisitor<'ast, 'emitter> {
    emitter: &'emitter mut Emitter,
    fatal: bool,
    attrs: Vec<&'ast Attribute>,
    doc: Vec<&'ast Attribute>,
    /// Resolved type of the `Self` type
    self_ty: Option<&'ast Path>,

    /// Original signature of the method
    sig: Option<&'ast syn::Signature>,

    /// Receiver argument, i.e. `self`
    receiver: Option<Arg>,
    /// Input fn arguments
    input_args: Vec<Arg>,
    /// Output fn argument
    output_arg: Option<Arg>,

    /// Name of the argument being visited
    curr_arg_name: Option<&'ast Ident>,
    curr_arg_is_handle: bool,
}

impl<'ast> ImplDescriptor<'ast> {
    pub fn from_impl(emitter: &mut Emitter, node: &'ast syn::ItemImpl) -> Option<Self> {
        let mut visitor = ImplVisitor::new(emitter);
        visitor.visit_item_impl(node);

        ImplDescriptor::from_visitor(visitor)
    }

    pub fn from_impl_allow_generics(
        emitter: &mut Emitter,
        node: &'ast syn::ItemImpl,
    ) -> Option<Self> {
        let mut visitor = ImplVisitor::new_allow_generics(emitter);
        visitor.visit_item_impl(node);

        ImplDescriptor::from_visitor(visitor)
    }

    pub fn from_foreign_impl(emitter: &mut Emitter, node: &'ast syn::ItemImpl) -> Option<Self> {
        let mut visitor = ImplVisitor::new_allow_generics(emitter);
        visitor.visit_item_impl(node);
        let mut impl_desc = Self::from_visitor(visitor)?;
        let is_drop_impl = impl_desc
            .trait_name
            .is_some_and(|trait_name| path_symbol_name(trait_name) == "Drop");

        impl_desc.fns.iter_mut().for_each(|fn_| {
            if is_drop_impl
                && fn_.sig.ident == "drop"
                && let Some(receiver) = &mut fn_.receiver
            {
                receiver.is_handle = true;
            }
            let mut arg_processor = ForeignArgProcessor {
                self_ty: fn_.self_ty.as_ref(),
            };

            if let Some(receiver) = &mut fn_.receiver {
                arg_processor.visit_type_mut(&mut receiver.type_);
            }
            if let syn::ReturnType::Type(_, output) = &mut fn_.sig.output {
                arg_processor.visit_type_mut(&mut *output);
            }
            if let Some(output_arg) = &mut fn_.output_arg {
                arg_processor.visit_type_mut(&mut output_arg.type_);
            }
        });

        Some(impl_desc)
    }

    fn from_visitor(visitor: ImplVisitor<'ast, '_>) -> Option<Self> {
        if visitor.fatal {
            return None;
        }
        Some(Self {
            trait_name: visitor.trait_name,
            generics: visitor.generics.unwrap(),
            associated_types: visitor.associated_types,
            associated_consts: visitor.associated_consts,
            fns: visitor.fns,
        })
    }
}

impl<'ast> FnDescriptor<'ast> {
    pub fn from_impl_method(
        emitter: &mut Emitter,
        self_ty: &'ast Path,
        node: &'ast syn::ImplItemFn,
    ) -> Option<Self> {
        let mut visitor = FnVisitor::new(emitter, Some(self_ty));

        visitor.visit_impl_item_fn(node);
        FnDescriptor::from_visitor(visitor)
    }

    pub fn from_fn(emitter: &mut Emitter, node: &'ast syn::ItemFn) -> Option<Self> {
        let mut visitor = FnVisitor::new(emitter, None);

        visitor.visit_item_fn(node);
        Self::from_visitor(visitor)
    }

    fn from_visitor(visitor: FnVisitor<'ast, '_>) -> Option<Self> {
        if visitor.fatal {
            return None;
        }
        Some(Self {
            attrs: visitor.attrs,
            doc: visitor.doc,
            self_ty: visitor.self_ty.cloned(),

            sig: visitor.sig.expect("Missing signature").clone(),

            receiver: visitor.receiver,
            input_args: visitor.input_args,
            output_arg: visitor.output_arg,
        })
    }

    pub fn self_ty_symbol_name(&self) -> Option<String> {
        self.self_ty.as_ref().map(path_symbol_name)
    }
}

pub(crate) fn path_symbol_name(path: &Path) -> String {
    let Some(seg) = path.segments.last() else {
        return String::from("Self");
    };

    let mut out = seg.ident.to_string();
    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
        for arg in &args.args {
            if let Some(arg_name) = generic_arg_symbol_name(arg)
                && !arg_name.is_empty()
            {
                out.push('_');
                out.push_str(&arg_name);
            }
        }
    }

    sanitize_symbol_component(&out)
}

fn generic_arg_symbol_name(arg: &syn::GenericArgument) -> Option<String> {
    match arg {
        syn::GenericArgument::Lifetime(_) => None,
        syn::GenericArgument::Type(ty) => Some(type_symbol_name(ty)),
        syn::GenericArgument::Const(expr) => Some(sanitize_symbol_component(
            &expr.to_token_stream().to_string(),
        )),
        syn::GenericArgument::AssocType(assoc) => {
            Some(format!("{}_{}", assoc.ident, type_symbol_name(&assoc.ty)))
        }
        syn::GenericArgument::AssocConst(assoc) => Some(format!(
            "{}_{}",
            assoc.ident,
            sanitize_symbol_component(&assoc.value.to_token_stream().to_string())
        )),
        syn::GenericArgument::Constraint(constraint) => Some(constraint.ident.to_string()),
        _ => Some(sanitize_symbol_component(
            &arg.to_token_stream().to_string(),
        )),
    }
}

fn type_symbol_name(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => path_symbol_name(&type_path.path),
        Type::Reference(reference) => {
            let mutability = if reference.mutability.is_some() {
                "mut_ref"
            } else {
                "ref"
            };
            format!("{mutability}_{}", type_symbol_name(&reference.elem))
        }
        Type::Slice(slice) => format!("slice_{}", type_symbol_name(&slice.elem)),
        Type::Array(array) => format!(
            "array_{}_{}",
            type_symbol_name(&array.elem),
            sanitize_symbol_component(&array.len.to_token_stream().to_string())
        ),
        Type::Ptr(ptr) => {
            let mutability = if ptr.mutability.is_some() {
                "mut_ptr"
            } else {
                "const_ptr"
            };
            format!("{mutability}_{}", type_symbol_name(&ptr.elem))
        }
        Type::Tuple(tuple) => {
            if tuple.elems.is_empty() {
                String::from("unit")
            } else {
                let items = tuple.elems.iter().map(type_symbol_name).collect::<Vec<_>>();
                format!("tuple_{}", items.join("_"))
            }
        }
        _ => sanitize_symbol_component(&ty.to_token_stream().to_string()),
    }
}

fn sanitize_symbol_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_is_us = false;

    for ch in input.chars() {
        let keep = ch.is_ascii_alphanumeric() || ch == '_';
        if keep {
            out.push(ch);
            prev_is_us = ch == '_';
        } else if !prev_is_us {
            out.push('_');
            prev_is_us = true;
        }
    }

    let out = out.trim_matches('_');
    if out.is_empty() {
        String::from("ty")
    } else {
        out.to_string()
    }
}

impl<'ast, 'emitter> ImplVisitor<'ast, 'emitter> {
    fn new(emitter: &'emitter mut Emitter) -> Self {
        Self {
            emitter,
            allow_non_lifetime_impl_generics: false,
            fatal: false,
            trait_name: None,
            self_ty: None,
            generics: None,
            associated_types: Vec::new(),
            associated_consts: Vec::new(),
            fns: vec![],
        }
    }

    fn new_allow_generics(emitter: &'emitter mut Emitter) -> Self {
        Self {
            emitter,
            allow_non_lifetime_impl_generics: true,
            fatal: false,
            trait_name: None,
            self_ty: None,
            generics: None,
            associated_types: Vec::new(),
            associated_consts: Vec::new(),
            fns: vec![],
        }
    }

    fn visit_self_type(&mut self, node: &'ast Type) {
        if let Type::Path(self_ty) = node {
            if self_ty.qself.is_some() {
                emit!(
                    self.emitter,
                    self_ty,
                    "Qualified types are not supported as self type"
                );
            }

            self.self_ty = Some(&self_ty.path);
        } else {
            emit!(
                self.emitter,
                node,
                "Only nominal types are supported as self type"
            );
        }
    }
}

impl<'ast, 'emitter> FnVisitor<'ast, 'emitter> {
    pub fn new(emitter: &'emitter mut Emitter, self_ty: Option<&'ast Path>) -> Self {
        Self {
            emitter,
            fatal: false,
            attrs: Vec::new(),
            doc: Vec::new(),
            self_ty,

            sig: None,

            receiver: None,
            input_args: vec![],
            output_arg: None,

            curr_arg_name: None,
            curr_arg_is_handle: false,
        }
    }

    fn add_input_arg(&mut self, src_type: &'ast Type) {
        let resolved = resolve_type(self.self_ty, src_type.clone());
        if matches!(resolved, Type::Array(_)) {
            unimplemented!();
        }

        let arg_name = self.curr_arg_name.take().cloned().unwrap_or_else(|| {
            // provide a dummy argument name so that codegen can work
            Ident::new(
                &format!("__arg_{}", self.input_args.len()),
                Span::call_site(),
            )
        });
        self.input_args.push(Arg::new_with_handle(
            self.self_ty.cloned(),
            arg_name,
            src_type.clone(),
            self.curr_arg_is_handle,
        ));
        self.curr_arg_is_handle = false;
    }

    fn add_output_arg(&mut self, src_type: &'ast Type) {
        assert!(self.curr_arg_name.is_none());
        assert!(self.output_arg.is_none());

        let output_arg = Arg::new(
            self.self_ty.cloned(),
            Ident::new("__output", Span::call_site()),
            src_type.clone(),
        );

        self.output_arg = Some(output_arg);
    }
}

impl<'ast> Visit<'ast> for ImplVisitor<'ast, '_> {
    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        let _ = node;
    }
    fn visit_generics(&mut self, node: &'ast syn::Generics) {
        self.generics = Some(node);
    }
    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let has_non_lifetime_generics = node
            .generics
            .params
            .iter()
            .any(|param| !matches!(param, syn::GenericParam::Lifetime(_)));
        if has_non_lifetime_generics && !self.allow_non_lifetime_impl_generics {
            self.fatal = true;
            emit!(
                self.emitter,
                node.generics,
                "Type and const generics on impl blocks are not supported"
            );
            return;
        }

        if node.unsafety.is_some() {
            emit!(self.emitter, node.unsafety, "Unsafe impl not supported");
        }
        if node.defaultness.is_some() {
            emit!(self.emitter, node.defaultness, "Default impl not supported");
        }

        for it in &node.attrs {
            self.visit_attribute(it);
        }

        self.visit_generics(&node.generics);
        self.trait_name = node.trait_.as_ref().map(|(_, trait_, _)| trait_);
        self.visit_self_type(&node.self_ty);

        let self_ty = self.self_ty.expect("Defined");
        self.associated_types
            .extend(node.items.iter().filter_map(|item| match item {
                syn::ImplItem::Type(associated_type) => {
                    Some((&associated_type.ident, &associated_type.ty))
                }
                _ => None,
            }));
        self.associated_consts
            .extend(node.items.iter().filter_map(|item| match item {
                syn::ImplItem::Const(associated_const) => Some((
                    &associated_const.ident,
                    &associated_const.ty,
                    &associated_const.expr,
                )),
                _ => None,
            }));

        for item in &node.items {
            if let syn::ImplItem::Fn(method) = item
                && let Some(desc) = FnDescriptor::from_impl_method(self.emitter, self_ty, method)
            {
                self.fns.push(desc);
            }
        }
    }
}

impl<'ast> Visit<'ast> for FnVisitor<'ast, '_> {
    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        if is_doc_attr(node) {
            self.doc.push(node);
        } else {
            self.attrs.push(node);
        }
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }

        self.sig = Some(&node.sig);
        self.visit_visibility(&node.vis);
        self.visit_signature(&node.sig);
    }
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }

        self.sig = Some(&node.sig);
        self.visit_visibility(&node.vis);
        self.visit_signature(&node.sig);
    }
    fn visit_visibility(&mut self, node: &'ast Visibility) {
        let _ = node;
    }
    fn visit_signature(&mut self, node: &'ast syn::Signature) {
        let has_non_lifetime_generics = node
            .generics
            .params
            .iter()
            .any(|param| !matches!(param, syn::GenericParam::Lifetime(_)));
        if has_non_lifetime_generics {
            self.fatal = true;
            emit!(
                self.emitter,
                node.generics,
                "Type and const generics on functions are not supported"
            );
            return;
        }

        if node.asyncness.is_some() {
            emit!(
                self.emitter,
                node.asyncness,
                "Async functions not supported"
            );
        }
        if node.variadic.is_some() {
            emit!(
                self.emitter,
                node.variadic,
                "Variadic arguments not supported"
            );
        }

        visit_signature(self, node);
    }

    fn visit_receiver(&mut self, node: &'ast syn::Receiver) {
        if let Some((_, lifetime)) = &node.reference
            && lifetime.is_some()
        {
            emit!(self.emitter, lifetime, "Explicit lifetimes not supported");
        }

        let src_type: Type = node.reference.as_ref().map_or_else(
            || parse_quote! {Self},
            |it| {
                if it.1.is_some() {
                    emit!(self.emitter, it.1, "Explicit lifetime not supported");
                }

                if node.mutability.is_some() {
                    parse_quote! {&mut Self}
                } else {
                    parse_quote! {&Self}
                }
            },
        );

        let handle_name = Ident::new("__handle", Span::call_site());
        let is_handle = node
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("dispatch"));
        self.receiver = Some(Arg::new_with_handle(
            self.self_ty.cloned(),
            handle_name,
            src_type,
            is_handle,
        ));
    }

    fn visit_pat_type(&mut self, node: &'ast syn::PatType) {
        if let syn::Pat::Ident(ident) = &*node.pat
            && ident.ident == "self"
        {
            let src_type: Option<Type> = match node.ty.as_ref() {
                Type::Path(type_path)
                    if type_path.qself.is_none() && type_path.path.is_ident("Self") =>
                {
                    Some(parse_quote! {Self})
                }
                Type::Reference(reference) => {
                    if let Type::Path(type_path) = reference.elem.as_ref()
                        && type_path.qself.is_none()
                        && type_path.path.is_ident("Self")
                    {
                        if reference.mutability.is_some() {
                            Some(parse_quote! {&mut Self})
                        } else {
                            Some(parse_quote! {&Self})
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(src_type) = src_type {
                let handle_name = Ident::new("__handle", Span::call_site());
                let is_handle = node
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("dispatch"));
                self.receiver = Some(Arg::new_with_handle(
                    self.self_ty.cloned(),
                    handle_name,
                    src_type,
                    is_handle,
                ));
                return;
            }
        }

        if let syn::Pat::Ident(ident) = &*node.pat {
            self.visit_pat_ident(ident);
        } else {
            // if we don't have an identifier (when pattern matching is used), we generate a synthetic argument name
            // it's not an error (anymore)
        }

        self.curr_arg_is_handle = node
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("dispatch"));
        self.add_input_arg(&node.ty);
    }

    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        if node.by_ref.is_some() {
            emit!(
                self.emitter,
                node.by_ref,
                "ref patterns not supported in argument name"
            );
        }
        if node.mutability.is_some() {
            // NOTE: It's irrelevant
        }
        if node.subpat.is_some() {
            emit!(
                self.emitter,
                node,
                "Subpatterns not supported in argument name"
            );
        }

        self.curr_arg_name = Some(&node.ident);
    }

    fn visit_return_type(&mut self, node: &'ast syn::ReturnType) {
        match node {
            syn::ReturnType::Default => {}
            syn::ReturnType::Type(_, src_type) => {
                self.add_output_arg(src_type);
            }
        }
    }
}

fn is_doc_attr(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("doc")
}

/// Visitor replaces all occurrences of `Self` in a path type with a fully qualified type
struct SelfResolver<'ast> {
    self_ty: &'ast Path,
}

impl<'ast> SelfResolver<'ast> {
    fn new(self_ty: &'ast Path) -> Self {
        Self { self_ty }
    }
}

impl VisitMut for SelfResolver<'_> {
    fn visit_path_mut(&mut self, node: &mut Path) {
        if node.leading_colon.is_some() {
            // NOTE: It's irrelevant
        }
        for segment in &mut node.segments {
            self.visit_path_arguments_mut(&mut segment.arguments);
        }

        if node.segments[0].ident == "Self" {
            let mut node_segments = self.self_ty.segments.clone();

            for segment in core::mem::take(&mut node.segments).into_iter().skip(1) {
                node_segments.push(segment);
            }

            node.segments = node_segments;
        }
    }
}

pub struct TypeImplTraitResolver;
impl VisitMut for TypeImplTraitResolver {
    fn visit_type_mut(&mut self, node: &mut Type) {
        let mut new_node = None;

        if let Type::ImplTrait(impl_trait) = node {
            for bound in &impl_trait.bounds {
                if let syn::TypeParamBound::Trait(trait_) = bound {
                    let trait_ = trait_.path.segments.last().expect("Defined");

                    match trait_.ident.to_string().as_str() {
                        "IntoIterator" | "ExactSizeIterator" => {
                            if let syn::PathArguments::AngleBracketed(args) = &trait_.arguments {
                                for arg in &args.args {
                                    if let syn::GenericArgument::AssocType(binding) = arg
                                        && binding.ident == "Item"
                                    {
                                        let mut ty = binding.ty.clone();
                                        TypeImplTraitResolver.visit_type_mut(&mut ty);
                                        new_node = Some(parse_quote! { Vec<#ty> });
                                    }
                                }
                            }
                        }
                        "Into" => {
                            if let syn::PathArguments::AngleBracketed(args) = &trait_.arguments {
                                for arg in &args.args {
                                    if let syn::GenericArgument::Type(type_) = arg {
                                        new_node = Some(type_.clone());
                                    }
                                }
                            }
                        }
                        "AsRef" => {
                            if let syn::PathArguments::AngleBracketed(args) = &trait_.arguments {
                                for arg in &args.args {
                                    if let syn::GenericArgument::Type(type_) = arg {
                                        new_node = Some(syn::parse_quote!(&#type_));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if let Some(new_node) = new_node {
            *node = new_node;
        }
    }
}
