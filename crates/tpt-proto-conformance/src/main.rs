//! `tpt-conformance` — conformance testee / harness driver.
//!
//! Usage:
//! * `tpt-conformance testee`  — run the framed conformance protocol loop on
//!   stdin/stdout (interoperates with `conformance_test_runner`).
//! * `tpt-conformance run`     — run the built-in harness in-process and print a
//!   report (default when no subcommand is given). Pass `--json` for a
//!   machine-readable report. Exits non-zero if any case fails.
//! * `tpt-conformance cases`   — list the generated case names.
//!
//! The standalone `tpt-conformance-testee` binary speaks *only* the framed
//! protocol with no subcommand, so it can be passed directly to the reference
//! `conformance_test_runner` (see `conformance/run_conformance.sh`).

use std::io::{IsTerminal, Read, Write};

use tpt_proto_conformance::{run_all, Registry};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let registry = Registry::build();

    match args.get(1).map(String::as_str) {
        Some("testee") => {
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            // Lock the streams for buffered, blocking IO.
            let sin = stdin.lock();
            let sout = stdout.lock();
            if sin.is_terminal() {
                eprintln!(
                    "tpt-conformance testee: reading framed requests from stdin; \
                     connect via conformance_test_runner or pipe input."
                );
            }
            run_testee(sin, sout, &registry);
        }
        Some("cases") => {
            let report = run_all(&registry);
            for r in report.results() {
                println!("[{}] {}", status_letter(&r.status), r.name);
            }
        }
        Some("run") | None => {
            let json = args.iter().any(|a| a == "--json" || a == "-j");
            let report = run_all(&registry);
            if json {
                println!("{}", report.to_json());
            } else {
                print!("{}", report.render());
            }
            if report.failures() > 0 {
                std::process::exit(1);
            }
        }
        Some(other) => {
            eprintln!("unknown subcommand '{other}'; expected: testee | run | cases");
            std::process::exit(2);
        }
    }
}

fn run_testee<R: Read, W: Write>(reader: R, writer: W, registry: &Registry) {
    let mut reader = reader;
    let mut writer = writer;
    if let Err(e) = tpt_proto_conformance::testee::run_testee_loop(
        &mut reader,
        &mut writer,
        registry,
    ) {
        eprintln!("tpt-conformance testee error: {e}");
        std::process::exit(1);
    }
}

fn status_letter(s: &tpt_proto_conformance::Status) -> char {
    match s {
        tpt_proto_conformance::Status::Pass => 'P',
        tpt_proto_conformance::Status::Fail => 'F',
        tpt_proto_conformance::Status::Skip => 'S',
    }
}
