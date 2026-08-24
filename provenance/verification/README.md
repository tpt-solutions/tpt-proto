# Formal verification spike: `decoder-depth-guard.telos`

This directory holds a small proof-of-concept applying formal verification
(via the sibling [`tpt-telos`](https://github.com/tpt-solutions/tpt-telos)
project) to a real tpt-proto bug, rather than relying solely on fuzzing/manual
audit to find this class of issue. See `todo.md`'s "Phase 17 follow-up" for the
bug this verifies and the resulting fix.

## The bug

`Reader::nested` (`crates/tpt-proto-core/src/reader.rs`) is tpt-proto's correct
pattern for crossing into a nested length-delimited message: the child's depth
must be exactly `parent.depth + 1`, and only when that doesn't exceed
`DecoderLimits::max_depth`. Two functions in `crates/tpt-proto-core/src/packed.rs`
(`read_packed_fixed32`, `read_packed_fixed64`) and the packed-scalar branch of
`tpt-proto-reflect/src/lib.rs`'s `decode_one` instead built a **fresh**
`Reader::new(body)`, silently resetting depth to 0 (and reverting to
`DecoderLimits::default()`, discarding the caller's configured limits
entirely) — defeating `max_depth`'s protection against stack-exhaustion DoS via
deeply nested / self-referential schemas.

## `decoder-depth-guard.telos`

Models the contract `Reader::nested` upholds as a `tpt-telos` module: an
`invariant Nesting { depth >= 0, max_depth >= 0, depth <= max_depth }`, a
correct `enter` function (`ensures n.depth == old(n.depth) + 1`, matching
`Reader::nested`), and an `enter_buggy` function with the same `ensures`
clause but a body that resets depth to 0 (matching the bug).

### Running it

`tpt-telos` isn't vendored into this repo — check it out as a sibling and run
`telos verify` against this file. As of 2026-08-24, `tpt-telos`'s workspace
`Cargo.toml` unconditionally includes `crates/tpt-telos-uir-bridge`, which has
a path dependency on a `tpt-uir` sibling repo; if you don't have that checked
out, comment out that one line in `tpt-telos`'s `Cargo.toml` `members` list
locally (don't commit it there — it's just a local unblock) before running:

```sh
cd ../tpt-telos   # or wherever your tpt-telos checkout lives
cargo run -p tpt-telos -- verify ../tpt-proto/provenance/verification/decoder-depth-guard.telos
```

### Actual output (2026-08-24)

```
Verifying decoder-depth-guard.telos

  function enter:
    [PASS] ensures ensures: n.depth == old(n.depth) + 1
    [PASS] invariant Nesting maintained: n.depth >= 0
    [PASS] invariant Nesting maintained: n.max_depth >= 0
    [PASS] invariant Nesting maintained: n.depth <= n.max_depth
    => PASS

  function enter_buggy:
    [FAIL] ensures ensures: n.depth == old(n.depth) + 1
      counterexample: {n.max_depth'=1, n.depth=0, n.max_depth=1, n.depth'=0}
    [PASS] invariant Nesting maintained: n.depth >= 0
    [PASS] invariant Nesting maintained: n.max_depth >= 0
    [PASS] invariant Nesting maintained: n.depth <= n.max_depth
    => FAIL

RESULT: verification failed (see FAIL above).
```

`enter` proves the correct contract holds; `enter_buggy` fails with a concrete
counterexample — the same solver that proves the fix correct also mechanically
rejects the exact bug shape that was live in `packed.rs`.

## Caveat

`tpt-telos` verifies over unbounded (QF_LRA) integer arithmetic — it proves
`depth <= max_depth` as a pure logical fact, but doesn't model machine
integer width/overflow (the real `depth` field is a `u32`). It's a tool for
catching *logic* bugs like this one (the wrong value being propagated), not a
substitute for the wire-format decoder's own bounds/overflow checks
(`DecoderLimits`, checked arithmetic — see `docs/security.md`).

## The fix

`crates/tpt-proto-core/src/packed.rs` and `crates/tpt-proto-reflect/src/lib.rs`
now call `r.nested(body)?` instead of `Reader::new(body)` in the affected
functions, matching the contract this file verifies. Regression tests:
`packed::tests::packed_fixed32_rejects_exceeding_depth`,
`packed::tests::packed_fixed64_rejects_exceeding_depth` (both in
`tpt-proto-core`), and `tests::packed_scalar_field_rejects_exceeding_depth` in
`tpt-proto-reflect` — each confirmed to fail against the pre-fix code before
being left in place as regression coverage.
