//! gRPC server reflection protocol (§14, addendum §14).
//!
//! Implements the `grpc.reflection.v1alpha.ServerReflection` service: a handler
//! over a [`FileDescriptorSet`] that answers `ListServices`, file-by-filename,
//! file-containing-symbol, and all-extension-numbers queries. The server
//! runtime drives the bidi `ServerReflectionInfo` method by feeding each client
//! request through [`ReflectionService::handle`] and returning the produced
//! responses.

use std::collections::BTreeSet;

use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use tpt_proto_core::{Message, Reader, Result as CoreResult, Writer};
use tpt_proto_core::scalar;
use tpt_proto_descriptor::{DescriptorProto, FileDescriptorProto, FileDescriptorSet, ServiceDescriptorProto};

use crate::context::RpcContext;
use crate::method::MethodKind;
use crate::service::ServiceHandler;
use crate::status::{Code, Status};
use crate::transport::{ClientStream, ServerStream};

/// `grpc.reflection.v1alpha.ErrorResponse` — field 1 `error_code` (int32),
/// field 2 `error_message` (string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorResponse {
    /// A gRPC status code describing the failure.
    pub error_code: i32,
    /// A human-readable message.
    pub error_message: String,
}

impl Default for ErrorResponse {
    fn default() -> Self {
        ErrorResponse {
            error_code: 0,
            error_message: String::new(),
        }
    }
}

impl Message for ErrorResponse {
    fn encode(&self, w: &mut Writer) -> CoreResult<()> {
        if self.error_code != 0 {
            scalar::encode_int32(w, 1, self.error_code);
        }
        if !self.error_message.is_empty() {
            scalar::encode_string(w, 2, &self.error_message);
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> CoreResult<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match tag.field_number {
                1 => self.error_code = scalar::read_int32(r)?,
                2 => self.error_message = r.read_string_owned()?,
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

/// `grpc.reflection.v1alpha.ServiceResponse` — field 1 `name` (string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceResponse {
    /// Fully-qualified service name (e.g. `example.UserService`).
    pub name: String,
}

impl Default for ServiceResponse {
    fn default() -> Self {
        ServiceResponse {
            name: String::new(),
        }
    }
}

impl Message for ServiceResponse {
    fn encode(&self, w: &mut Writer) -> CoreResult<()> {
        if !self.name.is_empty() {
            scalar::encode_string(w, 1, &self.name);
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> CoreResult<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            if tag.field_number == 1 {
                self.name = r.read_string_owned()?;
            } else {
                r.skip(tag.wire_type)?;
            }
        }
        Ok(())
    }
}

/// `grpc.reflection.v1alpha.ListServiceResponse` — field 1 `service` (repeated
/// `ServiceResponse`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListServiceResponse {
    /// The services exposed by the server.
    pub service: Vec<ServiceResponse>,
}

impl Message for ListServiceResponse {
    fn encode(&self, w: &mut Writer) -> CoreResult<()> {
        for s in &self.service {
            let bytes = s.encode_to_vec()?;
            scalar::encode_message(w, 1, &bytes);
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> CoreResult<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            if tag.field_number == 1 {
                let b = r.read_length_delimited()?;
                self.service.push(ServiceResponse::decode(b)?);
            } else {
                r.skip(tag.wire_type)?;
            }
        }
        Ok(())
    }
}

/// `grpc.reflection.v1alpha.FileDescriptorResponse` — field 1
/// `file_descriptor_proto` (repeated bytes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileDescriptorResponse {
    /// Serialized `FileDescriptorProto` messages.
    pub file_descriptor_proto: Vec<Vec<u8>>,
}

impl Message for FileDescriptorResponse {
    fn encode(&self, w: &mut Writer) -> CoreResult<()> {
        for f in &self.file_descriptor_proto {
            scalar::encode_bytes(w, 1, f);
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> CoreResult<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            if tag.field_number == 1 {
                self.file_descriptor_proto.push(r.read_length_delimited()?.to_vec());
            } else {
                r.skip(tag.wire_type)?;
            }
        }
        Ok(())
    }
}

/// `grpc.reflection.v1alpha.AllExtensionNumbersResponse` — field 1
/// `base_type_name` (string), field 2 `extension_number` (repeated int32).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllExtensionNumbersResponse {
    /// The fully-qualified message type the extensions belong to.
    pub base_type_name: String,
    /// The extension field numbers declared on that type.
    pub extension_number: Vec<i32>,
}

impl Message for AllExtensionNumbersResponse {
    fn encode(&self, w: &mut Writer) -> CoreResult<()> {
        if !self.base_type_name.is_empty() {
            scalar::encode_string(w, 1, &self.base_type_name);
        }
        for n in &self.extension_number {
            scalar::encode_int32(w, 2, *n);
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> CoreResult<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match tag.field_number {
                1 => self.base_type_name = r.read_string_owned()?,
                2 => self.extension_number.push(scalar::read_int32(r)?),
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

/// `grpc.reflection.v1alpha.ExtensionRequest` — field 1 `containing_type`
/// (string), field 2 `extension_number` (int32).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtensionRequest {
    /// Fully-qualified message type containing the extension.
    pub containing_type: String,
    /// The extension field number.
    pub extension_number: i32,
}

impl Message for ExtensionRequest {
    fn encode(&self, w: &mut Writer) -> CoreResult<()> {
        if !self.containing_type.is_empty() {
            scalar::encode_string(w, 1, &self.containing_type);
        }
        if self.extension_number != 0 {
            scalar::encode_int32(w, 2, self.extension_number);
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> CoreResult<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match tag.field_number {
                1 => self.containing_type = r.read_string_owned()?,
                2 => self.extension_number = scalar::read_int32(r)?,
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

/// `grpc.reflection.v1alpha.ServerReflectionRequest`.
///
/// The `message_request` oneof is modelled as the union of its individually
/// optional members; exactly one is populated per request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerReflectionRequest {
    /// The host the request is scoped to (optional).
    pub host: String,
    /// File lookup by file name (field 3).
    pub file_by_filename: String,
    /// File lookup by declaring symbol (field 4).
    pub file_containing_symbol: String,
    /// File lookup by extension (field 5, legacy string form).
    pub file_containing_extension_str: String,
    /// File lookup by extension (field 6, structured form).
    pub file_containing_extension: Option<ExtensionRequest>,
    /// All extension numbers of a type (field 7).
    pub all_extension_numbers_of_type: String,
    /// List all services (field 8; presence indicated by the flag).
    pub list_services_marker: bool,
}

impl Message for ServerReflectionRequest {
    fn encode(&self, w: &mut Writer) -> CoreResult<()> {
        if !self.host.is_empty() {
            scalar::encode_string(w, 1, &self.host);
        }
        if !self.file_by_filename.is_empty() {
            scalar::encode_string(w, 3, &self.file_by_filename);
        }
        if !self.file_containing_symbol.is_empty() {
            scalar::encode_string(w, 4, &self.file_containing_symbol);
        }
        if !self.file_containing_extension_str.is_empty() {
            scalar::encode_string(w, 5, &self.file_containing_extension_str);
        }
        if let Some(ext) = &self.file_containing_extension {
            let bytes = ext.encode_to_vec()?;
            scalar::encode_message(w, 6, &bytes);
        }
        if !self.all_extension_numbers_of_type.is_empty() {
            scalar::encode_string(w, 7, &self.all_extension_numbers_of_type);
        }
        if self.list_services_marker {
            // field 8 is an empty marker string in the original proto.
            scalar::encode_string(w, 8, "");
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> CoreResult<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match tag.field_number {
                1 => self.host = r.read_string_owned()?,
                3 => self.file_by_filename = r.read_string_owned()?,
                4 => self.file_containing_symbol = r.read_string_owned()?,
                5 => self.file_containing_extension_str = r.read_string_owned()?,
                6 => {
                    let b = r.read_length_delimited()?;
                    self.file_containing_extension = Some(ExtensionRequest::decode(b)?);
                }
                7 => self.all_extension_numbers_of_type = r.read_string_owned()?,
                8 => {
                    let _ = r.read_string_owned()?;
                    self.list_services_marker = true;
                }
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

/// `grpc.reflection.v1alpha.ServerReflectionResponse`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerReflectionResponse {
    /// The host the response is scoped to.
    pub valid_host: String,
    /// An error response (oneof `message_response`).
    pub error_response: Option<ErrorResponse>,
    /// A list-services response (oneof `message_response`).
    pub list_services_response: Option<ListServiceResponse>,
    /// A file-descriptor response (oneof `message_response`).
    pub file_descriptor_response: Option<FileDescriptorResponse>,
    /// An all-extension-numbers response (oneof `message_response`).
    pub all_extension_numbers_response: Option<AllExtensionNumbersResponse>,
}

impl Message for ServerReflectionResponse {
    fn encode(&self, w: &mut Writer) -> CoreResult<()> {
        if !self.valid_host.is_empty() {
            scalar::encode_string(w, 1, &self.valid_host);
        }
        if let Some(e) = &self.error_response {
            let bytes = e.encode_to_vec()?;
            scalar::encode_message(w, 2, &bytes);
        }
        if let Some(l) = &self.list_services_response {
            let bytes = l.encode_to_vec()?;
            scalar::encode_message(w, 3, &bytes);
        }
        if let Some(f) = &self.file_descriptor_response {
            let bytes = f.encode_to_vec()?;
            scalar::encode_message(w, 4, &bytes);
        }
        if let Some(a) = &self.all_extension_numbers_response {
            let bytes = a.encode_to_vec()?;
            scalar::encode_message(w, 5, &bytes);
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> CoreResult<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match tag.field_number {
                1 => self.valid_host = r.read_string_owned()?,
                2 => {
                    let b = r.read_length_delimited()?;
                    self.error_response = Some(ErrorResponse::decode(b)?);
                }
                3 => {
                    let b = r.read_length_delimited()?;
                    self.list_services_response = Some(ListServiceResponse::decode(b)?);
                }
                4 => {
                    let b = r.read_length_delimited()?;
                    self.file_descriptor_response = Some(FileDescriptorResponse::decode(b)?);
                }
                5 => {
                    let b = r.read_length_delimited()?;
                    self.all_extension_numbers_response =
                        Some(AllExtensionNumbersResponse::decode(b)?);
                }
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

/// The fully-qualified name of a service within its file.
fn service_fqn(file: &FileDescriptorProto, svc: &ServiceDescriptorProto) -> String {
    let pkg = file.package.as_deref().unwrap_or("");
    let name = svc.name.as_deref().unwrap_or("");
    qualify(pkg, name)
}

fn qualify(prefix: &str, name: &str) -> String {
    let prefix = prefix.strip_prefix('.').unwrap_or(prefix);
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

/// A reflection service backed by a [`FileDescriptorSet`].
#[derive(Debug, Clone)]
pub struct ReflectionService {
    set: FileDescriptorSet,
}

impl ReflectionService {
    /// Construct a reflection service from a descriptor set.
    pub fn new(set: FileDescriptorSet) -> Self {
        ReflectionService { set }
    }

    /// The fully-qualified service name.
    pub const SERVICE_NAME: &'static str = "grpc.reflection.v1alpha.ServerReflection";

    /// List all service names declared across the set.
    pub fn list_service_names(&self) -> Vec<String> {
        let mut out = Vec::new();
        for f in &self.set.file {
            for svc in &f.service {
                out.push(service_fqn(f, svc));
            }
        }
        out.sort();
        out
    }

    fn file_bytes_for_symbol(&self, symbol: &str) -> Option<Vec<u8>> {
        let norm = symbol.trim_start_matches('.');
        for f in &self.set.file {
            for svc in &f.service {
                let fqn = service_fqn(f, svc);
                let base = fqn.trim_start_matches('.');
                if base == norm
                    || format!("{base}.{}", svc.name.as_deref().unwrap_or("")).trim_start_matches('.')
                        == norm
                {
                    return f.encode_to_vec().ok();
                }
                for m in &svc.method {
                    if format!("{base}.{}", m.name.as_deref().unwrap_or("")).trim_start_matches('.')
                        == norm
                    {
                        return f.encode_to_vec().ok();
                    }
                }
            }
            if let Some(bytes) = self.file_bytes_for_message_symbol(
                f,
                &f.package.clone().unwrap_or_default(),
                norm,
            ) {
                return Some(bytes);
            }
        }
        None
    }

    fn file_bytes_for_message_symbol(
        &self,
        f: &FileDescriptorProto,
        prefix: &str,
        norm: &str,
    ) -> Option<Vec<u8>> {
        for m in &f.message_type {
            let fqn = qualify(prefix, m.name.as_deref().unwrap_or(""));
            if fqn.trim_start_matches('.') == norm {
                return f.encode_to_vec().ok();
            }
            if let Some(bytes) = self.nested_message_symbol(f, &fqn, m, norm) {
                return Some(bytes);
            }
        }
        for e in &f.enum_type {
            let fqn = qualify(prefix, e.name.as_deref().unwrap_or(""));
            if fqn.trim_start_matches('.') == norm {
                return f.encode_to_vec().ok();
            }
        }
        None
    }

    fn nested_message_symbol(
        &self,
        f: &FileDescriptorProto,
        prefix: &str,
        m: &DescriptorProto,
        norm: &str,
    ) -> Option<Vec<u8>> {
        for n in &m.nested_type {
            let fqn = qualify(prefix, n.name.as_deref().unwrap_or(""));
            if fqn.trim_start_matches('.') == norm {
                return f.encode_to_vec().ok();
            }
            if let Some(bytes) = self.nested_message_symbol(f, &fqn, n, norm) {
                return Some(bytes);
            }
        }
        for e in &m.enum_type {
            let fqn = qualify(prefix, e.name.as_deref().unwrap_or(""));
            if fqn.trim_start_matches('.') == norm {
                return f.encode_to_vec().ok();
            }
        }
        None
    }

    fn all_extension_numbers(&self, type_name: &str) -> Option<AllExtensionNumbersResponse> {
        let norm = type_name.trim_start_matches('.');
        let mut numbers: BTreeSet<i32> = BTreeSet::new();
        let mut found = false;
        for f in &self.set.file {
            for ext in &f.extension {
                if ext
                    .extendee
                    .as_deref()
                    .map(|e| e.trim_start_matches('.') == norm)
                    .unwrap_or(false)
                {
                    if let Some(n) = ext.number {
                        numbers.insert(n);
                        found = true;
                    }
                }
            }
            for m in &f.message_type {
                let prefix = qualify(&f.package.clone().unwrap_or_default(), m.name.as_deref().unwrap_or(""));
                if let Some(b) = self.nested_extensions(f, &prefix, m, norm, &mut numbers) {
                    found = found || b;
                }
            }
        }
        if !found {
            return None;
        }
        Some(AllExtensionNumbersResponse {
            base_type_name: norm.to_string(),
            extension_number: numbers.into_iter().collect(),
        })
    }

    #[allow(clippy::only_used_in_recursion)]
    fn nested_extensions(
        &self,
        f: &FileDescriptorProto,
        prefix: &str,
        m: &DescriptorProto,
        norm: &str,
        numbers: &mut BTreeSet<i32>,
    ) -> Option<bool> {
        let mut found = false;
        for ext in &m.extension {
            if ext
                .extendee
                .as_deref()
                .map(|e| e.trim_start_matches('.') == norm)
                .unwrap_or(false)
            {
                if let Some(n) = ext.number {
                    numbers.insert(n);
                    found = true;
                }
            }
        }
        for n in &m.nested_type {
            let fqn = qualify(prefix, n.name.as_deref().unwrap_or(""));
            if let Some(b) = self.nested_extensions(f, &fqn, n, norm, numbers) {
                found = found || b;
            }
        }
        Some(found)
    }

    /// Handle a single reflection request, returning the response(s).
    pub fn handle(&self, req: &ServerReflectionRequest) -> Vec<ServerReflectionResponse> {
        let host = req.host.clone();
        let err = |code: i32, msg: String| {
            vec![ServerReflectionResponse {
                valid_host: host.clone(),
                error_response: Some(ErrorResponse {
                    error_code: code,
                    error_message: msg,
                }),
                list_services_response: None,
                file_descriptor_response: None,
                all_extension_numbers_response: None,
            }]
        };

        if req.list_services_marker {
            let names = self.list_service_names();
            return vec![ServerReflectionResponse {
                valid_host: host,
                error_response: None,
                list_services_response: Some(ListServiceResponse {
                    service: names.into_iter().map(|n| ServiceResponse { name: n }).collect(),
                }),
                file_descriptor_response: None,
                all_extension_numbers_response: None,
            }];
        }

        if !req.file_by_filename.is_empty() {
            if let Some(f) = self
                .set
                .file
                .iter()
                .find(|f| f.name.as_deref() == Some(&req.file_by_filename))
            {
                return ok_file(host, f);
            }
            return err(5, format!("file not found: {}", req.file_by_filename));
        }

        if !req.file_containing_symbol.is_empty() {
            if let Some(bytes) = self.file_bytes_for_symbol(&req.file_containing_symbol) {
                return single_file(host, bytes);
            }
            return err(5, format!("symbol not found: {}", req.file_containing_symbol));
        }

        if !req.file_containing_extension_str.is_empty() || req.file_containing_extension.is_some() {
            let type_name = req
                .file_containing_extension
                .as_ref()
                .map(|e| e.containing_type.clone())
                .unwrap_or_else(|| req.file_containing_extension_str.clone());
            if let Some(bytes) = self.file_bytes_for_symbol_query(&type_name) {
                return single_file(host, bytes);
            }
            return err(5, format!("extension host not found: {type_name}"));
        }

        if !req.all_extension_numbers_of_type.is_empty() {
            if let Some(resp) = self.all_extension_numbers(&req.all_extension_numbers_of_type) {
                return vec![ServerReflectionResponse {
                    valid_host: host,
                    error_response: None,
                    list_services_response: None,
                    file_descriptor_response: None,
                    all_extension_numbers_response: Some(resp),
                }];
            }
            return err(5, format!("type not found: {}", req.all_extension_numbers_of_type));
        }

        err(3, "no recognized reflection request".to_string())
    }

    /// Resolve a symbol to the declaring file's serialized bytes (shared by the
    /// symbol and extension-host queries).
    fn file_bytes_for_symbol_query(&self, symbol: &str) -> Option<Vec<u8>> {
        self.file_bytes_for_symbol(symbol)
    }
}

#[async_trait::async_trait]
impl ServiceHandler for ReflectionService {
    fn full_name(&self) -> &str {
        Self::SERVICE_NAME
    }

    fn methods(&self) -> Vec<(String, MethodKind)> {
        vec![(
            format!("/{}/ServerReflectionInfo", Self::SERVICE_NAME),
            MethodKind::BidiStreaming,
        )]
    }

    async fn call_bidi_streaming(
        &self,
        _method: &str,
        _ctx: RpcContext,
        req: ClientStream<Vec<u8>>,
    ) -> Result<ServerStream<Vec<u8>>, Status> {
        let this = self.clone();
        let (mut tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            let mut input = req;
            while let Some(item) = input.next().await {
                let raw = match item {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };
                let req = match ServerReflectionRequest::decode(&raw) {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx.send(Err(Status::new(Code::Internal, e.to_string())));
                        continue;
                    }
                };
                for resp in this.handle(&req) {
                    let bytes = match resp.encode_to_vec() {
                        Ok(b) => b,
                        Err(e) => {
                            let _ = tx.send(Err(Status::new(Code::Internal, e.to_string())));
                            continue;
                        }
                    };
                    if tx.send(Ok(bytes)).await.is_err() {
                        return;
                    }
                }
            }
        });
        Ok(Box::pin(rx) as ServerStream<Vec<u8>>)
    }

    async fn call_unary(
        &self,
        method: &str,
        _ctx: RpcContext,
        _req: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        Err(Status::new(
            Code::Unimplemented,
            format!("reflection method {method} is not unary"),
        ))
    }

    async fn call_server_streaming(
        &self,
        method: &str,
        _ctx: RpcContext,
        _req: Vec<u8>,
    ) -> Result<ServerStream<Vec<u8>>, Status> {
        Err(Status::new(
            Code::Unimplemented,
            format!("reflection method {method} is not server-streaming"),
        ))
    }

    async fn call_client_streaming(
        &self,
        method: &str,
        _ctx: RpcContext,
        _req: ClientStream<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        Err(Status::new(
            Code::Unimplemented,
            format!("reflection method {method} is not client-streaming"),
        ))
    }
}

fn single_file(host: String, bytes: Vec<u8>) -> Vec<ServerReflectionResponse> {
    vec![ServerReflectionResponse {
        valid_host: host,
        error_response: None,
        list_services_response: None,
        file_descriptor_response: Some(FileDescriptorResponse {
            file_descriptor_proto: vec![bytes],
        }),
        all_extension_numbers_response: None,
    }]
}

fn ok_file(host: String, f: &FileDescriptorProto) -> Vec<ServerReflectionResponse> {
    match f.encode_to_vec() {
        Ok(bytes) => single_file(host, bytes),
        Err(_) => vec![ServerReflectionResponse {
            valid_host: host,
            error_response: Some(ErrorResponse {
                error_code: 13,
                error_message: "failed to encode descriptor".into(),
            }),
            list_services_response: None,
            file_descriptor_response: None,
            all_extension_numbers_response: None,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_proto_compiler::compile;
    use tpt_proto_language::parse_file;

    const SRC: &str = r#"
syntax = "proto3";
package ex;

message Person {
  string name = 1;
  message Address { string city = 1; }
}

enum Color { RED = 0; GREEN = 1; }

service UserService {
  rpc GetUser(Person) returns (Person);
}
"#;

    fn service() -> ReflectionService {
        let parsed = parse_file("ex.proto", SRC);
        assert!(!parsed.diagnostics.has_errors());
        let (fd, diags) = compile(&parsed.file);
        assert!(
            !diags.has_errors(),
            "diags: {:?}",
            diags.iter().collect::<Vec<_>>()
        );
        let set = FileDescriptorSet { file: vec![fd] };
        ReflectionService::new(set)
    }

    #[test]
    fn request_messages_roundtrip() {
        let mut req = ServerReflectionRequest {
            host: "localhost".into(),
            list_services_marker: true,
            ..Default::default()
        };
        let bytes = req.encode_to_vec().unwrap();
        req = ServerReflectionRequest::decode(&bytes).unwrap();
        assert!(req.list_services_marker);
        assert_eq!(req.host, "localhost");

        let resp = ServerReflectionResponse {
            valid_host: "h".into(),
            file_descriptor_response: Some(FileDescriptorResponse {
                file_descriptor_proto: vec![vec![1, 2, 3]],
            }),
            ..Default::default()
        };
        let rbytes = resp.encode_to_vec().unwrap();
        let rback = ServerReflectionResponse::decode(&rbytes).unwrap();
        assert_eq!(
            rback.file_descriptor_response.unwrap().file_descriptor_proto,
            vec![vec![1, 2, 3]]
        );
    }

    #[test]
    fn lists_services() {
        let svc = service();
        let names = svc.list_service_names();
        assert_eq!(names, vec!["ex.UserService".to_string()]);

        let req = ServerReflectionRequest {
            list_services_marker: true,
            ..Default::default()
        };
        let res = svc.handle(&req);
        assert_eq!(res.len(), 1);
        let ls = res[0].list_services_response.as_ref().unwrap();
        assert_eq!(ls.service[0].name, "ex.UserService");
    }

    #[test]
    fn file_by_filename() {
        let svc = service();
        let req = ServerReflectionRequest {
            file_by_filename: "ex.proto".into(),
            ..Default::default()
        };
        let res = svc.handle(&req);
        assert!(res[0].file_descriptor_response.is_some());
        let bytes = &res[0]
            .file_descriptor_response
            .as_ref()
            .unwrap()
            .file_descriptor_proto[0];
        let fd = FileDescriptorProto::decode(bytes).unwrap();
        assert_eq!(fd.name.as_deref(), Some("ex.proto"));
    }

    #[test]
    fn file_containing_symbol_message() {
        let svc = service();
        let req = ServerReflectionRequest {
            file_containing_symbol: "ex.Person.Address".into(),
            ..Default::default()
        };
        let res = svc.handle(&req);
        assert!(
            res[0].file_descriptor_response.is_some(),
            "expected descriptor for nested symbol"
        );
    }

    #[test]
    fn file_containing_symbol_unknown_errors() {
        let svc = service();
        let req = ServerReflectionRequest {
            file_containing_symbol: "ex.Nope".into(),
            ..Default::default()
        };
        let res = svc.handle(&req);
        assert_eq!(res[0].error_response.as_ref().unwrap().error_code, 5);
    }

    #[test]
    fn all_extension_numbers_unknown() {
        let svc = service();
        let req = ServerReflectionRequest {
            all_extension_numbers_of_type: "ex.Person".into(),
            ..Default::default()
        };
        let res = svc.handle(&req);
        assert_eq!(res[0].error_response.as_ref().unwrap().error_code, 5);
    }
}
