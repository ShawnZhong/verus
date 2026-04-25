// Verus State Machines Macros — regular library shape (mirrors verus_builtin_macros).
//
// Was a `proc-macro = true` crate; now a regular rlib. Consumers (rust_verify
// on the host, verus-explorer's rustc-in-wasm) register `MACROS` with the
// patched `rustc_metadata::proc_macro_registry`, which swaps the empty-body
// `pub macro NAME` stubs below for the real Bang client at resolve time.
//
// Two compilation modes via the `stub_only` cfg — see the `verus_builtin_macros`
// header for the full rationale. Briefly:
//   * default — full crate (impl fns + `MACROS` + deps on syn/quote/etc.); used
//     by cargo for the host (rust_verify links MACROS) and the explorer's
//     wasm32 build (registers MACROS at runtime via `proc_macros::install`).
//   * `--cfg=stub_only` — only the `pub macro` shims; built by
//     `scripts/build-libs-sysroot.sh` against our staged wasm32 sysroot,
//     bundled as the rmeta vstd and user code link against in rustc-in-wasm.

#![cfg_attr(stub_only, no_std)]
#![cfg_attr(all(verus_keep_ghost, not(stub_only)), feature(proc_macro_expand))]
#![cfg_attr(not(stub_only), allow(internal_features))]
#![cfg_attr(not(stub_only), feature(proc_macro_internals))]
#![feature(decl_macro)]

macro_rules! host_only {
    ($($i:item)*) => { $(#[cfg(not(stub_only))] $i)* };
}

host_only! {
    extern crate proc_macro;

    use proc_macro::bridge::client::ProcMacro;

    #[macro_use]
    mod vstd_path;

    mod ast;
    mod case_macro;
    mod check_bind_stmts;
    mod check_birds_eye;
    mod concurrency_tokens;
    mod field_access_visitor;
    mod ident_visitor;
    mod inherent_safety_conditions;
    mod lemmas;
    mod parse_token_stream;
    mod parse_transition;
    mod safety_conditions;
    mod self_type_visitor;
    mod simplification;
    mod simplify_asserts;
    mod to_relation;
    mod to_token_stream;
    mod token_transition_checks;
    mod transitions;
    mod util;

    use case_macro::case_on;
    use lemmas::check_lemmas;
    use parse_token_stream::{ParseResult, parse_result_to_smir};
    use proc_macro::TokenStream;
    use to_token_stream::output_token_stream;
    use verus_syn::parse_macro_input;

    fn construct_state_machine(input: TokenStream, concurrent: bool) -> TokenStream {
        let pr: ParseResult = parse_macro_input!(input as ParseResult);

        let smir_res = parse_result_to_smir(pr, concurrent);
        let smir = match smir_res {
            Ok(smir) => smir,
            Err(err) => {
                return TokenStream::from(err.to_compile_error());
            }
        };

        match check_lemmas(&smir) {
            Ok(_) => {}
            Err(err) => {
                return TokenStream::from(err.to_compile_error());
            }
        }

        let token_stream = match output_token_stream(smir, concurrent) {
            Ok(ts) => ts,
            Err(err) => {
                return TokenStream::from(err.to_compile_error());
            }
        };

        token_stream.into()
    }

    pub fn state_machine(input: TokenStream) -> TokenStream {
        crate::vstd_path::set_is_vstd(false);
        crate::vstd_path::set_is_core(cfg_verify_core());
        construct_state_machine(input, false)
    }

    pub fn tokenized_state_machine(input: TokenStream) -> TokenStream {
        crate::vstd_path::set_is_vstd(false);
        crate::vstd_path::set_is_core(cfg_verify_core());
        construct_state_machine(input, true)
    }

    pub fn tokenized_state_machine_vstd(input: TokenStream) -> TokenStream {
        crate::vstd_path::set_is_vstd(true);
        crate::vstd_path::set_is_core(cfg_verify_core());
        construct_state_machine(input, true)
    }

    pub fn case_on_next(input: TokenStream) -> TokenStream {
        case_on(input, false, false)
    }

    pub fn case_on_next_strong(input: TokenStream) -> TokenStream {
        case_on(input, false, true)
    }

    pub fn case_on_init(input: TokenStream) -> TokenStream {
        case_on(input, true, false)
    }

    #[cfg(verus_keep_ghost)]
    pub(crate) fn cfg_verify_core() -> bool {
        static CFG_VERIFY_CORE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *CFG_VERIFY_CORE.get_or_init(|| {
            let ts: proc_macro::TokenStream = quote::quote! { ::core::cfg!(verus_verify_core) }.into();
            let bool_ts = match ts.expand_expr() {
                Ok(name) => name.to_string(),
                _ => {
                    panic!("cfg_verify_core call failed")
                }
            };
            match bool_ts.as_str() {
                "true" => true,
                "false" => false,
                _ => {
                    panic!("cfg_verify_core call failed")
                }
            }
        })
    }

    // Because 'expand_expr' is unstable, we need a different impl when `not(verus_keep_ghost)`.
    #[cfg(not(verus_keep_ghost))]
    pub(crate) fn cfg_verify_core() -> bool {
        false
    }

    pub static MACROS: &[ProcMacro] = &[
        ProcMacro::bang("state_machine", state_machine),
        ProcMacro::bang("tokenized_state_machine", tokenized_state_machine),
        ProcMacro::bang("tokenized_state_machine_vstd", tokenized_state_machine_vstd),
        ProcMacro::bang("case_on_next", case_on_next),
        ProcMacro::bang("case_on_next_strong", case_on_next_strong),
        ProcMacro::bang("case_on_init", case_on_init),
    ];
}

// `pub macro` shim stubs — see `verus_builtin_macros` for the resolve-time swap.
#[rustfmt::skip]
mod shim {
    pub macro state_machine($($t:tt)*) { }
    pub macro tokenized_state_machine($($t:tt)*) { }
    pub macro tokenized_state_machine_vstd($($t:tt)*) { }
    pub macro case_on_next($($t:tt)*) { }
    pub macro case_on_next_strong($($t:tt)*) { }
    pub macro case_on_init($($t:tt)*) { }
}

#[rustfmt::skip]
pub use shim::{
    state_machine, tokenized_state_machine, tokenized_state_machine_vstd,
    case_on_next, case_on_next_strong, case_on_init,
};
