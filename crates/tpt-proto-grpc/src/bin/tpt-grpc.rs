//! `tpt-grpc` — gRPC debugging CLI (addendum §16).
//!
//! A small, dependency-light tool for interoperating with any gRPC server over
//! HTTP/2 (cleartext `h2c` by default; TLS when built with the `tls` feature):
//!
//! ```text
//! tpt-grpc health        localhost:50051 [--service ex.Foo]
//! tpt-grpc list-services localhost:50051
//! tpt-grpc reflect       localhost:50051 --symbol ex.Foo
//! tpt-grpc call          localhost:50051 --method /ex.Svc/Method --message-type ex.Foo --data '{...}'
//! tpt-grpc watch-stream  localhost:50051 --method /ex.Svc/Watch --message-type ex.Foo
//! ```
//!
//! The `call` / `watch-stream` commands accept JSON or raw binary (`--encoding`)
//! and decode responses through a `--descriptor-set` (a `FileDescriptorSet`
//! produced by `tpt-proto compile`). Metadata, deadlines, and compression can be
//! injected per call.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use clap::{Parser, Subcommand, ValueEnum};
use futures::stream;
use futures::StreamExt;

use tpt_proto_core::{Message, Reader};
use tpt_proto_descriptor::{FileDescriptorSet, FileDescriptorProto};
use tpt_proto_grpc::compression::Compression;
use tpt_proto_grpc::health::{HealthCheckRequest, HealthCheckResponse, ServingStatus};
use tpt_proto_grpc::metadata::Metadata;
use tpt_proto_grpc::reflection::{ServerReflectionRequest, ServerReflectionResponse};
use tpt_proto_grpc::transport::{ClientStream, Transport};
use tpt_proto_grpc::H2Transport;
use tpt_proto_json::{json_to_message, message_to_json, JsonOptions};
use tpt_proto_reflect::{DescriptorPool, DynamicMessage};

/// A gRPC debugging client.
#[derive(Parser)]
#[command(name = "tpt-grpc", version, about = "gRPC debugging CLI for tpt-proto")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Query the gRPC health-checking protocol.
    Health {
        /// Server address, e.g. `localhost:50051`.
        target: String,
        /// Service to query (empty string = overall server health).
        #[arg(long, default_value = "")]
        service: String,
        /// Inject request metadata (`KEY=VALUE`).
        #[arg(long = "metadata", value_name = "KEY=VALUE")]
        metadata: Vec<String>,
        /// Inject binary request metadata (`KEY=BASE64`).
        #[arg(long = "metadata-bin", value_name = "KEY=BASE64")]
        metadata_bin: Vec<String>,
        /// Per-call deadline, e.g. `5s`, `100ms`.
        #[arg(long)]
        deadline: Option<String>,
        /// Outgoing compression: `identity` or `gzip`.
        #[arg(long, default_value = "identity")]
        compression: String,
        /// Use TLS (requires the `tls` feature + `--cacert`).
        #[arg(long)]
        tls: bool,
        /// PEM CA bundle for TLS (`--tls` only).
        #[arg(long)]
        cacert: Option<PathBuf>,
    },

    /// List all services exposed by the server (via server reflection).
    ListServices {
        target: String,
        #[arg(long = "metadata", value_name = "KEY=VALUE")]
        metadata: Vec<String>,
        #[arg(long = "metadata-bin", value_name = "KEY=BASE64")]
        metadata_bin: Vec<String>,
        #[arg(long)]
        deadline: Option<String>,
        #[arg(long, default_value = "identity")]
        compression: String,
        #[arg(long)]
        tls: bool,
        #[arg(long)]
        cacert: Option<PathBuf>,
    },

    /// Query server reflection: list services, fetch a file by name, or resolve
    /// a symbol to its declaring descriptor.
    Reflect {
        target: String,
        /// Resolve a fully-qualified symbol (message/enum/service/method).
        #[arg(long)]
        symbol: Option<String>,
        /// Fetch a file descriptor by file name (e.g. `ex.proto`).
        #[arg(long)]
        file: Option<String>,
        /// List all services.
        #[arg(long)]
        list: bool,
        #[arg(long = "metadata", value_name = "KEY=VALUE")]
        metadata: Vec<String>,
        #[arg(long = "metadata-bin", value_name = "KEY=BASE64")]
        metadata_bin: Vec<String>,
        #[arg(long)]
        deadline: Option<String>,
        #[arg(long, default_value = "identity")]
        compression: String,
        #[arg(long)]
        tls: bool,
        #[arg(long)]
        cacert: Option<PathBuf>,
    },

    /// Call a unary method with a JSON or binary request payload.
    Call {
        target: String,
        /// Fully-qualified method path, e.g. `/ex.UserService/GetUser`.
        #[arg(long)]
        method: String,
        /// `FileDescriptorSet` (binary) for descriptor-based request/response
        /// decoding.
        #[arg(long)]
        descriptor_set: Option<PathBuf>,
        /// Request message fully-qualified type name (e.g. `ex.GetUserRequest`).
        #[arg(long)]
        message_type: Option<String>,
        /// Response message fully-qualified type name; defaults to `--message-type`.
        #[arg(long)]
        response_type: Option<String>,
        /// Request payload encoding.
        #[arg(long, value_enum, default_value = "json")]
        encoding: Encoding,
        /// Response output encoding.
        #[arg(long, value_enum, default_value = "json")]
        output: Encoding,
        /// Request payload as inline text (JSON object or base64/hex for binary).
        #[arg(long)]
        data: Option<String>,
        /// Read the request payload from a file instead of `--data`.
        #[arg(long = "data-file")]
        data_file: Option<PathBuf>,
        #[arg(long = "metadata", value_name = "KEY=VALUE")]
        metadata: Vec<String>,
        #[arg(long = "metadata-bin", value_name = "KEY=BASE64")]
        metadata_bin: Vec<String>,
        #[arg(long)]
        deadline: Option<String>,
        #[arg(long, default_value = "identity")]
        compression: String,
        #[arg(long)]
        tls: bool,
        #[arg(long)]
        cacert: Option<PathBuf>,
    },

    /// Call a server-streaming method and print each streamed response.
    WatchStream {
        target: String,
        #[arg(long)]
        method: String,
        #[arg(long)]
        descriptor_set: Option<PathBuf>,
        #[arg(long)]
        message_type: Option<String>,
        #[arg(long)]
        response_type: Option<String>,
        #[arg(long, value_enum, default_value = "json")]
        encoding: Encoding,
        #[arg(long, value_enum, default_value = "json")]
        output: Encoding,
        #[arg(long)]
        data: Option<String>,
        #[arg(long = "data-file")]
        data_file: Option<PathBuf>,
        #[arg(long = "metadata", value_name = "KEY=VALUE")]
        metadata: Vec<String>,
        #[arg(long = "metadata-bin", value_name = "KEY=BASE64")]
        metadata_bin: Vec<String>,
        #[arg(long)]
        deadline: Option<String>,
        #[arg(long, default_value = "identity")]
        compression: String,
        #[arg(long)]
        tls: bool,
        #[arg(long)]
        cacert: Option<PathBuf>,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Encoding {
    Json,
    Binary,
}

impl Encoding {
    fn is_json(self) -> bool {
        matches!(self, Encoding::Json)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Health {
            target,
            service,
            metadata,
            metadata_bin,
            deadline,
            compression,
            tls,
            cacert,
        } => cmd_health(&target, &service, &metadata, &metadata_bin, deadline, &compression, tls, cacert).await,
        Command::ListServices {
            target,
            metadata,
            metadata_bin,
            deadline,
            compression,
            tls,
            cacert,
        } => cmd_list_services(&target, &metadata, &metadata_bin, deadline, &compression, tls, cacert).await,
        Command::Reflect {
            target,
            symbol,
            file,
            list,
            metadata,
            metadata_bin,
            deadline,
            compression,
            tls,
            cacert,
        } => cmd_reflect(&target, symbol, file, list, &metadata, &metadata_bin, deadline, &compression, tls, cacert).await,
        Command::Call {
            target,
            method,
            descriptor_set,
            message_type,
            response_type,
            encoding,
            output,
            data,
            data_file,
            metadata,
            metadata_bin,
            deadline,
            compression,
            tls,
            cacert,
        } => {
            cmd_call(
                &target,
                &method,
                descriptor_set,
                message_type,
                response_type,
                encoding,
                output,
                data,
                data_file,
                &metadata,
                &metadata_bin,
                deadline,
                &compression,
                tls,
                cacert,
            )
            .await
        }
        Command::WatchStream {
            target,
            method,
            descriptor_set,
            message_type,
            response_type,
            encoding,
            output,
            data,
            data_file,
            metadata,
            metadata_bin,
            deadline,
            compression,
            tls,
            cacert,
        } => {
            cmd_watch_stream(
                &target,
                &method,
                descriptor_set,
                message_type,
                response_type,
                encoding,
                output,
                data,
                data_file,
                &metadata,
                &metadata_bin,
                deadline,
                &compression,
                tls,
                cacert,
            )
            .await
        }
    };
    if let Err(e) = &result {
        eprintln!("tpt-grpc: error: {e:#}");
    }
    result
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

async fn cmd_health(
    target: &str,
    service: &str,
    metadata: &[String],
    metadata_bin: &[String],
    deadline: Option<String>,
    compression: &str,
    tls: bool,
    cacert: Option<PathBuf>,
) -> Result<()> {
    let transport = connect(target, deadline, compression, tls, cacert)?;
    let md = build_metadata(metadata, metadata_bin)?;
    let req = HealthCheckRequest {
        service: service.to_string(),
    };
    let bytes = req.encode_to_vec().map_err(|e| anyhow!("encode: {e}"))?;
    let (resp_bytes, _trailers) = transport
        .unary("/grpc.health.v1.Health/Check", md, bytes)
        .await
        .map_err(|s| anyhow!("rpc failed: {s}"))?;
    let resp = HealthCheckResponse::decode(&resp_bytes).map_err(|e| anyhow!("decode: {e}"))?;
    println!("SERVING_STATUS {}", status_name(resp.status));
    Ok(())
}

async fn cmd_list_services(
    target: &str,
    metadata: &[String],
    metadata_bin: &[String],
    deadline: Option<String>,
    compression: &str,
    tls: bool,
    cacert: Option<PathBuf>,
) -> Result<()> {
    let (names, _trailers) = reflection_list_services(target, metadata, metadata_bin, deadline, compression, tls, cacert).await?;
    for n in names {
        println!("{n}");
    }
    Ok(())
}

async fn reflection_list_services(
    target: &str,
    metadata: &[String],
    metadata_bin: &[String],
    deadline: Option<String>,
    compression: &str,
    tls: bool,
    cacert: Option<PathBuf>,
) -> Result<(Vec<String>, Metadata)> {
    let transport = connect(target, deadline, compression, tls, cacert)?;
    let md = build_metadata(metadata, metadata_bin)?;
    let req = ServerReflectionRequest {
        list_services_marker: true,
        ..Default::default()
    };
    let req_bytes = req.encode_to_vec().map_err(|e| anyhow!("encode: {e}"))?;
    let stream: ClientStream<Vec<u8>> = Box::pin(stream::iter(vec![Ok(req_bytes)]));
    let (mut resp_stream, trailers) = transport
        .bidi_streaming("/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo", md, stream)
        .await
        .map_err(|s| anyhow!("rpc failed: {s}"))?;
    let first = resp_stream
        .next()
        .await
        .ok_or_else(|| anyhow!("no reflection response"))?
        .map_err(|s| anyhow!("reflection stream error: {s}"))?;
    let resp = ServerReflectionResponse::decode(&first).map_err(|e| anyhow!("decode: {e}"))?;
    let mut names = Vec::new();
    if let Some(list) = resp.list_services_response {
        for s in list.service {
            names.push(s.name);
        }
    }
    Ok((names, trailers))
}

async fn cmd_reflect(
    target: &str,
    symbol: Option<String>,
    file: Option<String>,
    list: bool,
    metadata: &[String],
    metadata_bin: &[String],
    deadline: Option<String>,
    compression: &str,
    tls: bool,
    cacert: Option<PathBuf>,
) -> Result<()> {
    if !list && symbol.is_none() && file.is_none() {
        bail!("reflect requires one of --symbol, --file, or --list");
    }
    let transport = connect(target, deadline, compression, tls, cacert)?;
    let md = build_metadata(metadata, metadata_bin)?;
    let mut req = ServerReflectionRequest::default();
    if list {
        req.list_services_marker = true;
    } else if let Some(f) = file {
        req.file_by_filename = f;
    } else if let Some(s) = symbol {
        req.file_containing_symbol = s;
    }
    let req_bytes = req.encode_to_vec().map_err(|e| anyhow!("encode: {e}"))?;
    let stream: ClientStream<Vec<u8>> = Box::pin(stream::iter(vec![Ok(req_bytes)]));
    let (mut resp_stream, _trailers) = transport
        .bidi_streaming("/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo", md, stream)
        .await
        .map_err(|s| anyhow!("rpc failed: {s}"))?;
    let first = resp_stream
        .next()
        .await
        .ok_or_else(|| anyhow!("no reflection response"))?
        .map_err(|s| anyhow!("reflection stream error: {s}"))?;
    let resp = ServerReflectionResponse::decode(&first).map_err(|e| anyhow!("decode: {e}"))?;
    print_reflection_response(&resp);
    Ok(())
}

fn print_reflection_response(resp: &ServerReflectionResponse) {
    if let Some(err) = &resp.error_response {
        println!("ERROR {}: {}", err.error_code, err.error_message);
        return;
    }
    if let Some(list) = &resp.list_services_response {
        println!("services:");
        for s in &list.service {
            println!("  - {}", s.name);
        }
        return;
    }
    if let Some(fd) = &resp.file_descriptor_response {
        println!("file_descriptor_proto ({} file(s)):", fd.file_descriptor_proto.len());
        for bytes in &fd.file_descriptor_proto {
            match FileDescriptorProto::decode(bytes) {
                Ok(f) => {
                    let pkg = f.package.as_deref().unwrap_or("");
                    let msg_count = f.message_type.len();
                    let enum_count = f.enum_type.len();
                    let svc_count = f.service.len();
                    println!(
                        "  name={} package={} messages={} enums={} services={}",
                        f.name.as_deref().unwrap_or("?"),
                        pkg,
                        msg_count,
                        enum_count,
                        svc_count
                    );
                }
                Err(e) => println!("  (failed to decode descriptor: {e})"),
            }
        }
        return;
    }
    if let Some(ext) = &resp.all_extension_numbers_response {
        println!(
            "all_extension_numbers {}: {:?}",
            ext.base_type_name, ext.extension_number
        );
        return;
    }
    println!("(empty reflection response)");
}

#[allow(clippy::too_many_arguments)]
async fn cmd_call(
    target: &str,
    method: &str,
    descriptor_set: Option<PathBuf>,
    message_type: Option<String>,
    response_type: Option<String>,
    encoding: Encoding,
    output: Encoding,
    data: Option<String>,
    data_file: Option<PathBuf>,
    metadata: &[String],
    metadata_bin: &[String],
    deadline: Option<String>,
    compression: &str,
    tls: bool,
    cacert: Option<PathBuf>,
) -> Result<()> {
    let transport = connect(target, deadline, compression, tls, cacert)?;
    let md = build_metadata(metadata, metadata_bin)?;
    let (pool, req_desc, resp_desc) =
        prepare_descriptors(descriptor_set, &message_type, &response_type)?;
    let opts = JsonOptions::default();
    let req_bytes = encode_request(encoding, data, data_file, &pool, req_desc.as_ref(), &opts)?;
    let (resp_bytes, trailers) = transport
        .unary(method, md, req_bytes)
        .await
        .map_err(|s| anyhow!("rpc failed: {s}"))?;
    let status = trailers
        .get_text("grpc-status")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "0".to_string());
    let message = decode_response(output, &pool, resp_desc.as_ref(), &resp_bytes, &opts)?;
    println!("grpc-status: {status}");
    if let Some(m) = trailers.get_text("grpc-message") {
        println!("grpc-message: {m}");
    }
    println!("{message}");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_watch_stream(
    target: &str,
    method: &str,
    descriptor_set: Option<PathBuf>,
    message_type: Option<String>,
    response_type: Option<String>,
    encoding: Encoding,
    output: Encoding,
    data: Option<String>,
    data_file: Option<PathBuf>,
    metadata: &[String],
    metadata_bin: &[String],
    deadline: Option<String>,
    compression: &str,
    tls: bool,
    cacert: Option<PathBuf>,
) -> Result<()> {
    let transport = connect(target, deadline, compression, tls, cacert)?;
    let md = build_metadata(metadata, metadata_bin)?;
    let (pool, req_desc, resp_desc) =
        prepare_descriptors(descriptor_set, &message_type, &response_type)?;
    let opts = JsonOptions::default();
    let req_bytes = encode_request(encoding, data, data_file, &pool, req_desc.as_ref(), &opts)?;
    let (mut resp_stream, _trailers) = transport
        .server_streaming(method, md, req_bytes)
        .await
        .map_err(|s| anyhow!("rpc failed: {s}"))?;
    let mut count = 0u32;
    while let Some(item) = resp_stream.next().await {
        let bytes = item.map_err(|s| anyhow!("stream error: {s}"))?;
        let message = decode_response(output, &pool, resp_desc.as_ref(), &bytes, &opts)?;
        println!("--- message {count} ---");
        println!("{message}");
        count += 1;
    }
    eprintln!("tpt-grpc: received {count} message(s)");
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn connect(
    target: &str,
    deadline: Option<String>,
    compression: &str,
    tls: bool,
    _cacert: Option<PathBuf>,
) -> Result<Box<dyn Transport>> {
    let comp = parse_compression(compression)?;
    let timeout = deadline.map(|d| parse_duration(&d)).transpose()?;
    if tls {
        #[cfg(feature = "tls")]
        {
            let ca = _cacert.context("TLS requires --cacert (PEM CA bundle)")?;
            return Ok(Box::new(tls_transport::TlsTransport::new(
                target, comp, timeout, &ca,
            )?));
        }
        #[cfg(not(feature = "tls"))]
        {
            bail!("TLS support was not compiled in; rebuild tpt-proto-grpc with the `tls` feature");
        }
    }
    let mut t = H2Transport::new(target).with_compression(comp);
    if let Some(d) = timeout {
        t = t.with_timeout(d);
    }
    Ok(Box::new(t))
}

fn parse_compression(s: &str) -> Result<Compression> {
    Ok(Compression::from_header(s))
}

fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    let (num, unit) = match s.find(|c: char| !c.is_ascii_digit() && c != '.') {
        Some(i) => (s[..i].parse::<f64>().map_err(|e| anyhow!("bad duration: {e}"))?, &s[i..]),
        None => (s.parse::<f64>().map_err(|e| anyhow!("bad duration: {e}"))?, "s"),
    };
    let secs = match unit {
        "ns" => num / 1e9,
        "us" | "µs" => num / 1e6,
        "ms" => num / 1e3,
        "s" => num,
        "m" => num * 60.0,
        "h" => num * 3600.0,
        other => bail!("unknown duration unit '{other}'"),
    };
    Ok(Duration::from_secs_f64(secs))
}

fn build_metadata(text: &[String], bin: &[String]) -> Result<Metadata> {
    let mut md = Metadata::new();
    for kv in text {
        let (k, v) = kv
            .split_once('=')
            .ok_or_else(|| anyhow!("metadata '{kv}' must be KEY=VALUE"))?;
        md.insert_text(k, v)
            .map_err(|e| anyhow!("metadata {k}: {e}"))?;
    }
    for kv in bin {
        let (k, v) = kv
            .split_once('=')
            .ok_or_else(|| anyhow!("metadata-bin '{kv}' must be KEY=BASE64"))?;
        let raw = base64::engine::general_purpose::STANDARD
            .decode(v.trim())
            .map_err(|e| anyhow!("metadata-bin {k} base64: {e}"))?;
        md.insert_binary(k, &raw)
            .map_err(|e| anyhow!("metadata-bin {k}: {e}"))?;
    }
    Ok(md)
}

fn load_descriptor_set(path: &PathBuf) -> Result<FileDescriptorSet> {
    let raw = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    FileDescriptorSet::decode(&raw).map_err(|e| anyhow!("decode descriptor set: {e}"))
}

fn prepare_descriptors(
    descriptor_set: Option<PathBuf>,
    message_type: &Option<String>,
    response_type: &Option<String>,
) -> Result<(DescriptorPool, Option<std::sync::Arc<tpt_proto_descriptor::DescriptorProto>>, Option<std::sync::Arc<tpt_proto_descriptor::DescriptorProto>>)> {
    let (pool, set) = match &descriptor_set {
        Some(p) => {
            let set = load_descriptor_set(p)?;
            let pool = DescriptorPool::from_set(&set);
            (pool, Some(set))
        }
        None => (DescriptorPool::default(), None),
    };
    let req_desc = match message_type {
        Some(name) => Some(lookup_message(&pool, name)?),
        None => None,
    };
    let resp_desc = match response_type {
        Some(name) => Some(lookup_message(&pool, name)?),
        None => match message_type {
            Some(name) => Some(lookup_message(&pool, name)?),
            None => None,
        },
    };
    let _ = set;
    Ok((pool, req_desc, resp_desc))
}

fn lookup_message(pool: &DescriptorPool, name: &str) -> Result<std::sync::Arc<tpt_proto_descriptor::DescriptorProto>> {
    pool.lookup_message(name)
        .ok_or_else(|| anyhow!("message type '{name}' not found in descriptor set"))
}

fn encode_request(
    encoding: Encoding,
    data: Option<String>,
    data_file: Option<PathBuf>,
    pool: &DescriptorPool,
    desc: Option<&std::sync::Arc<tpt_proto_descriptor::DescriptorProto>>,
    opts: &JsonOptions,
) -> Result<Vec<u8>> {
    let raw_text = match (data, data_file) {
        (Some(d), _) => d,
        (None, Some(f)) => std::fs::read_to_string(&f)
            .with_context(|| format!("reading {}", f.display()))?,
        (None, None) => String::new(),
    };
    if encoding.is_json() {
        let desc = desc.ok_or_else(|| {
            anyhow!("JSON request encoding requires --message-type and --descriptor-set")
        })?;
        let value: serde_json::Value = if raw_text.trim().is_empty() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            serde_json::from_str(&raw_text).map_err(|e| anyhow!("parse JSON: {e}"))?
        };
        let dm = json_to_message(pool, desc, &value, opts).map_err(|e| anyhow!("json->message: {e}"))?;
        dm.encode().map_err(|e| anyhow!("encode: {e}"))
    } else {
        decode_bytes(&raw_text)
    }
}

fn decode_response(
    output: Encoding,
    pool: &DescriptorPool,
    desc: Option<&std::sync::Arc<tpt_proto_descriptor::DescriptorProto>>,
    bytes: &[u8],
    opts: &JsonOptions,
) -> Result<String> {
    if output.is_json() {
        let desc = desc.ok_or_else(|| {
            anyhow!("JSON response decoding requires --response-type/--message-type and --descriptor-set (use --output binary for raw bytes)")
        })?;
        let dm = DynamicMessage::decode(pool, desc.clone(), &mut Reader::new(bytes))
            .map_err(|e| anyhow!("decode response: {e}"))?;
        let value = message_to_json(pool, desc, &dm, opts).map_err(|e| anyhow!("message->json: {e}"))?;
        Ok(serde_json::to_string_pretty(&value).map_err(|e| anyhow!("json render: {e}"))?)
    } else {
        Ok(hex::encode(bytes))
    }
}

/// Decode a raw byte-string payload given as base64 or hex.
fn decode_bytes(text: &str) -> Result<Vec<u8>> {
    let t = text.trim();
    if let Ok(b) = base64::engine::general_purpose::STANDARD.decode(t) {
        if !t.is_empty() {
            return Ok(b);
        }
    }
    hex::decode(t).map_err(|e| anyhow!("payload is not valid base64 or hex: {e}"))
}

fn status_name(s: ServingStatus) -> &'static str {
    match s {
        ServingStatus::Unknown => "UNKNOWN",
        ServingStatus::Serving => "SERVING",
        ServingStatus::NotServing => "NOT_SERVING",
        ServingStatus::ServiceUnknown => "SERVICE_UNKNOWN",
    }
}

// ---------------------------------------------------------------------------
// Optional TLS client transport (only compiled with the `tls` feature).
// ---------------------------------------------------------------------------

#[cfg(feature = "tls")]
mod tls_transport {
    use std::sync::Arc;
    use std::time::Duration;

    use anyhow::{anyhow, Context, Result};
    use bytes::Bytes;
    use futures::stream;
    use futures::StreamExt;
    use http::header::{HeaderMap, HeaderName, HeaderValue};
    use http::Request as HttpRequest;
    use rustls::RootCertStore;
    use rustls_pemfile::certs;
    use tokio::net::TcpStream;
    use tokio_rustls::TlsConnector;

    use tpt_proto_grpc::codec::{decode_message, encode_message, MAX_MESSAGE_SIZE_DEFAULT};
    use tpt_proto_grpc::compression::Compression;
    use tpt_proto_grpc::metadata::Metadata;
    use tpt_proto_grpc::status::{Code, Status};
    use tpt_proto_grpc::transport::{ClientStream, ServerStream, Transport};

    /// A TLS (mutual-TLS-capable) HTTP/2 gRPC client transport.
    pub struct TlsTransport {
        endpoint: String,
        compression: Compression,
        max_message_size: usize,
        timeout: Option<Duration>,
        ca_cert: Vec<u8>,
    }

    impl TlsTransport {
        pub fn new(
            endpoint: &str,
            compression: Compression,
            timeout: Option<Duration>,
            ca_cert_path: &std::path::Path,
        ) -> Result<Self> {
            let ca_cert = std::fs::read(ca_cert_path)
                .with_context(|| format!("reading CA cert {}", ca_cert_path.display()))?;
            Ok(TlsTransport {
                endpoint: endpoint.to_string(),
                compression,
                max_message_size: MAX_MESSAGE_SIZE_DEFAULT,
                timeout,
                ca_cert,
            })
        }

        fn host_and_port(&self) -> Result<(String, u16)> {
            let s = self.endpoint.trim();
            let s = s
                .strip_prefix("http://")
                .or_else(|| s.strip_prefix("https://"))
                .unwrap_or(s);
            let (host, port) = s.rsplit_once(':').ok_or_else(|| anyhow!("invalid endpoint"))?;
            let port = port.parse::<u16>().map_err(|_| anyhow!("invalid port"))?;
            Ok((host.to_string(), port))
        }

        async fn connect(&self) -> Result<(h2::client::SendRequest<Bytes>, h2::client::Connection<tokio_rustls::client::TlsStream<TcpStream>>)> {
            let (host, port) = self.host_and_port()?;
            let tcp = TcpStream::connect((host.as_str(), port))
                .await
                .map_err(|e| Status::new(Code::Unavailable, format!("connect: {e}")))?;
            tcp.set_nodelay(true).ok();

            let mut root_store = RootCertStore::empty();
            let mut reader = std::io::Cursor::new(&self.ca_cert);
            for cert in certs(&mut reader).map_err(|e| anyhow!("parse CA: {e}"))? {
                root_store
                    .add(cert)
                    .map_err(|e| anyhow!("add CA cert: {e}"))?;
            }
            let config = rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth();
            let connector = TlsConnector::from(Arc::new(config));
            let server_name = rustls::ServerName::try_from(host.as_str())
                .map_err(|e| anyhow!("bad server name: {e}"))?;
            let tls = connector
                .connect(server_name, tcp)
                .await
                .map_err(|e| anyhow!("tls handshake: {e}"))?;
            let (send, conn) = h2::client::Builder::new()
                .handshake(tls)
                .await
                .map_err(|e| anyhow!("h2 handshake: {e}"))?;
            Ok((send, conn))
        }

        async fn exchange(
            &self,
            path: &str,
            metadata: Metadata,
            request_body: Option<Vec<u8>>,
            mut client_stream: Option<ClientStream<Vec<u8>>>,
        ) -> Result<(Vec<u8>, HeaderMap, Option<HeaderMap>), Status> {
            let (mut send, conn) = self.connect().await?;
            tokio::spawn(async move {
                let _ = conn.await;
            });
            let req = self.build_request(path, &metadata)?;
            let (resp_future, mut body_send) = send
                .send_request(req, false)
                .map_err(|e| Status::new(Code::Unavailable, format!("send request: {e}")))?;
            let resp = resp_future
                .await
                .map_err(|e| Status::new(Code::Unavailable, format!("response: {e}")))?;
            if let Some(bytes) = request_body {
                let framed = encode_message(&bytes, self.compression.clone(), self.max_message_size)
                    .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
                body_send
                    .send_data(Bytes::from(framed), true)
                    .map_err(|e| Status::new(Code::Internal, format!("send data: {e}")))?;
            } else if let Some(mut cs) = client_stream.take() {
                while let Some(msg) = cs.next().await {
                    let msg = msg.map_err(|s| Status::new(Code::Internal, s.to_string()))?;
                    let framed = encode_message(&msg, self.compression.clone(), self.max_message_size)
                        .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
                    body_send
                        .send_data(Bytes::from(framed), false)
                        .map_err(|e| Status::new(Code::Internal, format!("send data: {e}")))?;
                }
                body_send
                    .send_data(Bytes::new(), true)
                    .map_err(|e| Status::new(Code::Internal, format!("end stream: {e}")))?;
            } else {
                body_send
                    .send_data(Bytes::new(), true)
                    .map_err(|e| Status::new(Code::Internal, format!("end stream: {e}")))?;
            }
            let (parts, mut recv) = resp.into_parts();
            let body = collect_body(&mut recv, self.max_message_size).await?;
            let trailers = recv.trailers().await.map_err(|e| Status::new(Code::Internal, e.to_string()))?;
            Ok((body, parts.headers, trailers))
        }

        fn build_request(&self, path: &str, metadata: &Metadata) -> Result<HttpRequest<()>, Status> {
            let (host, port) = self.host_and_port()?;
            let mut builder = HttpRequest::builder()
                .method("POST")
                .uri(format!("https://{host}:{port}{path}"))
                .header("content-type", tpt_proto_grpc::CONTENT_TYPE_GRPC)
                .header("te", "trailers")
                .header("user-agent", "tpt-proto-grpc/0.1");
            for (k, v) in metadata.to_headers() {
                if let (Ok(name), Ok(val)) = (
                    HeaderName::from_bytes(k.as_bytes()),
                    HeaderValue::from_bytes(v.as_bytes()),
                ) {
                    builder = builder.header(name, val);
                }
            }
            if self.compression != Compression::Identity {
                builder = builder.header("grpc-encoding", self.compression.as_header());
            }
            if let Some(t) = self.timeout {
                builder = builder.header("grpc-timeout", tpt_proto_grpc::timeout::format_timeout(t));
            }
            builder.body(()).map_err(|e| Status::new(Code::Internal, e.to_string()))
        }
    }

    async fn collect_body(
        recv: &mut h2::RecvStream,
        max: usize,
    ) -> Result<Vec<u8>, Status> {
        let mut buf = Vec::new();
        while let Some(chunk) = recv.data().await {
            let chunk = chunk.map_err(|e| Status::new(Code::Internal, e.to_string()))?;
            buf.extend_from_slice(&chunk);
            if buf.len() > max {
                return Err(Status::new(Code::ResourceExhausted, "response too large"));
            }
        }
        Ok(buf)
    }

    fn split_grpc_frames(buf: &[u8]) -> Result<Vec<Vec<u8>>, Status> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < buf.len() {
            if i + 5 > buf.len() {
                return Err(Status::new(Code::Internal, "truncated frame"));
            }
            let len = u32::from_be_bytes([buf[i + 1], buf[i + 2], buf[i + 3], buf[i + 4]]) as usize;
            let total = 5 + len;
            if total > buf.len() {
                return Err(Status::new(Code::Internal, "truncated message"));
            }
            out.push(buf[i..total].to_vec());
            i = total;
        }
        Ok(out)
    }

    #[async_trait::async_trait]
    impl Transport for TlsTransport {
        async fn unary(&self, path: &str, metadata: Metadata, message: Vec<u8>) -> Result<(Vec<u8>, Metadata), Status> {
            let framed = encode_message(&message, self.compression.clone(), self.max_message_size)
                .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
            let (body, headers, trailers) = self.exchange(path, metadata, Some(framed), None).await?;
            let frames = split_grpc_frames(&body)?;
            let raw = frames.into_iter().next().ok_or_else(|| Status::new(Code::Internal, "empty response"))?;
            let payload = decode_message(&raw, self.compression.clone(), self.max_message_size)
                .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
            Ok((payload, merge_metadata(&headers, trailers.as_ref())))
        }

        async fn server_streaming(&self, path: &str, metadata: Metadata, message: Vec<u8>) -> Result<(ServerStream<Vec<u8>>, Metadata), Status> {
            let framed = encode_message(&message, self.compression.clone(), self.max_message_size)
                .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
            let (body, headers, trailers) = self.exchange(path, metadata, Some(framed), None).await?;
            let compression = self.compression.clone();
            let max = self.max_message_size;
            let frames = split_grpc_frames(&body)?;
            let stream = stream::iter(frames.into_iter().map(move |raw| {
                decode_message(&raw, compression.clone(), max).map_err(|e| Status::new(Code::Internal, e.to_string()))
            }));
            Ok((Box::pin(stream), merge_metadata(&headers, trailers.as_ref())))
        }

        async fn client_streaming(&self, path: &str, metadata: Metadata, stream: ClientStream<Vec<u8>>) -> Result<(Vec<u8>, Metadata), Status> {
            let (body, headers, trailers) = self.exchange(path, metadata, None, Some(stream)).await?;
            let frames = split_grpc_frames(&body)?;
            let raw = frames.into_iter().next().ok_or_else(|| Status::new(Code::Internal, "empty response"))?;
            let payload = decode_message(&raw, self.compression.clone(), self.max_message_size)
                .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
            Ok((payload, merge_metadata(&headers, trailers.as_ref())))
        }

        async fn bidi_streaming(&self, path: &str, metadata: Metadata, stream: ClientStream<Vec<u8>>) -> Result<(ServerStream<Vec<u8>>, Metadata), Status> {
            let (body, headers, trailers) = self.exchange(path, metadata, None, Some(stream)).await?;
            let compression = self.compression.clone();
            let max = self.max_message_size;
            let frames = split_grpc_frames(&body)?;
            let stream = stream::iter(frames.into_iter().map(move |raw| {
                decode_message(&raw, compression.clone(), max).map_err(|e| Status::new(Code::Internal, e.to_string()))
            }));
            Ok((Box::pin(stream), merge_metadata(&headers, trailers.as_ref())))
        }
    }

    fn merge_metadata(headers: &HeaderMap, trailers: Option<&HeaderMap>) -> Metadata {
        let mut md = Metadata::new();
        let mut add = |h: &HeaderMap| {
            for (name, value) in h.iter() {
                if name.as_str().starts_with(':') {
                    continue;
                }
                if let Ok(v) = value.to_str() {
                    let _ = md.insert_text(name.as_str(), v);
                }
            }
        };
        add(headers);
        if let Some(t) = trailers {
            add(t);
        }
        md
    }
}
