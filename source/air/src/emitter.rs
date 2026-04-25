use crate::ast::{Decl, Expr, Ident, Query};
use crate::context::SmtSolver;
use crate::printer::{NodeWriter, Printer, macro_push_node};
use crate::{node, nodes};
use sise::Node;
use std::io::Write;

pub(crate) struct Emitter {
    /// AIR/SMT -> Node printer
    printer: Printer,
    /// Node -> string writer
    node_writer: NodeWriter,
    /// buffer for data to be sent across pipe to Z3 process
    pipe_buffer: Option<Vec<u8>>,
    /// commands-only log (replayable `.smt2`)
    log: Option<Box<dyn std::io::Write>>,
    /// commands-plus-responses transcript (verus-explorer: also where
    /// section markers + response blocks land for the unified UI tab)
    transcript_log: Option<Box<dyn std::io::Write>>,
    /// string of space characters representing current indentation level
    current_indent: String,
}

impl Emitter {
    pub fn new(
        message_interface: std::sync::Arc<dyn crate::messages::MessageInterface>,
        use_pipe: bool,
        print_as_smt: bool,
        writer: Option<Box<dyn std::io::Write>>,
        solver: SmtSolver,
    ) -> Self {
        let pipe_buffer = if use_pipe { Some(Vec::new()) } else { None };
        Emitter {
            printer: Printer::new(message_interface, print_as_smt, solver),
            node_writer: NodeWriter::new(),
            pipe_buffer,
            log: writer,
            transcript_log: None,
            current_indent: "".to_string(),
        }
    }

    pub fn set_log(&mut self, writer: Option<Box<dyn std::io::Write>>) {
        self.log = writer;
    }

    pub fn set_transcript_log(&mut self, writer: Option<Box<dyn std::io::Write>>) {
        self.transcript_log = writer;
    }

    fn is_none(&self) -> bool {
        self.pipe_buffer.is_none() && self.log.is_none() && self.transcript_log.is_none()
    }

    /// Return all the data in pipe_buffer, and reset pipe_buffer to Some empty vector
    pub fn take_pipe_data(&mut self) -> Vec<u8> {
        let data = self.pipe_buffer.take().expect("use_pipe must be set to true to take pipe");
        self.pipe_buffer = Some(Vec::new());
        data
    }

    pub fn indent(&mut self) {
        if self.log.is_some() || self.transcript_log.is_some() {
            self.current_indent = self.current_indent.clone() + " ";
        }
    }

    pub fn unindent(&mut self) {
        if self.log.is_some() || self.transcript_log.is_some() {
            self.current_indent = self.current_indent[1..].to_string();
        }
    }

    pub fn blank_line(&mut self) {
        if let Some(w) = &mut self.log {
            writeln!(w, "").unwrap();
            w.flush().unwrap();
        }
        if let Some(w) = &mut self.transcript_log {
            writeln!(w, "").unwrap();
            w.flush().unwrap();
        }
    }

    pub fn comment(&mut self, s: &str) {
        if let Some(w) = &mut self.pipe_buffer {
            writeln!(w, "{};; {}", self.current_indent, s).unwrap();
            w.flush().unwrap();
        }
        if let Some(w) = &mut self.log {
            writeln!(w, "{};; {}", self.current_indent, s).unwrap();
            w.flush().unwrap();
        }
        if let Some(w) = &mut self.transcript_log {
            writeln!(w, "{};; {}", self.current_indent, s).unwrap();
            w.flush().unwrap();
        }
    }

    // verus-explorer: section open marker — `;;<marker> <label>`.
    // Writes to `log` and `transcript_log`, never to `pipe_buffer`.
    // The marker char lets downstream consumers (the browser's fold
    // scanner in `public/app.js`) tell section-opening banners apart
    // from plain comments without enumerating op-kind label strings.
    // Markers mirror the CM6 fold-gutter glyphs:
    //   `>` — open a section that auto-folds (▸ collapsed)
    //   `v` — open a section that stays expanded but foldable (▾)
    // Each open must be paired with a `section_close()` (see below).
    // Sections nest — an open inside another open creates a child.
    //
    // `pipe_buffer` is intentionally skipped: pipe contents are
    // what get flushed to Z3, and a `;;` comment line in the middle
    // of a `(check-sat)` batch is harmless but adds noise to the
    // wire. The downstream UI reads from the writer side anyway.
    pub fn section(&mut self, marker: char, label: &str) {
        // Section markers are structural and always land at column
        // 0 — the JS scanner's line-prefix checks (`line[2]` for
        // the marker char) don't account for `current_indent`
        // whitespace, and an open/close mismatch there misaligns
        // the stack-based fold builder.
        if let Some(w) = &mut self.log {
            writeln!(w, ";;{} {}", marker, label).unwrap();
            w.flush().unwrap();
        }
        if let Some(w) = &mut self.transcript_log {
            writeln!(w, ";;{} {}", marker, label).unwrap();
            w.flush().unwrap();
        }
    }

    // verus-explorer: section close marker — `;;<`. Closes the
    // innermost open section. Writes to `log` and `transcript_log`,
    // never to `pipe_buffer`; also skips `current_indent` — see
    // `section` above.
    pub fn section_close(&mut self) {
        if let Some(w) = &mut self.log {
            writeln!(w, ";;<").unwrap();
            w.flush().unwrap();
        }
        if let Some(w) = &mut self.transcript_log {
            writeln!(w, ";;<").unwrap();
            w.flush().unwrap();
        }
    }

    pub fn log_node(&mut self, node: &Node) {
        if let Some(w) = &mut self.pipe_buffer {
            writeln!(w, "{}", self.node_writer.node_to_string_indent(&self.current_indent, &node))
                .unwrap();
            w.flush().unwrap();
        }
        if let Some(w) = &mut self.log {
            writeln!(
                w,
                "{}{}",
                self.current_indent,
                self.node_writer.node_to_string_indent(&self.current_indent, &node)
            )
            .unwrap();
            w.flush().unwrap();
        }
        if let Some(w) = &mut self.transcript_log {
            writeln!(
                w,
                "{}{}",
                self.current_indent,
                self.node_writer.node_to_string_indent(&self.current_indent, &node)
            )
            .unwrap();
            w.flush().unwrap();
        }
    }

    // verus-explorer: write the response half of a Z3 round trip —
    // `;;> response <ms>ms\n<line>\n…\n;;<` — to `transcript_log`
    // only. Auto-folded by default so large model dumps / stats
    // replies fold independently. Commands flow through `log_node`
    // at emission time; this method covers the matching response
    // block. Skipping `log` keeps the commands-only `.smt2`
    // output replayable in `z3 -in`.
    pub fn log_response_block(&mut self, elapsed_ms: f64, lines: &[String]) {
        if let Some(w) = &mut self.transcript_log {
            writeln!(w, ";;> response {:.2}ms", elapsed_ms).unwrap();
            for line in lines {
                writeln!(w, "{}", line).unwrap();
            }
            writeln!(w, ";;<").unwrap();
            w.flush().unwrap();
        }
    }

    pub fn log_set_option(&mut self, option: &str, value: &str) {
        if !self.is_none() {
            self.log_node(&node!(
                (set-option {Node::Atom(":".to_owned() + option)} {Node::Atom(value.to_string())})
            ));
        }
    }

    pub fn log_get_info(&mut self, param: &str) {
        if !self.is_none() {
            self.log_node(&node!(
                (get-info {Node::Atom(format!(":{}", param))})
            ));
        }
    }

    pub fn log_push(&mut self) {
        if !self.is_none() {
            self.log_node(&nodes!(push));
            self.indent();
        }
    }

    pub fn log_pop(&mut self) {
        if !self.is_none() {
            self.unindent();
            self.log_node(&nodes!(pop));
        }
    }

    /*
    pub fn log_function_decl(&mut self, x: &Ident, typs: &[Typ], typ: &Typ) {
        if let Some(_) = self.log {
            self.log_node(&function_decl_to_node(x, typs, typ));
        }
    }
    */

    pub fn log_decl(&mut self, decl: &Decl) {
        if !self.is_none() {
            self.log_node(&self.printer.decl_to_node(decl));
        }
    }

    pub fn log_assert(&mut self, named: &Option<Ident>, expr: &Expr) {
        if !self.is_none() {
            self.log_node(&
                if let Some(named) = named {
                    nodes!(assert ({Node::Atom("!".to_string())} {self.printer.expr_to_node(expr)} {Node::Atom(":named".to_string())} {Node::Atom((**named).clone())}))
                } else {
                    nodes!(assert {self.printer.expr_to_node(expr)})
                })
        }
    }

    pub fn log_word(&mut self, s: &str) {
        if !self.is_none() {
            self.log_node(&Node::List(vec![Node::Atom(s.to_string())]));
        }
    }

    pub fn log_query(&mut self, query: &Query) {
        if !self.is_none() {
            self.log_node(&self.printer.query_to_node(query));
        }
    }

    pub fn log_eval(&mut self, expr: Node) {
        if !self.is_none() {
            self.log_node(&nodes!(eval { expr }));
        }
    }
}
