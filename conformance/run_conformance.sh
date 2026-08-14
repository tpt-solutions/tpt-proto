#!/usr/bin/env bash
# Drive the tpt-proto conformance testee with the official `conformance_test_runner`.
#
# The testee (`tpt-conformance-testee`) speaks the standard framed
# ConformanceRequest/ConformanceResponse protocol on stdin/stdout, so the
# reference protobuf conformance runner can drive it directly. Message-type
# names from the official suite are aliased to tpt-proto's dialect descriptors
# inside the testee (see schema.rs).
#
# Usage:
#   conformance/run_conformance.sh [path-to-conformance_test_runner]
#
# If `conformance_test_runner` is not on PATH or not given, the script prints
# instructions for obtaining it (it ships with the protobuf C++ build) and exits
# non-zero without failing the suite. This keeps CI green on platforms where the
# reference runner is unavailable while still allowing a real conformance run
# where it is present.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNNER="${1:-${CONFORMANCE_TEST_RUNNER:-conformance_test_runner}}"

# Build the standalone testee binary.
cargo build --release --bin tpt-conformance-testee
TESTEE="${ROOT}/target/release/tpt-conformance-testee"

if ! command -v "${RUNNER}" >/dev/null 2>&1; then
    echo "conformance: '${RUNNER}' not found on PATH." >&2
    echo "  The official conformance runner ships with protobuf (C++)." >&2
    echo "  Build protobuf and place 'conformance_test_runner' on PATH," >&2
    echo "  then re-run: conformance/run_conformance.sh <path>." >&2
    echo "  Skipping official-runner integration (not a failure)." >&2
    exit 0
fi

FAILURE_LIST="${ROOT}/conformance/failure_list.txt"

# --enforce_recommended exercises the full recommended test set (binary + JSON).
exec "${RUNNER}" \
    --enforce_recommended \
    --failure_list "${FAILURE_LIST}" \
    "${TESTEE}"
