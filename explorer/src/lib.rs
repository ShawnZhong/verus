// verus-explorer — browser-based exploration of Verus's internal representations.
//
// This crate compiles `vir` and `air` (as-is, via path dependencies) to wasm32
// and exposes a wasm-bindgen entry point that drives a real AIR query through
// `air::context::Context` end-to-end. SMT is routed through the wasm32
// `SmtProcess` shim added in air/src/smt_process.rs, which calls
// `globalThis.__verusExplorerZ3Eval` — installed by web/index.html on top of
// the self-hosted single-threaded Z3 wasm in web/z3/.
//
// What this proves: vir + air compile to wasm32, the SmtProcess abstraction is
// cleanly replaceable, and AIR-generated SMT round-trips through the browser
// Z3. Driving a hand-built `vir::ast::Krate` through `ast_to_sst → sst_to_air`
// is the next step.

use std::sync::Arc;

use air::ast::CommandX;
use air::context::{Context, SmtSolver, ValidityResult};
use air::messages::{AirMessageInterface, Reporter};
use air::parser::Parser;

/// One AIR query, plus the verdict we got back.
struct QueryRun {
    label: &'static str,
    air: String,
    /// "Valid" / "Invalid" / "TypeError(...)" / "UnexpectedOutput(...)" / "Canceled"
    verdict: String,
    /// True iff the assertion was proved (the negation was unsat).
    proved: bool,
}

/// AIR scripts — these are the real Verus AIR surface, not the SMT-LIB sent
/// to Z3. `air::context::Context` is what lowers them and runs check-sat.
const QUERIES: &[(&str, &str)] = &[
    (
        "commutativity of +",
        r#"
            (check-valid
                (declare-const x Int)
                (declare-const y Int)
                (assert (= (+ x y) (+ y x))))
        "#,
    ),
    (
        "false claim: x == 0 for all x",
        r#"
            (check-valid
                (declare-const x Int)
                (assert (= x 0)))
        "#,
    ),
];

fn run_one_query(label: &'static str, air_script: &str) -> QueryRun {
    let message_interface = Arc::new(AirMessageInterface {});
    let reporter = Reporter {};

    // The AIR parser expects a top-level list of commands; wrap in parens.
    let mut bytes: Vec<u8> = Vec::with_capacity(air_script.len() + 2);
    bytes.push(b'(');
    bytes.extend_from_slice(air_script.as_bytes());
    bytes.push(b')');
    let mut sise_parser = sise::Parser::new(&bytes);
    let node = sise::read_into_tree(&mut sise_parser).expect("AIR sise parse");
    let nodes = match node {
        sise::Node::List(nodes) => nodes,
        sise::Node::Atom(_) => panic!("expected list at AIR top level"),
    };
    let commands = Parser::new(message_interface.clone())
        .nodes_to_commands(&nodes)
        .expect("AIR parse");

    let mut ctx = Context::new(message_interface.clone(), SmtSolver::Z3);
    ctx.set_z3_param("air_recommended_options", "true");

    let mut verdict = String::from("Valid");
    let mut proved = true;
    for command in commands.iter() {
        let result = ctx.command(&*message_interface, &reporter, command, Default::default());
        match (&**command, &result) {
            (CommandX::CheckValid(_), ValidityResult::Valid(_)) => {
                verdict = "Valid".to_string();
                proved = true;
            }
            (CommandX::CheckValid(_), ValidityResult::Invalid(_, _, _)) => {
                verdict = "Invalid".to_string();
                proved = false;
            }
            (CommandX::CheckValid(_), ValidityResult::Canceled) => {
                verdict = "Canceled".to_string();
                proved = false;
            }
            (CommandX::CheckValid(_), ValidityResult::TypeError(e)) => {
                verdict = format!("TypeError({:?})", e);
                proved = false;
            }
            (CommandX::CheckValid(_), ValidityResult::UnexpectedOutput(s)) => {
                verdict = format!("UnexpectedOutput({})", s);
                proved = false;
            }
            (_, ValidityResult::TypeError(e)) => {
                verdict = format!("TypeError({:?})", e);
                proved = false;
            }
            _ => {}
        }
        if matches!(&**command, CommandX::CheckValid(_)) {
            ctx.finish_query();
        }
    }

    QueryRun {
        label,
        air: air_script.trim().to_string(),
        verdict,
        proved,
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm_entry {
    use wasm_bindgen::prelude::*;

    // JS-side sink for Rust panics — defined on globalThis by index.html, which
    // appends the message to #out so the user sees it in the page, not just the
    // devtools console. Without a hook, panics abort as "unreachable executed"
    // with no message.
    #[wasm_bindgen]
    extern "C" {
        fn reportPanic(msg: &str);
    }

    // Runs automatically when the wasm module is instantiated (wasm-bindgen
    // wires this up so JS's `await init()` triggers it exactly once).
    #[wasm_bindgen(start)]
    fn init() {
        std::panic::set_hook(Box::new(|info| reportPanic(&info.to_string())));
    }

    /// Result of one AIR query — surfaced to JS field-by-field (no JSON).
    #[wasm_bindgen]
    #[derive(Clone)]
    pub struct Query {
        #[wasm_bindgen(getter_with_clone)]
        pub label: String,
        #[wasm_bindgen(getter_with_clone)]
        pub air: String,
        #[wasm_bindgen(getter_with_clone)]
        pub verdict: String,
        pub proved: bool,
    }

    /// Aggregate result of `run`.
    #[wasm_bindgen]
    pub struct Output {
        pub all_expected: bool,
        #[wasm_bindgen(getter_with_clone)]
        pub queries: Vec<Query>,
    }

    #[wasm_bindgen]
    pub fn run() -> Output {
        // Per-query expectation: index 0 is provable, index 1 is not.
        let expectations = [true, false];

        let mut all_expected = true;
        let mut queries: Vec<Query> = Vec::new();
        for (i, (label, script)) in super::QUERIES.iter().enumerate() {
            let run = super::run_one_query(label, script);
            if run.proved != expectations[i] {
                all_expected = false;
            }
            queries.push(Query {
                label: run.label.to_string(),
                air: run.air,
                verdict: run.verdict,
                proved: run.proved,
            });
        }

        Output {
            all_expected,
            queries,
        }
    }
}
