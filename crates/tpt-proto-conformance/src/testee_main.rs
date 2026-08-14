//! `tpt-conformance-testee` — standalone conformance testee.
//!
//! Unlike the multi-mode `tpt-conformance` driver, this binary speaks *only* the
//! standard framed `ConformanceRequest`/`ConformanceResponse` protocol on
//! stdin/stdout. It is the binary `conformance_test_runner` invokes when you
//! pass it the path to `tpt-conformance-testee` with no arguments:
//!
//! ```text
//! conformance_test_runner --enforce_recommended \
//!     --failure_list conformance/failure_list.txt \
//!     target/debug/tpt-conformance-testee
//! ```
//!
//! The loop: read a 4-byte LE length, decode `ConformanceRequest`, process it,
//! encode `ConformanceResponse`, write a 4-byte LE length + frame. Nothing else
//! is ever written to stdout, so it interoperates with the reference runner.

use std::io::{IsTerminal, Write};

use tpt_proto_conformance::schema::Registry;
use tpt_proto_conformance::testee::run_testee_loop;

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut sin = stdin.lock();
    let mut sout = stdout.lock();

    if sin.is_terminal() {
        eprintln!(
            "tpt-conformance-testee: reading framed conformance requests from stdin.\n\
             \x20   Normally invoked by `conformance_test_runner`. To drive the built-in\n\
             \x20   harness instead, run `tpt-conformance run`."
        );
    }

    let registry = Registry::build();
    if let Err(e) = run_testee_loop(&mut sin, &mut sout, &registry) {
        // Only stderr is used for diagnostics; stdout stays protocol-clean.
        eprintln!("tpt-conformance-testee error: {e}");
        let _ = sout.flush();
        std::process::exit(1);
    }
}
