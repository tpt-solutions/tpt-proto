//! `tpt-proto-conformance` — conformance test harness and testee (§4.10, §19).
//!
//! Provides a Rust conformance testee that speaks the standard framed
//! `ConformanceRequest`/`ConformanceResponse` protocol, plus a self-contained
//! harness that exercises proto2/proto3/editions binaries and JSON, failure
//! behavior, unknown-field handling, and well-known-type behavior.

pub mod protocol;
pub mod runner;
pub mod schema;
pub mod testee;

pub use runner::{run_all, CaseResult, Report, Status};
pub use schema::Registry;
