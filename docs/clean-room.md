# Clean-Room Policy

`tpt-proto` is developed as a **clean-room** implementation (§2.3, §24, §25). This
document summarizes the policy; the authoritative records live in
`provenance/`.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## Allowed inputs

- public protobuf documentation;
- public wire-format documentation;
- public language guides;
- public conformance behavior;
- original design notes (this repository's `spec.txt`);
- original tests.

## Disallowed inputs

- upstream protobuf implementation source code;
- existing protobuf crate internals;
- copied generated-code templates;
- proprietary implementations;
- AI prompts that contain upstream source code.

## AI-assisted contributions

AI tooling may be used, but it must follow the clean-room policy: prompts must
not include upstream source, and generated code is reviewed to ensure it is
original and derived only from public specifications and `spec.txt`. The full
policy and review process are in `provenance/ai-policy.md`.

## Provenance records

The `provenance/` directory records:

- `README.md` — overview of provenance status.
- `decisions.md` — log of major implementation decisions and rationale.
- `ai-policy.md` — AI usage and review process policy.
- `test-vectors.md` — origin and derivation of test vectors.

## Why it matters

Clean-room provenance reduces IP risk and supports the project's independence.
Any code suspected of reproducing upstream implementation code is rewritten. See
[Licensing](licensing.md) for the dual-license and trademark posture, and
[Security](security.md) for the corresponding provenance risk mitigation.
