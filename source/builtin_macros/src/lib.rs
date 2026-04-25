// Verus Builtin Macros — regular library shape.
//
// Was a `proc-macro = true` crate; now a regular rlib. Consumers (rust_verify
// on the host, verus-explorer's rustc-in-wasm) register `MACROS` with the
// patched `rustc_metadata::proc_macro_registry`, which swaps the empty-body
// `pub macro NAME` stubs below for the real Bang/Attr/Derive client at resolve
// time. Single crate now serves both paths — wasm32 can't emit `proc-macro`,
// and rustc-in-wasm has no dlopen, so the dylib path is unusable on either
// side.
//
// Two compilation modes via the `stub_only` cfg:
//   * default — full crate (impl fns, `MACROS`, deps on syn/quote/etc.);
//     used by cargo for both the host (rust_verify links MACROS) and the
//     explorer's wasm32 build (registers MACROS at runtime via
//     `proc_macros::install`).
//   * `--cfg=stub_only` — only the `pub macro` shims; built by
//     `scripts/build-libs-sysroot.sh` against our staged wasm32 sysroot,
//     bundled as the rmeta vstd and user code link against in rustc-in-wasm.
//
// Host-only items are wrapped in `host_only! { ... }` (a macro_rules that
// stamps `#[cfg(not(stub_only))]` onto each item) — keeps the cfg branching
// in one place rather than sprinkled across ~60 declarations.

// `stub_only` builds against our minimal staged sysroot (core+alloc, no std).
// All `std::*` paths are inside `host_only!`.
#![cfg_attr(stub_only, no_std)]
#![cfg_attr(
    all(verus_keep_ghost, not(stub_only)),
    feature(proc_macro_span),
    feature(proc_macro_tracked_env),
    feature(proc_macro_quote),
    feature(proc_macro_expand),
    feature(proc_macro_diagnostic)
)]
// `proc_macro::bridge` is `#[doc(hidden)] pub mod bridge` — gated as "internal
// to the compiler", which is exactly what we are when registering descriptors
// with `rustc_metadata::proc_macro_registry`.
#![cfg_attr(not(stub_only), allow(internal_features))]
#![cfg_attr(not(stub_only), feature(proc_macro_internals))]
// `pub macro NAME` decl_macro stubs (see file header).
#![feature(decl_macro)]

macro_rules! host_only {
    ($($i:item)*) => { $(#[cfg(not(stub_only))] $i)* };
}

host_only! {
    // `proc_macro` is part of the host sysroot but isn't in the prelude for
    // non-proc-macro crates.
    extern crate proc_macro;

    use std::sync::OnceLock;

    use proc_macro::bridge::client::ProcMacro;

    #[macro_use]
    mod syntax;
    mod atomic_ghost;
    mod attr_block_trait;
    mod attr_rewrite;
    mod calc_macro;
    mod contrib;
    mod enum_synthesize;
    mod fndecl;
    mod is_variant;
    mod rustdoc;
    mod struct_decl_inv;
    mod structural;
    mod syntax_trait;
    mod topological_sort;
    mod unerased_proxies;

    // -----------------------------------------------------------------------
    // Internal helpers (mirror what `decl_derive!`/`decl_attribute!` would
    // expand to for the `Structural` / `is_variant` families).
    // -----------------------------------------------------------------------

    fn derive_with<F>(input: proc_macro::TokenStream, inner: F) -> proc_macro::TokenStream
    where
        F: FnOnce(synstructure::Structure) -> proc_macro2::TokenStream,
    {
        let parsed: syn::DeriveInput = match syn::parse(input) {
            Ok(p) => p,
            Err(e) => return e.to_compile_error().into(),
        };
        match synstructure::Structure::try_new(&parsed) {
            Ok(s) => inner(s).into(),
            Err(e) => e.to_compile_error().into(),
        }
    }

    fn attr_with_structure<F>(
        attr: proc_macro::TokenStream,
        item: proc_macro::TokenStream,
        inner: F,
    ) -> proc_macro::TokenStream
    where
        F: FnOnce(proc_macro2::TokenStream, synstructure::Structure) -> proc_macro2::TokenStream,
    {
        let parsed: syn::DeriveInput = match syn::parse(item) {
            Ok(p) => p,
            Err(e) => return e.to_compile_error().into(),
        };
        match synstructure::Structure::try_new(&parsed) {
            Ok(s) => inner(attr.into(), s).into(),
            Err(e) => e.to_compile_error().into(),
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(crate) enum EraseGhost {
        /// keep all ghost code
        Keep,
        /// erase ghost code, but leave ghost stubs
        Erase,
        /// erase all ghost code
        EraseAll,
    }

    impl EraseGhost {
        pub(crate) fn keep(&self) -> bool {
            matches!(self, EraseGhost::Keep)
        }

        pub(crate) fn erase(&self) -> bool {
            !self.keep()
        }

        pub(crate) fn erase_all(&self) -> bool {
            matches!(self, EraseGhost::EraseAll)
        }
    }

    // `expand_expr` requires the bridge to be live. It is when our impl fns
    // are invoked through the proc-macro bridge (BangProcMacro::expand /
    // AttrProcMacro::expand / DeriveProcMacro::expand), which is the only way
    // these are reached after the registry override fires.
    #[cfg(verus_keep_ghost)]
    pub(crate) fn cfg_erase() -> EraseGhost {
        let ts: proc_macro::TokenStream =
            quote::quote! { ::core::cfg!(verus_keep_ghost_body) }.into();
        let ts_stubs: proc_macro::TokenStream =
            quote::quote! { ::core::cfg!(verus_keep_ghost) }.into();
        let (bool_ts, bool_ts_stubs) = match (ts.expand_expr(), ts_stubs.expand_expr()) {
            (Ok(name), Ok(name_stubs)) => (name.to_string(), name_stubs.to_string()),
            _ => panic!("cfg_erase call failed"),
        };
        match (bool_ts.as_str(), bool_ts_stubs.as_str()) {
            ("true", "true" | "false") => EraseGhost::Keep,
            ("false", "true") => EraseGhost::Erase,
            ("false", "false") => EraseGhost::EraseAll,
            _ => panic!("cfg_erase call failed"),
        }
    }

    #[cfg(not(verus_keep_ghost))]
    pub(crate) fn cfg_erase() -> EraseGhost {
        EraseGhost::EraseAll
    }

    #[derive(Clone, Copy)]
    pub(crate) enum VstdKind {
        /// The current crate is vstd.
        IsVstd,
        /// There is no vstd (only verus_builtin). Really only used for testing.
        NoVstd,
        /// Imports the vstd crate like usual.
        Imported,
        /// Embed vstd and verus_builtin as modules, necessary for verifying the `core` library.
        IsCore,
        /// For other crates in stdlib verification that import core
        ImportedViaCore,
    }

    pub(crate) fn vstd_kind() -> VstdKind {
        static VSTD_KIND: OnceLock<VstdKind> = OnceLock::new();
        *VSTD_KIND.get_or_init(|| {
            if let Ok(s) = std::env::var("VSTD_KIND") {
                return match s.as_str() {
                    "IsVstd" => VstdKind::IsVstd,
                    "NoVstd" => VstdKind::NoVstd,
                    "Imported" => VstdKind::Imported,
                    "IsCore" => VstdKind::IsCore,
                    "ImportedViaCore" => VstdKind::ImportedViaCore,
                    _ => panic!("The environment variable VSTD_KIND was set but its value ('{:}') is invalid. Allowed values are 'IsVstd', 'NoVstd', 'Imported', 'IsCore', and 'ImportedViaCore'", s),
                };
            }
            if std::env::var("CARGO_PKG_NAME").map_or(false, |s| s == "vstd") {
                return VstdKind::IsVstd;
            }
            if cfg_verify_core() {
                return VstdKind::IsCore;
            }
            if cfg_no_vstd() {
                return VstdKind::NoVstd;
            }
            VstdKind::Imported
        })
    }

    #[cfg(verus_keep_ghost)]
    pub(crate) fn cfg_verify_core() -> bool {
        static CFG_VERIFY_CORE: OnceLock<bool> = OnceLock::new();
        *CFG_VERIFY_CORE.get_or_init(|| {
            let ts: proc_macro::TokenStream =
                quote::quote! { ::core::cfg!(verus_verify_core) }.into();
            match ts.expand_expr().map(|t| t.to_string()) {
                Ok(s) if s == "true" => true,
                Ok(s) if s == "false" => false,
                _ => panic!("cfg_verify_core call failed"),
            }
        })
    }

    #[cfg(not(verus_keep_ghost))]
    pub(crate) fn cfg_verify_core() -> bool {
        false
    }

    #[cfg(verus_keep_ghost)]
    fn cfg_no_vstd() -> bool {
        static CFG_NO_VSTD: OnceLock<bool> = OnceLock::new();
        *CFG_NO_VSTD.get_or_init(|| {
            let ts: proc_macro::TokenStream =
                quote::quote! { ::core::cfg!(verus_no_vstd) }.into();
            match ts.expand_expr().map(|t| t.to_string()) {
                Ok(s) if s == "true" => true,
                Ok(s) if s == "false" => false,
                _ => panic!("cfg_no_vstd call failed"),
            }
        })
    }

    #[cfg(not(verus_keep_ghost))]
    fn cfg_no_vstd() -> bool {
        false
    }

    // -----------------------------------------------------------------------
    // Real impl fns (proc_macro::TokenStream in/out, called via the bridge
    // once the registry has swapped the stub kinds). One per macro name in
    // `MACROS`.
    // -----------------------------------------------------------------------

    pub fn derive_structural(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        derive_with(input, structural::derive_structural)
    }

    pub fn derive_structural_eq(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        derive_with(input, structural::derive_structural_eq)
    }

    pub fn attribute_is_variant(
        attr: proc_macro::TokenStream,
        item: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
        attr_with_structure(attr, item, is_variant::attribute_is_variant)
    }

    pub fn attribute_is_variant_no_deprecation_warning(
        attr: proc_macro::TokenStream,
        item: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
        attr_with_structure(attr, item, is_variant::attribute_is_variant_no_deprecation_warning)
    }

    pub fn verus_enum_synthesize(
        attr: proc_macro::TokenStream,
        input: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
        enum_synthesize::attribute_verus_enum_synthesize(&cfg_erase(), attr, input)
    }

    pub fn fndecl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        proc_macro::TokenStream::from(fndecl::fndecl(proc_macro2::TokenStream::from(input)))
    }

    pub fn verus_keep_ghost(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        syntax::rewrite_items(input, EraseGhost::Keep, true)
    }

    pub fn verus_erase_ghost(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        syntax::rewrite_items(input, EraseGhost::Erase, true)
    }

    pub fn verus(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        syntax::rewrite_items(input, cfg_erase(), true)
    }

    pub fn verus_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        syntax::rewrite_impl_items(input, cfg_erase(), true, false)
    }

    pub fn verus_trait_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        syntax::rewrite_impl_items(input, cfg_erase(), true, true)
    }

    pub fn verus_proof_expr(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        syntax::rewrite_expr(EraseGhost::Keep, true, input)
    }

    pub fn verus_exec_expr_keep_ghost(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        syntax::rewrite_expr(EraseGhost::Keep, false, input)
    }

    pub fn verus_exec_expr_erase_ghost(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        syntax::rewrite_expr(EraseGhost::Keep, false, input)
    }

    pub fn verus_exec_expr(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        syntax::rewrite_expr(cfg_erase(), false, input)
    }

    pub fn verus_proof_macro_exprs(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        syntax::proof_macro_exprs(EraseGhost::Keep, true, input)
    }

    pub fn verus_exec_macro_exprs(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        syntax::proof_macro_exprs(cfg_erase(), false, input)
    }

    pub fn verus_exec_inv_macro_exprs(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        syntax::inv_macro_exprs(cfg_erase(), false, input)
    }

    pub fn verus_ghost_inv_macro_exprs(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        syntax::inv_macro_exprs(cfg_erase(), true, input)
    }

    pub fn verus_proof_macro_explicit_exprs(
        input: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
        syntax::proof_macro_explicit_exprs(EraseGhost::Keep, true, input)
    }

    pub fn struct_with_invariants(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        struct_decl_inv::struct_decl_inv(input)
    }

    pub fn atomic_with_ghost_helper(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        atomic_ghost::atomic_ghost(input)
    }

    pub fn calc_proc_macro(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        calc_macro::calc_macro(input)
    }

    pub fn verus_verify(
        args: proc_macro::TokenStream,
        input: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
        attr_rewrite::rewrite_verus_attribute(&cfg_erase(), args, input)
    }

    pub fn verus_spec(
        attr: proc_macro::TokenStream,
        input: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
        attr_rewrite::rewrite_verus_spec(cfg_erase(), attr, input)
    }

    pub fn proof_with(_input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        proc_macro::TokenStream::new()
    }

    pub fn proof(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        attr_rewrite::proof_rewrite(cfg_erase(), input.into())
    }

    pub fn proof_decl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        let erase = cfg_erase();
        if erase.keep() {
            syntax::rewrite_proof_decl(erase, input)
        } else {
            proc_macro::TokenStream::new()
        }
    }

    pub fn set_build(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        contrib::set_build::set_build(input, false)
    }

    pub fn set_build_debug(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        contrib::set_build::set_build(input, true)
    }

    pub fn exec_spec_verified(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        contrib::exec_spec::exec_spec(input, false)
    }

    pub fn exec_spec_unverified(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
        contrib::exec_spec::exec_spec(input, true)
    }

    pub fn auto_spec(
        _args: proc_macro::TokenStream,
        input: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
        // All the work is done in the preprocessing; this just double-checks name resolution.
        input
    }

    pub fn make_spec_type(
        attr: proc_macro::TokenStream,
        input: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
        contrib::spec_derive::make_spec_type(attr, input)
    }

    pub fn self_view(
        attr: proc_macro::TokenStream,
        input: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
        contrib::spec_derive::self_view(attr, input)
    }

    // -----------------------------------------------------------------------
    // Descriptor slice consumed by `rustc_metadata::proc_macro_registry::register`
    // in both rust_verify and verus-explorer. Order is irrelevant — lookups
    // are keyed by `(crate_name, macro_name)` (registry path #2 in
    // `proc_macro_registry.rs`).
    // -----------------------------------------------------------------------
    pub static MACROS: &[ProcMacro] = &[
        ProcMacro::custom_derive("Structural", &[], derive_structural),
        ProcMacro::custom_derive("StructuralEq", &[], derive_structural_eq),
        ProcMacro::attr("is_variant", attribute_is_variant),
        ProcMacro::attr("is_variant_no_deprecation_warning", attribute_is_variant_no_deprecation_warning),
        ProcMacro::attr("verus_enum_synthesize", verus_enum_synthesize),
        ProcMacro::bang("fndecl", fndecl),
        ProcMacro::bang("verus_keep_ghost", verus_keep_ghost),
        ProcMacro::bang("verus_erase_ghost", verus_erase_ghost),
        ProcMacro::bang("verus", verus),
        ProcMacro::bang("verus_impl", verus_impl),
        ProcMacro::bang("verus_trait_impl", verus_trait_impl),
        ProcMacro::bang("verus_proof_expr", verus_proof_expr),
        ProcMacro::bang("verus_exec_expr_keep_ghost", verus_exec_expr_keep_ghost),
        ProcMacro::bang("verus_exec_expr_erase_ghost", verus_exec_expr_erase_ghost),
        ProcMacro::bang("verus_exec_expr", verus_exec_expr),
        ProcMacro::bang("verus_proof_macro_exprs", verus_proof_macro_exprs),
        ProcMacro::bang("verus_exec_macro_exprs", verus_exec_macro_exprs),
        ProcMacro::bang("verus_exec_inv_macro_exprs", verus_exec_inv_macro_exprs),
        ProcMacro::bang("verus_ghost_inv_macro_exprs", verus_ghost_inv_macro_exprs),
        ProcMacro::bang("verus_proof_macro_explicit_exprs", verus_proof_macro_explicit_exprs),
        ProcMacro::bang("struct_with_invariants", struct_with_invariants),
        ProcMacro::bang("atomic_with_ghost_helper", atomic_with_ghost_helper),
        ProcMacro::bang("calc_proc_macro", calc_proc_macro),
        ProcMacro::attr("verus_verify", verus_verify),
        ProcMacro::attr("verus_spec", verus_spec),
        ProcMacro::bang("proof_with", proof_with),
        ProcMacro::bang("proof", proof),
        ProcMacro::bang("proof_decl", proof_decl),
        ProcMacro::bang("set_build", set_build),
        ProcMacro::bang("set_build_debug", set_build_debug),
        ProcMacro::bang("exec_spec_verified", exec_spec_verified),
        ProcMacro::bang("exec_spec_unverified", exec_spec_unverified),
        ProcMacro::attr("auto_spec", auto_spec),
        ProcMacro::attr("make_spec_type", make_spec_type),
        ProcMacro::attr("self_view", self_view),
    ];
}

// ---------------------------------------------------------------------------
// `pub macro` shim stubs. These exist purely so name resolution in downstream
// crates (vstd, user code) finds a macro by each user-facing name; the
// registry override in `rustc_resolve::build_reduced_graph::get_macro_by_def_id`
// swaps each stub's `SyntaxExtensionKind` for the matching client below at
// expansion time. Shapes deliberately ignore arguments — the empty body never
// runs.
// ---------------------------------------------------------------------------

#[rustfmt::skip]
mod shim {
    pub macro Structural($($t:tt)*) { }
    pub macro StructuralEq($($t:tt)*) { }
    pub macro is_variant($($t:tt)*) { }
    pub macro is_variant_no_deprecation_warning($($t:tt)*) { }
    pub macro verus_enum_synthesize($($t:tt)*) { }
    pub macro fndecl($($t:tt)*) { }
    pub macro verus_keep_ghost($($t:tt)*) { }
    pub macro verus_erase_ghost($($t:tt)*) { }
    pub macro verus($($t:tt)*) { }
    pub macro verus_impl($($t:tt)*) { }
    pub macro verus_trait_impl($($t:tt)*) { }
    pub macro verus_proof_expr($($t:tt)*) { }
    pub macro verus_exec_expr_keep_ghost($($t:tt)*) { }
    pub macro verus_exec_expr_erase_ghost($($t:tt)*) { }
    pub macro verus_exec_expr($($t:tt)*) { }
    pub macro verus_proof_macro_exprs($($t:tt)*) { }
    pub macro verus_exec_macro_exprs($($t:tt)*) { }
    pub macro verus_exec_inv_macro_exprs($($t:tt)*) { }
    pub macro verus_ghost_inv_macro_exprs($($t:tt)*) { }
    pub macro verus_proof_macro_explicit_exprs($($t:tt)*) { }
    pub macro struct_with_invariants($($t:tt)*) { }
    pub macro atomic_with_ghost_helper($($t:tt)*) { }
    pub macro calc_proc_macro($($t:tt)*) { }
    pub macro verus_verify($($t:tt)*) { }
    pub macro verus_spec($($t:tt)*) { }
    pub macro proof_with($($t:tt)*) { }
    pub macro proof($($t:tt)*) { }
    pub macro proof_decl($($t:tt)*) { }
    pub macro set_build($($t:tt)*) { }
    pub macro set_build_debug($($t:tt)*) { }
    pub macro exec_spec_verified($($t:tt)*) { }
    pub macro exec_spec_unverified($($t:tt)*) { }
    pub macro auto_spec($($t:tt)*) { }
    pub macro make_spec_type($($t:tt)*) { }
    pub macro self_view($($t:tt)*) { }
}

#[rustfmt::skip]
pub use shim::{
    Structural, StructuralEq, is_variant, is_variant_no_deprecation_warning, verus_enum_synthesize,
    fndecl, verus_keep_ghost, verus_erase_ghost, verus, verus_impl, verus_trait_impl,
    verus_proof_expr, verus_exec_expr_keep_ghost, verus_exec_expr_erase_ghost, verus_exec_expr,
    verus_proof_macro_exprs, verus_exec_macro_exprs, verus_exec_inv_macro_exprs,
    verus_ghost_inv_macro_exprs, verus_proof_macro_explicit_exprs, struct_with_invariants,
    atomic_with_ghost_helper, calc_proc_macro, verus_verify, verus_spec, proof_with, proof,
    proof_decl, set_build, set_build_debug, exec_spec_verified, exec_spec_unverified, auto_spec,
    make_spec_type, self_view,
};
