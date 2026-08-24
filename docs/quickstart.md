# Quickstart

This guide gets you from a `.proto` schema to running Rust code as fast as
possible. It assumes a Cargo project.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## 1. Add dependencies

You only need **one runtime crate** and **one build-time crate**:

```toml
# Cargo.toml
[dependencies]
tpt-proto-core = "0"      # binary wire-format runtime (encode/decode)

[build-dependencies]
tpt-proto-build = "0"     # compiles .proto -> Rust from build.rs
```

> Do **not** add `tpt-proto-codegen-rust` to `[dependencies]` or
> `[build-dependencies]` yourself: it is the code generator that
> `tpt-proto-build` invokes internally at build time, and the generated code
> only depends on `tpt-proto-core`. Depending on it directly is unnecessary and
> will not make generated code appear.

`build.rs` integration is documented in [Code generation](codegen.md) and the
`build` crate.

## 2. Write a schema

```proto
// proto/user.proto
syntax = "proto3";

package example;

message User {
  int64 id = 1;
  string name = 2;
  string email = 3;
}
```

## 3. Generate Rust code

### Option A — CLI

```sh
# Generate Rust source for a schema (add -I DIR for import search paths).
tpt-proto generate proto/user.proto --out src/user.rs
```

Then include the generated module:

```rust
// src/main.rs
include!("user.rs");
```

### Option B — `build.rs`

```rust
// build.rs
fn main() {
    tpt_proto_build::compile_protos_simple(
        &["proto/user.proto".into()],  // .proto inputs
        &["proto".into()],             // import search paths
    )
    .expect("compile protos");
}
```

```rust
// src/main.rs
include!(concat!(env!("OUT_DIR"), "/user.rs"));

fn main() {
    let user = User {
        id: 42,
        name: "Ada".into(),
        email: "ada@example.com".into(),
    };

    let bytes = user.encode_to_vec();
    let decoded = User::decode(bytes.as_slice()).expect("decode");
    assert_eq!(user, decoded);
}
```

## 4. Use the CLI to inspect and convert messages

The CLI compiles schemas (and their transitive imports via `-I`) on the fly —
no pre-built descriptor file needed:

```sh
# Describe a schema's structure
tpt-proto describe proto/user.proto

# Emit the serialized FileDescriptorSet (entry + all transitive imports)
tpt-proto descriptors proto/user.proto descriptor.bin

# Decode wire bytes against a schema (hex input)
tpt-proto decode proto/user.proto example.User 082a1203416461

# JSON -> binary
tpt-proto to-binary --from json proto/user.proto example.User user.json user.bin

# binary -> text format
tpt-proto text proto/user.proto example.User user.bin
```

You can also scaffold a ready-to-build starter project:

```sh
tpt-proto init my-app   # creates Cargo.toml, build.rs, proto/, src/main.rs
```

See [the CLI section of the spec](../spec.txt) (§18) for the full command set
(`compile`, `generate`, `descriptors`, `describe`, `decode`, `encode`,
`json`, `text`, `to-binary`, `lint`, `diff`, `init`).

## 5. Work with messages dynamically

When you do not have generated types at compile time, use the reflection
runtime:

```rust,ignore
use tpt_proto_reflect::DynamicMessage;

let msg = DynamicMessage::decode(&descriptor, bytes)?;
let name = msg.get_field("name")?;
let encoded = msg.encode()?;
```

See [Reflection & dynamic messages](reflection.md).

## 6. Next steps

- [Language support](language-support.md) — proto2/proto3/editions differences.
- [Wire format](wire-format.md) — encoding details and limits.
- [JSON mapping](json.md) — convert to/from JSON.
- [gRPC layer](grpc.md) — turn services into RPCs.
