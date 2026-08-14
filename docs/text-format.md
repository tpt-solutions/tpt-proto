# Text Format

`tpt-proto-text` implements the protobuf **text format** (§13), a human-readable,
debugging-friendly representation. It is commonly used for inspection, CLI
output, and test fixtures.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## Printing

`print` renders a message to text format, including support for:

- repeated fields (comma- or newline-separated entries);
- map fields;
- nested messages;
- oneofs (only the set member is printed);
- extensions (by their extended field path);
- unknown fields (subject to policy).

## Parsing

`parse` reads a text-format string back into a message. The parser is robust to
the conventional text-format syntax and tolerates the variations used by
reference tooling.

## Unknown field policies

Text output of unknown fields can be configured (preserve / discard / fail),
consistent with the binary wire-format policy (see
[Wire format](wire-format.md)).

## Deterministic output

A deterministic mode produces stable, sorted text output, useful for
reproducible snapshots, diffs, and audits. See also
[Deterministic encoding](deterministic-encoding.md) for the binary equivalent.

## CLI usage

```sh
tpt-proto binary-to-text --descriptor descriptor.bin --message example.User --input user.bin
tpt-proto text-to-binary --descriptor descriptor.bin --message example.User --input user.txt
```

See [Quickstart](quickstart.md) and the CLI design (§18).
