// verus-explorer — browser-based exploration of Verus's internal representations.
//
// This crate compiles `vir` and `air` (as-is, via path dependencies) to wasm32
// and exposes a wasm-bindgen entry point that drives a minimal verification
// end-to-end. SMT is routed through the self-hosted single-threaded Z3 wasm
// in web/z3/ via `globalThis.__verusExplorerZ3Eval`.

use std::sync::Arc;

/// Construct a small AIR expression — exercises air::ast types.
fn air_smoke_test() -> String {
    use air::ast::{BinaryOp, Constant, ExprX};
    let one = Arc::new(ExprX::Const(Constant::Nat(Arc::new("1".into()))));
    let two = Arc::new(ExprX::Const(Constant::Nat(Arc::new("2".into()))));
    let sum = Arc::new(ExprX::Binary(BinaryOp::EuclideanMod, one, two));
    format!("AIR expr built: {:?}", sum)
}

/// Construct an empty VIR krate — exercises vir::ast types.
fn vir_smoke_test() -> String {
    use vir::ast::{Arch, ArchWordBits, KrateX};
    let krate = KrateX {
        functions: vec![],
        reveal_groups: vec![],
        datatypes: vec![],
        traits: vec![],
        trait_impls: vec![],
        assoc_type_impls: vec![],
        modules: vec![],
        external_fns: vec![],
        external_types: vec![],
        path_as_rust_names: vec![],
        arch: Arch { word_bits: ArchWordBits::Either32Or64 },
        opaque_types: vec![],
    };
    format!(
        "vir::KrateX built: {} fns, {} datatypes, {} traits",
        krate.functions.len(),
        krate.datatypes.len(),
        krate.traits.len(),
    )
}

/// SMT payload matching what Verus emits for `forall x: int :: x + 0 == x`,
/// modulo boxing sugar.
const POC_SMT_QUERY: &str = r#"
(set-option :auto_config false)
(set-option :smt.mbqi false)
(set-option :smt.case_split 3)

(declare-fun add_zero (Int) Int)
(assert (forall ((x Int))
  (! (= (add_zero x) x)
     :pattern ((add_zero x)))))

(push)
(declare-const x_sym Int)
(assert (! (not (= (add_zero x_sym) (+ x_sym 0)))
            :named assertion_add_zero_correct))
(check-sat)
(pop)

(push)
(declare-const y_sym Int)
(assert (! (not (= (add_zero y_sym) (+ y_sym 1)))
            :named assertion_off_by_one))
(check-sat)
(pop)
"#;

#[cfg(target_arch = "wasm32")]
mod wasm_entry {
    use wasm_bindgen::prelude::*;

    // The host (index.html) installs `globalThis.__verusExplorerZ3Eval` before
    // calling run_poc(); it points at the single-threaded Z3 wasm in web/z3/.
    #[wasm_bindgen(inline_js = r#"
        export async function z3Eval(script) {
            if (typeof globalThis.__verusExplorerZ3Eval !== 'function') {
                throw new Error('verus-explorer: globalThis.__verusExplorerZ3Eval not installed by host');
            }
            return await globalThis.__verusExplorerZ3Eval(script);
        }
    "#)]
    extern "C" {
        #[wasm_bindgen(js_name = z3Eval, catch)]
        async fn z3_eval(script: &str) -> Result<JsValue, JsValue>;
    }

    /// Result of `run_poc`. Fields are read directly from JS; no JSON.
    #[wasm_bindgen]
    pub struct PocResult {
        pub verified: bool,
        #[wasm_bindgen(getter_with_clone)] pub vir: String,
        #[wasm_bindgen(getter_with_clone)] pub air: String,
        #[wasm_bindgen(getter_with_clone)] pub smt_raw: String,
        #[wasm_bindgen(getter_with_clone)] pub smt_results: Vec<String>,
        #[wasm_bindgen(getter_with_clone)] pub error: Option<String>,
    }

    fn parse_check_sat_results(output: &str) -> Vec<String> {
        output
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| l == "sat" || l == "unsat" || l == "unknown")
            .collect()
    }

    #[wasm_bindgen]
    pub async fn run_poc() -> PocResult {
        let vir = super::vir_smoke_test();
        let air = super::air_smoke_test();

        let smt_raw = match z3_eval(super::POC_SMT_QUERY).await {
            Ok(v) => v.as_string().unwrap_or_default(),
            Err(e) => {
                let msg = e.as_string().unwrap_or_else(|| format!("{:?}", e));
                return PocResult {
                    verified: false,
                    vir, air,
                    smt_raw: String::new(),
                    smt_results: vec![],
                    error: Some(msg),
                };
            }
        };

        let smt_results = parse_check_sat_results(&smt_raw);
        let verified = smt_results.first().map(|s| s == "unsat").unwrap_or(false);

        PocResult {
            verified,
            vir, air, smt_raw, smt_results,
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_tests_run_natively() {
        assert!(!air_smoke_test().is_empty());
        assert!(!vir_smoke_test().is_empty());
    }

    #[test]
    fn vir_pipeline_runs_without_rustc() {
        use std::sync::{Arc, Mutex};
        use vir::ast::{Arch, ArchWordBits, KrateX};
        use vir::context::GlobalCtx;
        use vir::messages::Span;

        let krate = Arc::new(KrateX {
            functions: vec![],
            reveal_groups: vec![],
            datatypes: vec![],
            traits: vec![],
            trait_impls: vec![],
            assoc_type_impls: vec![],
            modules: vec![],
            external_fns: vec![],
            external_types: vec![],
            path_as_rust_names: vec![],
            arch: Arch { word_bits: ArchWordBits::Either32Or64 },
            opaque_types: vec![],
        });

        let no_span = Span {
            raw_span: Arc::new(()),
            id: 0,
            data: vec![],
            as_string: "<explorer-no-span>".into(),
        };

        let mut ctx = GlobalCtx::new(
            &krate,
            Arc::new("explorer_crate".to_string()),
            no_span,
            10.0,
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(None)),
            air::context::SmtSolver::Z3,
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .expect("GlobalCtx::new should succeed on empty krate");

        let simplified = vir::ast_simplify::simplify_krate(&mut ctx, &krate)
            .expect("simplify_krate should succeed on empty krate");

        assert_eq!(simplified.functions.len(), 0);
    }
}
