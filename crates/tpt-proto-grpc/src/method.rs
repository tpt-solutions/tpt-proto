//! Service and method model derived from descriptors.
//!
//! gRPC supports four method kinds depending on whether the request and/or
//! response streams. These types mirror that classification and are produced
//! from compiled [`ServiceDescriptorProto`] values.

use tpt_proto_descriptor::{FileDescriptorProto, MethodDescriptorProto, ServiceDescriptorProto};

/// The four gRPC method kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    /// Single request, single response.
    Unary,
    /// Single request, streaming response.
    ServerStreaming,
    /// Streaming request, single response.
    ClientStreaming,
    /// Streaming request, streaming response.
    BidiStreaming,
}

impl MethodKind {
    /// Classify a method from its descriptor streaming flags.
    pub fn from_descriptor(m: &MethodDescriptorProto) -> MethodKind {
        let client = m.client_streaming.unwrap_or(false);
        let server = m.server_streaming.unwrap_or(false);
        match (client, server) {
            (false, false) => MethodKind::Unary,
            (false, true) => MethodKind::ServerStreaming,
            (true, false) => MethodKind::ClientStreaming,
            (true, true) => MethodKind::BidiStreaming,
        }
    }

    /// Whether the request side streams.
    pub fn client_streaming(self) -> bool {
        matches!(self, MethodKind::ClientStreaming | MethodKind::BidiStreaming)
    }

    /// Whether the response side streams.
    pub fn server_streaming(self) -> bool {
        matches!(self, MethodKind::ServerStreaming | MethodKind::BidiStreaming)
    }
}

/// A resolved gRPC method.
#[derive(Debug, Clone)]
pub struct Method {
    /// Method name (e.g. `GetUser`).
    pub name: String,
    /// Method kind.
    pub kind: MethodKind,
    /// Fully-qualified request type (e.g. `.example.GetUserRequest`).
    pub input_type: String,
    /// Fully-qualified response type (e.g. `.example.User`).
    pub output_type: String,
    /// The full request path `/package.Service/Method`.
    pub full_path: String,
}

impl Method {
    /// Whether the request side of this method streams.
    pub fn client_streaming(&self) -> bool {
        self.kind.client_streaming()
    }

    /// Whether the response side of this method streams.
    pub fn server_streaming(&self) -> bool {
        self.kind.server_streaming()
    }
}

/// A resolved gRPC service.
#[derive(Debug, Clone)]
pub struct Service {
    /// Service name (e.g. `UserService`).
    pub name: String,
    /// Fully-qualified service name (e.g. `example.UserService`).
    pub full_name: String,
    /// The methods exposed by the service.
    pub methods: Vec<Method>,
}

impl Service {
    /// Look up a method by its (simple) name.
    pub fn method(&self, name: &str) -> Option<&Method> {
        self.methods.iter().find(|m| m.name == name)
    }

    /// Look up a method by its full request path.
    pub fn method_by_path(&self, path: &str) -> Option<&Method> {
        self.methods.iter().find(|m| m.full_path == path)
    }
}

/// Build a [`Service`] model from a file descriptor and a service descriptor.
pub fn build_service(file: &FileDescriptorProto, svc: &ServiceDescriptorProto) -> Service {
    let pkg = file.package.as_deref().unwrap_or("");
    let name = svc.name.clone().unwrap_or_default();
    let full_name = if pkg.is_empty() {
        name.clone()
    } else {
        format!("{pkg}.{name}")
    };
    let methods = svc
        .method
        .iter()
        .map(|m| {
            let mname = m.name.clone().unwrap_or_default();
            Method {
                name: mname.clone(),
                kind: MethodKind::from_descriptor(m),
                input_type: m.input_type.clone().unwrap_or_default(),
                output_type: m.output_type.clone().unwrap_or_default(),
                full_path: format!("/{full_name}/{mname}"),
            }
        })
        .collect();
    Service {
        name,
        full_name,
        methods,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_proto_descriptor::{FileDescriptorProto, MethodDescriptorProto, ServiceDescriptorProto};

    fn method(name: &str, cs: bool, ss: bool) -> MethodDescriptorProto {
        MethodDescriptorProto {
            name: Some(name.into()),
            input_type: Some(".ex.Req".into()),
            output_type: Some(".ex.Res".into()),
            options: None,
            client_streaming: Some(cs),
            server_streaming: Some(ss),
        }
    }

    #[test]
    fn classifies_kinds() {
        assert_eq!(
            MethodKind::from_descriptor(&method("u", false, false)),
            MethodKind::Unary
        );
        assert_eq!(
            MethodKind::from_descriptor(&method("s", false, true)),
            MethodKind::ServerStreaming
        );
        assert_eq!(
            MethodKind::from_descriptor(&method("c", true, false)),
            MethodKind::ClientStreaming
        );
        assert_eq!(
            MethodKind::from_descriptor(&method("b", true, true)),
            MethodKind::BidiStreaming
        );
    }

    #[test]
    fn builds_service_with_paths() {
        let file = FileDescriptorProto {
            package: Some("example".into()),
            ..Default::default()
        };
        let svc = ServiceDescriptorProto {
            name: Some("UserService".into()),
            method: vec![method("GetUser", false, false), method("Watch", false, true)],
            options: None,
        };
        let s = build_service(&file, &svc);
        assert_eq!(s.full_name, "example.UserService");
        assert_eq!(s.methods.len(), 2);
        assert_eq!(s.methods[0].full_path, "/example.UserService/GetUser");
        assert!(!s.methods[0].server_streaming());
        assert!(s.methods[1].server_streaming());
        assert!(s.method_by_path("/example.UserService/Watch").is_some());
    }
}
