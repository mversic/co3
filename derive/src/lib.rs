//! Crate containing FFI related macro functionality
//!
//! # Example
//!
//! ```rust
//! #[cfg(not(feature = "import"))]
//! struct Local(u8);
//!
//! #[cfg(not(feature = "import"))]
//! fn make_local() -> Box<Local> { Box::new(Local(0)) }
//!
//! #[cfg(not(feature = "import"))]
//! type LocalType = Box<Local>;
//! #[cfg(feature = "import")]
//! type LocalType = OwnedLocal;
//! co3::ffi! {
//!     #![cfg_attr(not(feature = "import"), unsafe(export("C")))]
//!     #![cfg_attr(feature = "import", unsafe(extern("C")))]
//!
//!     #![symbol_prefix = "provider"]
//!
//!     type Local;
//!
//!     move fn make_local() -> LocalType;
//! }
//! # fn main() {}
//! ```
use std::collections::{BTreeMap, BTreeSet, HashMap};

use manyhow::manyhow;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Attribute, Expr, ItemFn, ItemImpl, LitStr, Path, Result, StaticMutability, Type, parse_quote,
    visit_mut::VisitMut,
};

use crate::{
    cfg_attr::{emit_macro_invocations, expand as expand_cfg_attr},
    dispatch::synthesize_dispatch_tag_ids,
    generate::{expand_export_decls, expand_extern_decls},
    layout::derive_repr_c,
    parse::{ParsedInput, ParsedItem},
    utils::{
        co3_path, has_non_lifetime_generics, is_drop_impl, path_symbol_name, push_error,
        type_symbol_name,
    },
    validate::{validate_export_attrs, validate_export_decls, validate_extern_decls},
};

mod abi_retype;
mod cfg_attr;
mod dispatch;
mod ffi_fn;
mod generate;
mod layout;
mod parse;
mod statics;
mod tag;
mod utils;
mod validate;
mod wrapper;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeclKind {
    Export,
    Extern,
}

pub(crate) enum ForeignItem {
    Type(ForeignItemType),
    Static(Co3Static),
    Impl(Co3Impl),
    Fn(Co3Fn),
}

#[derive(Clone)]
struct ForeignItemType {
    ty: syn::ForeignItemType,
    id: Option<Box<syn::Type>>,
    id_value: Option<Box<Expr>>,
    covariant_lifetimes: Vec<syn::Lifetime>,
    drop: Option<Co3Impl>,

    self_impls: Vec<Co3Impl>,
}

#[derive(Clone)]
pub(crate) struct Co3Static {
    pub(crate) attrs: Vec<Attribute>,
    pub(crate) vis: syn::Visibility,
    pub(crate) static_token: syn::Token![static],
    pub(crate) mutability: StaticMutability,
    pub(crate) ident: syn::Ident,
    pub(crate) ty: Box<Type>,
    pub(crate) expr: Option<Box<Expr>>,
}

#[derive(Clone)]
struct Co3Impl {
    item: ItemImpl,

    dispatch_args: DispatchGroups,

    method_dispatch_args: HashMap<syn::Ident, DispatchGroups>,
}

#[derive(Clone)]
struct Co3Fn {
    item: ItemFn,
    dispatch_args: DispatchGroups,
}

#[derive(Clone, Default)]
struct DispatchGroups {
    groups: BTreeMap<Vec<syn::Ident>, Vec<syn::AngleBracketedGenericArguments>>,
}

pub(crate) struct DispatchSelection<'a> {
    pub(crate) params: &'a [syn::Ident],
    pub(crate) target: &'a syn::AngleBracketedGenericArguments,
}

impl Co3Impl {
    fn new(item: ItemImpl) -> Self {
        Self {
            item,
            dispatch_args: DispatchGroups::default(),
            method_dispatch_args: HashMap::default(),
        }
    }
}

impl core::ops::Deref for Co3Impl {
    type Target = ItemImpl;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl core::ops::DerefMut for Co3Impl {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

impl core::ops::Deref for Co3Fn {
    type Target = ItemFn;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl core::ops::DerefMut for Co3Fn {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

impl DispatchGroups {
    fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub(crate) fn groups(
        &self,
    ) -> impl Iterator<Item = (&[syn::Ident], &[syn::AngleBracketedGenericArguments])> {
        self.groups
            .iter()
            .map(|(params, targets)| (params.as_slice(), targets.as_slice()))
    }

    pub(crate) fn contains_param(&self, param: &syn::Ident) -> bool {
        self.groups
            .keys()
            .any(|params| params.iter().any(|candidate| candidate == param))
    }

    pub(crate) fn concretize_self(&mut self, self_ty: &syn::Type) {
        let mut concretizer = crate::ffi_fn::SelfConcretizer { self_ty };
        for targets in self.groups.values_mut() {
            for target in targets {
                for argument in &mut target.args {
                    if let syn::GenericArgument::Type(ty) = argument {
                        concretizer.visit_type_mut(ty);
                    }
                }
            }
        }
    }

    fn for_each_combination(&self, mut f: impl for<'a> FnMut(&[DispatchSelection<'a>])) {
        fn visit<'a>(
            groups: &[(&'a [syn::Ident], &'a [syn::AngleBracketedGenericArguments])],
            selections: &mut Vec<DispatchSelection<'a>>,
            f: &mut impl FnMut(&[DispatchSelection<'a>]),
        ) {
            let Some((params, targets)) = groups.first() else {
                f(selections);
                return;
            };

            for target in *targets {
                selections.push(DispatchSelection { params, target });
                visit(&groups[1..], selections, f);
                selections.pop();
            }
        }

        let groups = self.groups().collect::<Vec<_>>();
        visit(&groups, &mut Vec::new(), &mut f);
    }

    fn combined_with(&self, other: &Self) -> Self {
        let mut groups = self.groups.clone();
        groups.extend(other.groups.clone());
        Self { groups }
    }

    fn bindings_for(&self, root: &syn::Ident) -> Self {
        let all_params = self
            .groups()
            .flat_map(|(params, _)| params.iter().cloned())
            .collect::<BTreeSet<_>>();
        let mut selected = BTreeSet::from([root.clone()]);

        loop {
            let mut next = selected.clone();
            for (params, targets) in self.groups() {
                let selected_indices = params
                    .iter()
                    .enumerate()
                    .filter_map(|(index, param)| selected.contains(param).then_some(index))
                    .collect::<Vec<_>>();
                if selected_indices.is_empty() {
                    continue;
                }
                for candidate in &all_params {
                    let detector = crate::utils::ParamUseDetector::new([candidate]);
                    if targets.iter().any(|target| {
                        selected_indices.iter().any(|index| {
                            target
                                .args
                                .get(*index)
                                .is_some_and(|arg| detector.generic_arg_mentions_param(arg))
                        })
                    }) {
                        next.insert(candidate.clone());
                    }
                }
            }
            if next == selected {
                break;
            }
            selected = next;
        }

        let mut bindings = Self::default();
        for (params, targets) in self.groups() {
            let indices = params
                .iter()
                .enumerate()
                .filter_map(|(index, param)| selected.contains(param).then_some(index))
                .collect::<Vec<_>>();
            if indices.is_empty() {
                continue;
            }
            let projected_params = indices
                .iter()
                .map(|index| params[*index].clone())
                .collect::<Vec<_>>();
            let projected_targets = targets
                .iter()
                .map(|target| {
                    let mut target = target.clone();
                    target.args = target
                        .args
                        .into_iter()
                        .enumerate()
                        .filter_map(|(index, arg)| indices.contains(&index).then_some(arg))
                        .collect();
                    target
                })
                .fold(Vec::new(), |mut unique, target| {
                    if !unique.contains(&target) {
                        unique.push(target);
                    }
                    unique
                });
            bindings.groups.insert(projected_params, projected_targets);
        }
        bindings
    }

    fn static_bindings(
        &self,
        generics: &syn::Generics,
        static_binding_params: &BTreeSet<syn::Ident>,
    ) -> Vec<(Self, Self)> {
        fn is_static_param(
            generics: &syn::Generics,
            static_binding_params: &BTreeSet<syn::Ident>,
            ident: &syn::Ident,
        ) -> bool {
            if !static_binding_params.contains(ident) {
                return false;
            }

            generics.params.iter().any(|param| match param {
                syn::GenericParam::Type(param) => {
                    param.ident == *ident && !param.attrs.iter().any(crate::utils::is_type_erased)
                }
                syn::GenericParam::Const(param) => param.ident == *ident,
                syn::GenericParam::Lifetime(_) => false,
            })
        }

        fn project_target(
            target: &syn::AngleBracketedGenericArguments,
            indices: &[usize],
        ) -> syn::AngleBracketedGenericArguments {
            let mut projected = target.clone();
            projected.args = target
                .args
                .iter()
                .enumerate()
                .filter(|(idx, _)| indices.contains(idx))
                .map(|(_, arg)| arg.clone())
                .collect();
            projected
        }

        let mut bindings = vec![(Self::default(), Self::default())];
        for (params, targets) in self.groups() {
            let static_indices = params
                .iter()
                .enumerate()
                .filter_map(|(idx, param)| {
                    is_static_param(generics, static_binding_params, param).then_some(idx)
                })
                .collect::<Vec<_>>();
            let dynamic_indices = (0..params.len())
                .filter(|idx| !static_indices.contains(idx))
                .collect::<Vec<_>>();

            let static_params = static_indices
                .iter()
                .map(|idx| params[*idx].clone())
                .collect::<Vec<_>>();
            let dynamic_params = dynamic_indices
                .iter()
                .map(|idx| params[*idx].clone())
                .collect::<Vec<_>>();

            let mut variants = BTreeMap::<String, (Self, Self)>::new();
            for target in targets {
                let static_target = project_target(target, &static_indices);
                let key = quote::quote!(#static_target).to_string();
                let (static_groups, dynamic_groups) = variants.entry(key).or_default();
                if !static_params.is_empty() {
                    static_groups
                        .groups
                        .entry(static_params.clone())
                        .or_insert_with(|| vec![static_target]);
                }
                if !dynamic_params.is_empty() {
                    dynamic_groups
                        .groups
                        .entry(dynamic_params.clone())
                        .or_default()
                        .push(project_target(target, &dynamic_indices));
                }
            }

            let variants = variants.into_values().collect::<Vec<_>>();
            let mut next = Vec::new();
            for (binding_static, binding_dynamic) in bindings {
                for (group_static, group_dynamic) in &variants {
                    next.push((
                        binding_static.combined_with(group_static),
                        binding_dynamic.combined_with(group_dynamic),
                    ));
                }
            }
            bindings = next;
        }
        bindings
    }

    pub(crate) fn inject_unnamed_lifetimes(&mut self, generics: &mut syn::Generics) {
        for targets in self.groups.values_mut() {
            for target in targets {
                *target =
                    crate::dispatch::inject_unnamed_lifetimes(&mut generics.params, target.clone());
            }
        }
    }
}

/// Generate a C-compatible counterpart and conversions to and from the Rust type.
///
/// A type deriving [`co3::ReprC`](https://docs.rs/co3/latest/co3/derive.ReprC.html) can participate
/// in [`ffi!`] declarations that use C ABI. Note that most types will also require an
/// implementation of [`rust_spec::RustSpec`](https://docs.rs/rust-spec/latest/rust_spec/trait.RustSpec.html).
///
/// # Helper Attributes
///
/// * `#[reprC(NICHE_VALUE = <expr>)]` on a struct customizes
///   [`co3::niche::Niche::NICHE_VALUE`](https://docs.rs/co3/latest/co3/niche/trait.Niche.html#associatedconstant.NICHE_VALUE)
/// * `#[reprC(is_valid = |[fieldN]| ...)]` on a struct or enum variant customizes validation
/// * `#[reprC(identity)]` uses a `repr(C)` or `repr(transparent)` struct directly as its CType
///
/// # Example
///
/// ```rust
/// use co3::{ReprC, rust_spec::RustSpec};
///
/// #[derive(RustSpec, ReprC)]
/// pub struct Hello(u32);
/// # fn main() {}
/// ```
#[manyhow]
// FIXME: It's totally weird that `tag` is part of ReprC
#[proc_macro_derive(ReprC, attributes(reprC, tag))]
pub fn repr_c_derive(item: syn::DeriveInput) -> Result<TokenStream> {
    derive_repr_c(&item)
}

/// Derives the tagged-dispatch traits for a Rust type.
///
/// - `#[tag(Type, unsafe(value))]` defines both the tag type and value of the derived type.
/// - `#[tag(Type)]` defines only the tag type; `Tagged` trait is then implemented by hand.
#[manyhow]
#[proc_macro_derive(Tag, attributes(tag))]
pub fn tag_derive(item: syn::DeriveInput) -> Result<TokenStream> {
    tag::derive_tag(&item)
}

/// Declare FFI exports or imports via native API.
///
/// `ffi!` generates ABI-facing wrappers and conversions for statics, free functions, inherent and
/// trait methods, and extern/opaque types. Value representations are checked for traps, including
/// pointees. **Ownership transfer is opt-in**.
///
/// **Except for statics, export declarations are completely interchangeable for imports**
///
/// # Safety
///
/// An import declaration is an unsafe assertion that the linked provider matches the declared ABI
/// contract, including symbol names, calling convention, complete item signatures, ownership,
/// lifetime, mutability, and aliasing requirements.
///
/// Foreign callers must ensure that non-null pointer-backed arguments satisfy Rust's reference
/// rules for the types declared by the exported function for the duration of the call.
///
/// The exporting crate must ensure that exported symbol names are unique among linked definitions.
/// Multiple `ffi!` blocks can generate colliding names, so prefer a single export block per crate
/// and prefer exporting **only** symbols defined in that crate.
///
/// # Import from an external library
///
/// Use `#![unsafe(extern("ABI"))]` to declare the Rust-facing interface implemented by an external
/// library:
///
/// ```rust
/// use co3::{ffi, ReprC};
///
/// // `ReprC` generates a stable C-compatible companion even without an explicit `#[repr(...)]`.
/// // If given, an explicit representation would be leveraged to produce a more optimal mapping.
/// #[derive(Clone, Copy, ReprC)]
/// struct Value(u32);
///
/// trait Counter {
///     fn increment(&mut self, by: Value);
/// }
///
/// ffi! {
///     #![unsafe(extern("C"))]
///
///     type CounterHandle;
///
///     impl Counter for CounterHandle {
///         fn increment(&mut self, by: Value);
///     }
/// }
/// # fn main() {}
/// ```
///
/// # Export from Rust
///
/// Use `#![unsafe(export("ABI"))]` to expose existing Rust items. Although an ABI can be exported
/// on its own, the provider will often also provide a Rust client as well. In the common case,
/// export declarations are naturally paired with matching import declarations via a shared crate
/// that defines the application interface.
///
/// ```rust
/// use co3::ffi;
///
/// #[cfg(not(feature = "import"))]
/// pub struct CounterHandle(u32);
///
/// #[cfg(not(feature = "import"))]
/// impl core::ops::AddAssign<u32> for CounterHandle {
///     fn add_assign(&mut self, rhs: u32) {
///         self.0 += rhs;
///     }
/// }
///
/// #[cfg(not(feature = "import"))]
/// fn increment<T: core::ops::AddAssign<u32>>(value: &mut T) {
///     *value += 1;
/// }
///
/// ffi! {
///     #![cfg_attr(not(feature = "import"), unsafe(export("C")))]
///     #![cfg_attr(feature = "import", unsafe(extern("C")))]
///
///     type CounterHandle;
///
///     fn increment(value: &mut CounterHandle);
/// }
/// # fn main() {}
/// ```
///
/// # Naming convention
///
/// ABI symbols follow a stable naming convention:
///
/// - Static items: `{prefix}__{static_name}`
/// - Free functions: `{prefix}__{function_name}`
/// - Inherent methods: `{prefix}__{SelfType}__{method_name}`
/// - Trait methods: `{prefix}__{TraitPath}__{SelfType}__{method_name}`
///
/// By default, `ffi!` uses `CARGO_CRATE_NAME` as the symbol prefix. `#![symbol_prefix = "..."]`
/// overrides the prefix for the entire scope, whereas `#[symbol_name = "..."]` overrides the name
/// for the declaration it is applied to:
///
/// ```rust
/// use co3::ffi;
///
/// trait Operations {
///     fn add(&self, left: u32, right: u32) -> u32;
/// }
///
/// ffi! {
///     #![unsafe(extern("C"))]
///     #![symbol_prefix = "my_lib"]
///
///     type Calculator;
///
///     // Linked as `my_lib__subtract`.
///     fn subtract(left: u32, right: u32) -> u32;
///
///     impl Calculator {
///         // Linked as `my_lib__Calculator__multiply`.
///         fn multiply(&self, left: u32, right: u32) -> u32;
///     }
///
///     impl Operations for Calculator {
///         // Linked as `library_add`.
///         #[symbol_name = "library_add"]
///         fn add(&self, left: u32, right: u32) -> u32;
///     }
/// }
/// # fn main() {}
/// ```
///
/// # Static parameter interpolation
///
/// A generic parameter is selected at compile time with `where use<Param, ...> @ (ConcreteTy | ...)`
/// where each selection of a static parameter generates a specialized function export or import with
/// a distinct ABI symbol.
///
/// An explicit `#[symbol_name]` **MUST** interpolate every selected static parameter exactly once
/// using `{Parameter}`. `#![symbol_fragments { ... }]` defines the interpolated string value.
///
/// ```rust
/// use co3::ffi;
///
/// pub struct SQLCHAR;
/// pub struct SQLWCHAR;
///
/// ffi! {
///     #![unsafe(extern("C"))]
///     #![symbol_fragments {
///         SQLCHAR = "A",
///         SQLWCHAR = "W",
///     }]
///
///     #[symbol_name = "convert_{T}"]
///     fn convert<T>()
///     where
///         use<T> @ (<SQLCHAR> | <SQLWCHAR>);
/// }
/// # fn main() {}
/// ```
///
/// This synthesizes 2 function imports: `convert_A` and `convert_W`. Primitive types have stable
/// built-in fragments, while const arguments are converted directly into valid symbol fragments.
///
/// **Runtime-dispatched `dyn(...)` parameters use tags instead and are not interpolated.**
///
/// # Tagged dispatch
///
/// Tagged dispatch is a dynamic dispatch over a closed set of concrete implementations commonly used
/// in C APIs. The concrete type is erased at the FFI boundary and carried as a shared representation
/// accompanied by a tag that identifies the concrete implementation to invoke. The tag position is
/// inferred at the start of the function parameter list but can also be specified explicitly. Every
/// tag-dispatched concrete instantiation is compile-time checked to have a C-compatible representation
/// with the same size and alignment of the declared shared ABI type.
///
/// In the following example:
/// - `T` is a tag-dispatched type parameter
/// - The tag type of parameter `T` is chosen as `u8`
/// - `u16` specifies the shared ABI representation used in the place of every concrete `T`.
/// - `where use<T> @ (...)` defines the set of concrete instantiations of dispatched types.
/// - `dyn Self` dispatches the `Self` parameter, but is only allowed for extern/opaque types.
///
/// The example corresponds to this C counterpart:
///
/// ```rust
/// use co3::{ffi, Tag, ReprC, rust_spec::RustSpec};
/// use co3::tag::Tagged;
///
/// #[derive(RustSpec, Tag, ReprC)]
/// #[tag(u8, unsafe(1))]
/// #[repr(transparent)]
/// struct LocalCounter(u16);
///
/// #[derive(RustSpec, Tag, ReprC)]
/// #[tag(u8, unsafe(2))]
/// #[repr(transparent)]
/// struct RemoteCounter(u16);
///
/// trait Counter {
///     fn increment(&mut self, by: u8);
/// }
///
/// trait Reset {
///     fn reset(&mut self);
/// }
///
/// unsafe impl Tagged for CounterHandle<i16> {
///     const TAG: u8 = 3;
/// }
///
/// unsafe impl Tagged for CounterHandle<u16> {
///     const TAG: u8 = 4;
/// }
///
/// ffi! {
///     #![unsafe(extern("C"))]
///
///     #[tag(u8)]
///     type CounterHandle<T>;
///
///     impl<T> Drop for CounterHandle<T> {
///         fn drop(&mut self);
///     }
///
///     // Declare `T` as tag dispatched
///     impl<dyn(u8) T = u16> Counter for T
///     where
///         // Select concrete types with the shared `u16` ABI representation.
///         use<T> @ (<LocalCounter> | <RemoteCounter>)
///     {
///         // Make the tag-carrying argument position explicit.
///         fn increment(t_tag: <dyn T>::TAG, &mut self, by: u8);
///     }
///
///     // Dispatch the extern type itself
///     impl<T> Reset for dyn CounterHandle<T>
///     where
///         use<T> @ (<i16> | <u16>)
///     {
///         fn reset(self_tag: <dyn Self>::TAG, &mut self);
///     }
/// }
/// # fn main() {}
/// ```
///
/// # Ownership transfer
///
/// In Rust, passing a value to a function transfers its ownership. Across an FFI boundary, however,
/// ownership transfer carries additional requirements which are a common cause of UB, such as
/// agreeing on the allocator and who is responsible for freeing the allocation.
///
/// Because it is designed to eliminate common footguns in FFI, `CO3` takes an opinionated stance here.
/// By default, all owned values (e.g. `Vec<T>`) are exported as references and immediately cloned on
/// the importing side. The universal guarantee is that any reference be valid for the duration of
/// the function call; past that, it is the user's responsibility to ensure reference validity.
///
/// Apply `move` to transfer ownership of an argument or return value:
///
/// ```rust
/// use co3::ffi;
///
/// fn passthrough(input: Vec<u8>) -> Vec<u8> {
///     input
/// }
///
/// fn clone_into(input: Vec<u8>) {
///     drop(input);
/// }
///
/// ffi! {
///     #![unsafe(export("C"))]
///
///     // `input` and the return both transfer ownership
///     move fn passthrough(move input: Vec<u8>) -> Vec<u8>;
///
///     // `input` is passed by reference
///     fn clone_into(input: Vec<u8>);
/// }
/// # fn main() {}
/// ```
/// # Soft references
///
/// `#[soft]` opts a function argument into conversion through temporary backing storage. This enables
/// mutable references to non-robust values (e.g. `&mut bool`) or values that cannot be represented
/// in place (e.g. `&(u32, u32)`). The caveat is that the referent is converted into a temporary
/// C-compatible representation instead of reusing its original address.
/// **Pointer identity is not preserved.**
///
/// Changes made through mutable references are synchronized back to the original value after the
/// call. A synchronization failure follows the configured failure mode.
///
/// Apply `#[soft]` to a function argument to opt into this kind of conversion:
///
/// ```rust
/// use co3::ffi;
///
/// fn increment(value: (&(u8, u32), u32)) -> u8 {
///     value.0.0 + 1
/// }
///
/// ffi! {
///     #![unsafe(export("C"))]
///
///     fn increment(#[soft] value: (&(u8, u32), u32)) -> u8;
/// }
/// # fn main() {}
/// ```
///
/// # Unpacking at the ABI boundary
///
/// Rust slices are lowered into [`CSlice`](https://docs.rs/co3/latest/co3/slice/struct.CSlice.html)/[`CSliceMut`](https://docs.rs/co3/latest/co3/slice/struct.CSliceMut.html)
/// which are C-ABI containers holding a data pointer and a length. However, it is common for FFI APIs to instead accept those components as separate function arguments.
/// Mark an argument with `#[unpack(T1, T2)]` to import its two ABI parts:
///
/// ```rust
/// # use co3::ffi;
///
/// ffi! {
///     #![unsafe(extern("system"))]
///
///     // - imported as `sum(*const u32, usize)`
///     fn sum(#[unpack(_, usize)] values: &[u32]) -> u32;
/// }
/// ```
///
/// **This pattern is not limited to slices**; it applies to every type implementing the [`Unpack2`](https://docs.rs/co3/latest/co3/slice/trait.Unpack2.html) trait.
///
/// # Failure modes
///
/// Select how failures are reported over the FFI boundary:
/// - `#![failure = "error"]`: return type must implement
///   [`co3::Error`](https://docs.rs/co3/latest/co3/trait.Error.html).
/// - `#![failure = "panic"]`: panic on failure (the default).
///
/// ```rust
/// use co3::{ffi, Error, ReprC, rust_spec::RustSpec};
///
/// #[derive(RustSpec, ReprC)]
/// #[repr(u8)]
/// enum ApiError { InvalidRepresentation, UnknownHandle, SoftSync }
///
/// impl Error for ApiError {
///     fn trap_value() -> Self { Self::InvalidRepresentation }
///     fn unknown_tag() -> Self { Self::UnknownHandle }
///     fn soft_sync_error() -> Self { Self::SoftSync }
/// }
///
/// fn check_error(value: u8) -> ApiError {
///     let _ = value;
///     ApiError::InvalidRepresentation
/// }
///
/// ffi! {
///     #![unsafe(export("C"))]
///     #![failure = "error"]
///
///     fn check_error(value: u8) -> ApiError;
/// }
///
/// ffi! {
///     #![unsafe(extern("C"))]
///
///     fn check_panic(value: u8) -> u32;
/// }
/// # fn main() {}
/// ```
///
/// # Opaque type variance
///
/// Lifetime and type parameters of declared opaque types are invariant by default. A lifetime
/// parameter can explicitly be declared covariant with `#[unsafe(covariant(...))]`:
///
/// ```rust
/// use co3::ffi;
///
/// ffi! {
///     #![unsafe(extern("C"))]
///
///     #[unsafe(covariant('parent))]
///     type Child<'parent, T>;
///
///     impl<'parent, T> Drop for Child<'parent, T> {
///         fn drop(&mut self);
///     }
/// }
/// ```
///
/// The attribute is unsafe because it asserts that the provider's real type is covariant over every
/// listed lifetime. Type parameters cannot be listed and remain invariant.
#[manyhow]
#[proc_macro]
pub fn ffi(input: TokenStream) -> Result<TokenStream> {
    let cfg_attr_variants = expand_cfg_attr(input.clone())?;

    if cfg_attr_variants.len() == 1 {
        let ParsedInput {
            kind,
            abi,
            symbol_prefix,
            features,
            failure_mode,
            symbol_fragments,
            attrs,
            items,
        } = ParsedInput::parse(input)?;

        let items = normalize_items(items)?;
        let mut items = pack_items(items)?;
        validate_items(kind, &attrs, &items)?;
        synthesize_items(&symbol_prefix, &mut items)?;

        return Ok(match kind {
            DeclKind::Export => {
                expand_export_decls(abi, features, failure_mode, items, &symbol_fragments)
            }
            DeclKind::Extern => expand_extern_decls(
                abi,
                features,
                failure_mode,
                &attrs,
                items,
                &symbol_fragments,
            ),
        });
    }

    let co3 = co3_path();
    Ok(emit_macro_invocations(
        quote!(#co3::ffi),
        TokenStream::new(),
        cfg_attr_variants,
    ))
}

fn normalize_items(items: Vec<ParsedItem>) -> Result<Vec<ForeignItem>> {
    items.into_iter().map(ParsedItem::normalize).collect()
}

fn pack_items(items: Vec<ForeignItem>) -> Result<Vec<ForeignItem>> {
    pack_type_drop_impls(pack_type_self_impls(items))
}

fn validate_items(kind: DeclKind, attrs: &[Attribute], items: &[ForeignItem]) -> Result<()> {
    match kind {
        DeclKind::Export => {
            validate_export_attrs(attrs)?;
            validate_export_decls(items)
        }
        DeclKind::Extern => validate_extern_decls(items),
    }
}

fn synthesize_items(symbol_prefix: &LitStr, items: &mut [ForeignItem]) -> Result<()> {
    synthesize_dispatch_tag_ids_in_decls(items)?;

    let declared_types = declared_foreign_type_idents(items);
    for item in &mut *items {
        ensure_symbol_names(item, symbol_prefix, &declared_types);
    }

    synthesize_default_drop_impls(symbol_prefix, items)?;

    Ok(())
}

fn normalize_dyn_self_tag_ids(impl_: &mut ItemImpl) {
    struct Normalizer<'a> {
        receiver: &'a Type,
    }

    impl VisitMut for Normalizer<'_> {
        fn visit_type_path_mut(&mut self, ty: &mut syn::TypePath) {
            let receiver_bound =
                trait_object_single_trait_bound(self.receiver).map(|bound| &bound.path);
            if ty.path.segments.len() == 1
                && ty.path.segments[0].ident == "TAG"
                && ty.path.segments[0].arguments.is_none()
                && ty.qself.as_ref().is_some_and(|qself| {
                    qself.ty.as_ref() == self.receiver
                        || matches!(
                            qself.ty.as_ref(),
                            Type::Path(path)
                                if receiver_bound.is_some_and(|bound| path.path == *bound)
                        )
                })
            {
                *ty.qself.as_mut().unwrap().ty = parse_quote!(dyn Self);
            }

            syn::visit_mut::visit_type_path_mut(self, ty);
        }
    }

    let receiver = (*impl_.self_ty).clone();
    let mut normalizer = Normalizer {
        receiver: &receiver,
    };
    for item in &mut impl_.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };
        normalizer.visit_signature_mut(&mut method.sig);
    }
}

pub(crate) fn is_declared_dyn_self_impl(
    impl_: &ItemImpl,
    declared_types: &BTreeSet<syn::Ident>,
) -> bool {
    let Some(bound) = trait_object_single_trait_bound(&impl_.self_ty) else {
        return false;
    };
    let Some(ident) = direct_trait_bound_ident(bound) else {
        return false;
    };
    declared_types.contains(ident)
}

fn direct_trait_bound_ident(bound: &syn::TraitBound) -> Option<&syn::Ident> {
    (bound.path.segments.len() == 1).then(|| &bound.path.segments[0].ident)
}

fn synthesize_default_drop_impls(
    symbol_prefix: &syn::LitStr,
    items: &mut [ForeignItem],
) -> Result<()> {
    let selected_drop_types = selected_drop_types(items);
    for item in items {
        let ForeignItem::Type(item) = item else {
            continue;
        };
        if item.drop.is_some() || selected_drop_types.contains(&item.ty.ident) {
            continue;
        }

        if has_non_lifetime_generics(&item.ty.generics) {
            continue;
        }

        item.drop = Some(Co3Impl::new(synthesize_default_drop_impl(
            symbol_prefix,
            &item.ty,
        )));
    }

    Ok(())
}

fn synthesize_dispatch_tag_ids_in_decls(items: &mut [ForeignItem]) -> Result<()> {
    let declared_types = declared_foreign_type_idents(items);

    fn synthesize_impl(impl_: &mut Co3Impl, declared_types: &BTreeSet<syn::Ident>) -> Result<()> {
        let generics = impl_.generics.clone();
        let self_ty = (*impl_.self_ty).clone();
        let dyn_self = is_declared_dyn_self_impl(impl_, declared_types);
        let mut dispatched_methods = impl_
            .method_dispatch_args
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        dispatched_methods.extend(impl_.items.iter().filter_map(|item| {
            let syn::ImplItem::Fn(method) = item else {
                return None;
            };
            method
                .sig
                .generics
                .type_params()
                .any(|param| param.attrs.iter().any(utils::is_type_erased))
                .then(|| method.sig.ident.clone())
        }));

        if dyn_self {
            for item in &mut impl_.items {
                let syn::ImplItem::Fn(method) = item else {
                    continue;
                };
                synthesize_dispatch_tag_ids(Some(&self_ty), &generics, &mut method.sig.inputs);
            }
        }

        if !impl_.dispatch_args.is_empty() || utils::has_runtime_dispatch(&generics) {
            for item in &mut impl_.items {
                let syn::ImplItem::Fn(method) = item else {
                    continue;
                };
                synthesize_dispatch_tag_ids(None, &generics, &mut method.sig.inputs);
            }
        }

        for item in &mut impl_.items {
            let syn::ImplItem::Fn(method) = item else {
                continue;
            };
            if dispatched_methods.contains(&method.sig.ident) {
                let mut visible_generics = method.sig.generics.clone();
                ffi_fn::merge_generics(generics.clone(), &mut visible_generics);
                synthesize_dispatch_tag_ids(
                    dyn_self.then_some(&self_ty),
                    &visible_generics,
                    &mut method.sig.inputs,
                );
            }
        }

        Ok(())
    }

    for item in items {
        match item {
            ForeignItem::Type(item) => {
                for impl_ in &mut item.self_impls {
                    synthesize_impl(impl_, &declared_types)?;
                }
                if let Some(drop) = &mut item.drop {
                    synthesize_impl(drop, &declared_types)?;
                }
            }
            ForeignItem::Impl(impl_) => synthesize_impl(impl_, &declared_types)?,
            ForeignItem::Fn(item)
                if !item.dispatch_args.is_empty()
                    || utils::has_runtime_dispatch(&item.sig.generics) =>
            {
                let generics = item.sig.generics.clone();
                synthesize_dispatch_tag_ids(None, &generics, &mut item.sig.inputs);
            }
            ForeignItem::Fn(_) | ForeignItem::Static(_) => {}
        }
    }

    Ok(())
}

fn declared_foreign_type_idents(items: &[ForeignItem]) -> BTreeSet<syn::Ident> {
    items
        .iter()
        .filter_map(|item| match item {
            ForeignItem::Type(item) => Some(item.ty.ident.clone()),
            _ => None,
        })
        .collect()
}

fn ensure_symbol_names(
    decl: &mut ForeignItem,
    symbol_prefix: &LitStr,
    declared_types: &BTreeSet<syn::Ident>,
) {
    match decl {
        ForeignItem::Fn(item) => ensure_symbol_names_on_fn(item, symbol_prefix, declared_types),
        ForeignItem::Impl(impl_) => {
            ensure_symbol_names_on_impl(impl_, symbol_prefix, declared_types);
        }
        ForeignItem::Type(item) => {
            for impl_ in &mut item.self_impls {
                ensure_symbol_names_on_impl(impl_, symbol_prefix, declared_types);
            }
            if let Some(drop) = &mut item.drop {
                ensure_symbol_names_on_impl(drop, symbol_prefix, declared_types);
            }
        }
        ForeignItem::Static(static_) => {
            ensure_symbol_name_on_static(&mut static_.attrs, symbol_prefix, &static_.ident);
        }
    }
}

fn ensure_symbol_names_on_fn(
    item: &mut Co3Fn,
    symbol_prefix: &LitStr,
    declared_types: &BTreeSet<syn::Ident>,
) {
    let ident = item.sig.ident.clone();

    let static_params = crate::validate::fn_symbol_binding_params(item, declared_types);
    ensure_symbol_name_on_fn(&mut item.attrs, symbol_prefix, &ident, &static_params);
}

fn ensure_symbol_names_on_impl(
    impl_: &mut Co3Impl,
    symbol_prefix: &LitStr,
    declared_types: &BTreeSet<syn::Ident>,
) {
    let trait_ = impl_.trait_.as_ref().map(|(path, _)| path.clone());
    let self_ty = (*impl_.self_ty).clone();
    let generics = impl_.generics.clone();
    let static_params = impl_
        .items
        .iter()
        .filter_map(|item| {
            let syn::ImplItem::Fn(method) = item else {
                return None;
            };
            Some((
                method.sig.ident.clone(),
                crate::validate::impl_method_symbol_binding_params(impl_, method, declared_types),
            ))
        })
        .collect::<HashMap<_, _>>();

    for item in &mut impl_.items {
        let syn::ImplItem::Fn(syn::ImplItemFn { attrs, sig, .. }) = item else {
            continue;
        };

        ensure_symbol_name_on_impl_fn(
            attrs,
            symbol_prefix,
            trait_.as_ref(),
            &self_ty,
            &generics,
            &sig.ident,
            &static_params[&sig.ident],
        );
    }
}

pub(crate) fn is_symbol_name_attr(attr: &Attribute) -> bool {
    attr.path().is_ident("symbol_name")
}

pub(crate) fn symbol_name_value(attr: &Attribute) -> Option<&Expr> {
    if !is_symbol_name_attr(attr) {
        return None;
    }

    let syn::Meta::NameValue(nv) = &attr.meta else {
        return None;
    };

    Some(&nv.value)
}

fn selected_drop_target_idents(drop: &Co3Impl) -> Option<Vec<syn::Ident>> {
    if !is_drop_impl(&drop.item) {
        return None;
    }

    let Type::Path(self_ty) = drop.item.self_ty.as_ref() else {
        return None;
    };
    let param = self_ty.path.get_ident()?;
    if !drop
        .item
        .generics
        .type_params()
        .any(|candidate| candidate.ident == *param)
        || !drop.dispatch_args.contains_param(param)
    {
        return None;
    }

    let (params, targets) = drop
        .dispatch_args
        .groups()
        .find(|(params, _)| params.contains(param))?;
    let param_index = params.iter().position(|candidate| candidate == param)?;
    Some(
        targets
            .iter()
            .filter_map(|target| match target.args.get(param_index) {
                Some(syn::GenericArgument::Type(Type::Path(target)))
                    if target.qself.is_none() && target.path.segments.len() == 1 =>
                {
                    target
                        .path
                        .segments
                        .first()
                        .map(|segment| segment.ident.clone())
                }
                _ => None,
            })
            .collect(),
    )
}

pub(crate) fn selected_drop_types(decls: &[ForeignItem]) -> BTreeSet<syn::Ident> {
    decls
        .iter()
        .filter_map(|decl| match decl {
            ForeignItem::Impl(impl_) => selected_drop_target_idents(impl_),
            ForeignItem::Type(_) | ForeignItem::Fn(_) | ForeignItem::Static(_) => None,
        })
        .flatten()
        .collect()
}

fn pack_type_drop_impls(decls: Vec<ForeignItem>) -> Result<Vec<ForeignItem>> {
    fn insert_drop(
        explicit_drops: &mut BTreeMap<syn::Ident, Co3Impl>,
        errors: &mut Option<syn::Error>,
        self_ty: syn::Ident,
        drop_impl: Co3Impl,
    ) {
        if let Some(prev) = explicit_drops.insert(self_ty, drop_impl) {
            let err_msg = "duplicate explicit `impl Drop` declaration";
            push_error(errors, syn::Error::new_spanned(prev.item.self_ty, err_msg));
        }
    }

    fn self_ty_ident(impl_: &syn::ItemImpl) -> Option<syn::Ident> {
        match &*impl_.self_ty {
            Type::Path(syn::TypePath {
                qself: None, path, ..
            }) if path.segments.len() == 1 => path.segments.last().map(|seg| seg.ident.clone()),
            self_ty => trait_object_single_trait_bound(self_ty)
                .filter(|bound| bound.path.segments.len() == 1)
                .and_then(|bound| bound.path.segments.first())
                .map(|segment| segment.ident.clone()),
        }
    }

    const UNKNOWN_DROP: &str = "explicit `impl Drop` is only allowed for declared types";

    let mut dynamic_drop_coverage = BTreeMap::<syn::Ident, usize>::new();
    for decl in &decls {
        let ForeignItem::Impl(drop) = decl else {
            continue;
        };
        let Some(targets) = selected_drop_target_idents(drop) else {
            continue;
        };
        for target in targets.into_iter().collect::<BTreeSet<_>>() {
            *dynamic_drop_coverage.entry(target).or_default() += 1;
        }
    }
    let mut kept_decls = Vec::with_capacity(decls.len());
    let mut explicit_drops = BTreeMap::new();
    let mut errors = None::<syn::Error>;

    for decl in decls {
        match decl {
            ForeignItem::Type(mut item) => {
                let mut kept_self_impls = Vec::with_capacity(item.self_impls.len());

                let self_ty = &item.ty.ident;
                if let Some(drop) = item.drop.take() {
                    insert_drop(&mut explicit_drops, &mut errors, self_ty.clone(), drop);
                }
                for dyn_impl in item.self_impls {
                    if is_drop_impl(&dyn_impl.item) {
                        insert_drop(&mut explicit_drops, &mut errors, self_ty.clone(), dyn_impl);
                    } else {
                        kept_self_impls.push(dyn_impl);
                    }
                }

                item.self_impls = kept_self_impls;
                kept_decls.push(ForeignItem::Type(item));
            }
            ForeignItem::Impl(impl_) => {
                if is_drop_impl(&impl_) {
                    if selected_drop_target_idents(&impl_).is_some() {
                        kept_decls.push(ForeignItem::Impl(impl_));
                        continue;
                    }
                    if let Some(self_ty) = self_ty_ident(&impl_) {
                        insert_drop(&mut explicit_drops, &mut errors, self_ty, impl_);
                    } else {
                        let err = syn::Error::new_spanned(&impl_.self_ty, UNKNOWN_DROP);
                        push_error(&mut errors, err);
                    }
                } else {
                    kept_decls.push(ForeignItem::Impl(impl_));
                }
            }
            ForeignItem::Fn(item) => kept_decls.push(ForeignItem::Fn(item)),
            ForeignItem::Static(item) => kept_decls.push(ForeignItem::Static(item)),
        }
    }

    for decl in &mut kept_decls {
        let ForeignItem::Type(item) = decl else {
            continue;
        };

        item.drop = explicit_drops.remove(&item.ty.ident);
        let dynamic_count = dynamic_drop_coverage
            .get(&item.ty.ident)
            .copied()
            .unwrap_or_default();
        if dynamic_count > 1 || (dynamic_count == 1 && item.drop.is_some()) {
            let err_msg = "duplicate `impl Drop` declaration for opaque type";
            push_error(&mut errors, syn::Error::new_spanned(&item.ty, err_msg));
        }
    }

    for drop_impl in explicit_drops.into_values() {
        push_error(
            &mut errors,
            syn::Error::new_spanned(drop_impl.item.self_ty, UNKNOWN_DROP),
        );
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(kept_decls)
}

pub(crate) fn trait_object_single_trait_bound(self_ty: &syn::Type) -> Option<&syn::TraitBound> {
    let syn::Type::TraitObject(trait_object) = self_ty else {
        return None;
    };
    if trait_object.bounds.len() != 1 {
        return None;
    }

    let bound = trait_object.bounds.first()?;
    let syn::TypeParamBound::Trait(trait_bound) = bound else {
        return None;
    };
    if trait_bound.maybe.is_some() || trait_bound.lifetimes.is_some() {
        return None;
    }

    Some(trait_bound)
}

fn pack_type_self_impls(decls: Vec<ForeignItem>) -> Vec<ForeignItem> {
    fn declared_self_type(impl_: &ItemImpl) -> Option<syn::Ident> {
        match &*impl_.self_ty {
            Type::Path(syn::TypePath {
                qself: None, path, ..
            }) if path.segments.len() == 1 => {
                path.segments.first().map(|segment| segment.ident.clone())
            }
            _ => trait_object_single_trait_bound(&impl_.self_ty)
                .filter(|bound| bound.path.segments.len() == 1)
                .and_then(|bound| bound.path.segments.first())
                .map(|segment| segment.ident.clone()),
        }
    }

    let mut type_dispatch = decls
        .iter()
        .filter_map(|decl| {
            if let ForeignItem::Type(item) = decl {
                Some((item.ty.ident.clone(), Vec::new()))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();

    let mut kept_decls = Vec::with_capacity(decls.len());

    for decl in decls {
        let dispatch = match decl {
            ForeignItem::Impl(dispatch) => dispatch,
            decl => {
                kept_decls.push(decl);
                continue;
            }
        };
        let Some(ident) = declared_self_type(&dispatch.item) else {
            kept_decls.push(ForeignItem::Impl(dispatch));
            continue;
        };
        if !type_dispatch.contains_key(&ident) {
            kept_decls.push(ForeignItem::Impl(dispatch));
            continue;
        }

        type_dispatch.entry(ident).or_default().push(dispatch);
    }

    for decl in &mut kept_decls {
        let ForeignItem::Type(item) = decl else {
            continue;
        };

        item.self_impls = type_dispatch.remove(&item.ty.ident).unwrap_or_default();
    }

    kept_decls
}

fn synthesize_default_drop_impl(symbol_prefix: &LitStr, ty: &syn::ForeignItemType) -> ItemImpl {
    let (impl_generics, ty_generics, where_clause) = ty.generics.split_for_impl();

    let ident = &ty.ident;
    let symbol_name = LitStr::new(
        &format!(
            "{}__{}__{}__drop",
            symbol_prefix.value(),
            path_symbol_name(&parse_quote!(Drop), &Default::default()),
            type_symbol_name(&parse_quote!(#ident #ty_generics), &ty.generics),
        ),
        ident.span(),
    );

    parse_quote! {
        impl #impl_generics Drop for #ident #ty_generics #where_clause {
            #[symbol_name = #symbol_name]
            fn drop(&mut self) {}
        }
    }
}

fn ensure_symbol_name_on_fn(
    attrs: &mut Vec<Attribute>,
    symbol_prefix: &LitStr,
    fn_name: &syn::Ident,
    static_params: &[syn::Ident],
) {
    if !attrs.iter().any(is_symbol_name_attr) {
        let suffix = static_params_suffix(static_params);
        let symbol_name = LitStr::new(
            &format!("{}__{fn_name}{suffix}", symbol_prefix.value()),
            fn_name.span(),
        );

        attrs.push(parse_quote! {
            #[symbol_name = #symbol_name]
        });
    }
}

fn static_params_suffix(static_params: &[syn::Ident]) -> String {
    static_params
        .iter()
        .map(|param| format!("__{{{param}}}"))
        .collect()
}

fn ensure_symbol_name_on_static(
    attrs: &mut Vec<Attribute>,
    symbol_prefix: &LitStr,
    static_name: &syn::Ident,
) {
    ensure_symbol_name_on_fn(attrs, symbol_prefix, static_name, &[]);
}

fn ensure_symbol_name_on_impl_fn(
    attrs: &mut Vec<Attribute>,
    symbol_prefix: &LitStr,
    trait_: Option<&Path>,
    self_ty: &Type,
    generics: &syn::Generics,
    fn_name: &syn::Ident,
    static_params: &[syn::Ident],
) {
    if !attrs.iter().any(is_symbol_name_attr) {
        let default_symbol_name = trait_.map_or_else(
            || format!("{}__{}", type_symbol_name(self_ty, generics), fn_name),
            |trait_| {
                format!(
                    "{}__{}__{}",
                    path_symbol_name(trait_, generics),
                    type_symbol_name(self_ty, generics),
                    fn_name
                )
            },
        );
        let symbol_name = LitStr::new(
            &format!(
                "{}__{}{suffix}",
                symbol_prefix.value(),
                default_symbol_name,
                suffix = static_params_suffix(static_params),
            ),
            proc_macro2::Span::call_site(),
        );

        attrs.push(parse_quote! {
            #[symbol_name = #symbol_name]
        });
    }
}

#[cfg(test)]
mod dyn_self_tag_id_tests {
    use super::*;

    fn id_type(item: &ItemImpl) -> &Type {
        let syn::ImplItem::Fn(method) = &item.items[0] else {
            panic!("expected method");
        };
        let syn::FnArg::Typed(arg) = &method.sig.inputs[0] else {
            panic!("expected typed argument");
        };
        arg.ty.as_ref()
    }

    #[test]
    fn leaves_dyn_type_id_for_concrete_impl_self() {
        let mut item: ItemImpl = parse_quote! {
            impl<T> Trait for T {
                fn kita(tag: <dyn T>::TAG) {}
            }
        };

        normalize_dyn_self_tag_ids(&mut item);
        let expected = parse_quote!(<dyn T>::TAG);
        assert_eq!(id_type(&item), &expected);
    }

    #[test]
    fn rewrites_dyn_type_id_for_dyn_self_impl() {
        let mut item: ItemImpl = parse_quote! {
            impl<T> Trait for dyn T {
                fn kita(tag: <dyn T>::TAG) {}
            }
        };

        normalize_dyn_self_tag_ids(&mut item);
        let expected = parse_quote!(<dyn Self>::TAG);
        assert_eq!(id_type(&item), &expected);
    }
}
