# Quickstart

This guide gets you from a `.proto` schema to running Rust code as fast as
possible. It assumes a Cargo project.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## 1. Add dependencies

```toml
# Cargo.toml
[dependencies]
tpt-proto-core = "0"      # binary wire-format runtime
tpt-proto-codegen-rust = "0"
```

For build-time code generation you also need:

```toml
[build-dependencies]
tpt-proto-build = "0"
```

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
tpt-proto generate --input proto --output src/generated
```

Then include the generated module:

```rust
// src/generated.rs
pub mod user { include!("generated/user.rs"); }
```

### Option B — `build.rs`

```rust
// build.rs
fn main() {
    tpt_proto_build::compile_protos(&["proto/user.proto"], &["proto"])
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

## 4. Use the CLI to inspect messages

With a compiled descriptor set you can convert between formats without writing
Rust:

```sh
# Emit a FileDescriptorSet
tpt-proto descriptors --input proto --output descriptor.bin

# Decode a binary message to text
tpt-proto decode --descriptor descriptor.bin --message example.User --input user.bin

# JSON -> binary
tpt-proto encode --descriptor descriptor.bin --message example.User --input user.json
```

See the [CLI](../spec.txt) (§18) for the full command set
(`compile`, `generate`, `descriptors`, `decode`, `encode`,
`json-to-binary`, `binary-to-json`, `text-to-binary`, `binary-to-text`,
`lint`, `diff`).

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
