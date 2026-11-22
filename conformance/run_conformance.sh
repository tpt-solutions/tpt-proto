#!/usr/bin/env bash
# Run the official protobuf conformance suites against the tpt-proto testee and
# record the result in conformance/results/latest.md.
#
# The testee (`tpt-conformance-testee`) speaks the standard framed
# ConformanceRequest/ConformanceResponse protocol on stdin/stdout, so the
# reference protobuf conformance runner can drive it directly.
#
# Usage:
#   conformance/run_conformance.sh [path-to-conformance_test_runner]
#
# The runner is taken from the first argument, then $CONFORMANCE_TEST_RUNNER,
# then PATH. It ships with the protobuf C++ build (protobuf_BUILD_CONFORMANCE=ON)
# and must be built from PROTOBUF_VERSION; it only runs on POSIX systems because
# it uses fork(). The script exits non-zero when the runner is missing or any
# case fails, so a skipped run can never look like a pass.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"
RUNNER="${1:-${CONFORMANCE_TEST_RUNNER:-conformance_test_runner}}"
PROTOBUF_VERSION="${PROTOBUF_VERSION:-v6.31.1}"
RESULTS_DIR="${ROOT}/conformance/results"
mkdir -p "${RESULTS_DIR}"

if ! command -v "${RUNNER}" >/dev/null 2>&1; then
    echo "conformance: '${RUNNER}' not found." >&2
    echo "  Build protobuf ${PROTOBUF_VERSION} with -Dprotobuf_BUILD_CONFORMANCE=ON," >&2
    echo "  then pass the path to conformance_test_runner (or set CONFORMANCE_TEST_RUNNER)." >&2
    exit 1
fi

# Build the standalone testee binary.
cargo build --release --bin tpt-conformance-testee
TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT}/target}"
TESTEE="${TARGET_DIR}/release/tpt-conformance-testee"

LOG="$(mktemp)"
trap 'rm -f "${LOG}"' EXIT

# --enforce_recommended exercises the full recommended test set. The runner
# also runs the text-format suite once the binary/JSON suite passes.
set +e
"${RUNNER}" \
    --enforce_recommended \
    --failure_list "${ROOT}/conformance/failure_list.txt" \
    "${TESTEE}" > "${LOG}" 2>&1
STATUS=$?
set -e

# Keep the runner's own output for the record and for the console.
cat "${LOG}" >&2

COMMIT="$(git -C "${ROOT}" rev-parse --short HEAD 2>/dev/null || echo unknown)"
# Uncommitted changes mean the commit above does not fully describe the testee.
if [ -n "$(git -C "${ROOT}" status --porcelain 2>/dev/null)" ]; then
    COMMIT="${COMMIT} (with uncommitted changes)"
fi
if [ "${STATUS}" -eq 0 ]; then OVERALL="PASSED"; else OVERALL="FAILED"; fi
{
    echo "# Conformance results"
    echo
    echo "- Overall: **${OVERALL}** (runner exit code ${STATUS})"
    echo "- Date (UTC): $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "- Commit: ${COMMIT}"
    echo "- Reference runner: protobuf ${PROTOBUF_VERSION} (\`conformance_test_runner\`)"
    echo "- Testee: \`tpt-conformance-testee\` (release build)"
    echo "- Failure list: \`conformance/failure_list.txt\` ($(grep -cv '^\s*\(#\|$\)' "${ROOT}/conformance/failure_list.txt" || true) entries)"
    echo
    echo "## Suites"
    echo
    echo '```'
    grep "CONFORMANCE SUITE" "${LOG}" || echo "(no suite summary was produced)"
    echo '```'
    if [ "${STATUS}" -ne 0 ]; then
        echo
        echo "## Unexpected failures (first 50)"
        echo
        echo '```'
        grep "^ERROR, test=" "${LOG}" | head -50 || true
        echo '```'
    fi
} > "${RESULTS_DIR}/latest.md"

echo "conformance: ${OVERALL}; summary written to conformance/results/latest.md" >&2
exit "${STATUS}"
