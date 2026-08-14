//! gRPC health checking protocol (§13, addendum §13).
//!
//! Implements the `grpc.health.v1.Health` service: a thread-safe
//! [`HealthRegistry`] tracking per-service and overall serving status, the
//! request/response message types, and a [`HealthService`] handler the server
//! runtime drives for the unary `Check` and server-streaming `Watch` methods.
//!
//! Status values mirror the wire enum: `UNKNOWN=0`, `SERVING=1`,
//! `NOT_SERVING=2`, `SERVICE_UNKNOWN=3`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use futures::StreamExt;
use tpt_proto_core::{Message, Reader, Result as CoreResult, WireType, Writer};
use tpt_proto_core::scalar;

use crate::context::{Request, Response};
use crate::status::{Code, Status};
use crate::transport::ServerStream;

/// The serving status of a service, as carried on the wire (`int32`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ServingStatus {
    /// The entity is not known to the server (e.g. never registered).
    Unknown = 0,
    /// The entity is serving and able to handle requests.
    Serving = 1,
    /// The entity exists but is not currently serving.
    NotServing = 2,
    /// The requested service name is not known to the server.
    ServiceUnknown = 3,
}

impl ServingStatus {
    /// Parse the wire `int32` value, defaulting to [`ServingStatus::Unknown`].
    pub fn from_i32(v: i32) -> ServingStatus {
        match v {
            0 => ServingStatus::Unknown,
            1 => ServingStatus::Serving,
            2 => ServingStatus::NotServing,
            3 => ServingStatus::ServiceUnknown,
            _ => ServingStatus::Unknown,
        }
    }

    /// The wire `int32` value.
    pub fn as_i32(self) -> i32 {
        self as i32
    }

    /// The canonical name used in logs and the CLI.
    pub fn as_str(self) -> &'static str {
        match self {
            ServingStatus::Unknown => "UNKNOWN",
            ServingStatus::Serving => "SERVING",
            ServingStatus::NotServing => "NOT_SERVING",
            ServingStatus::ServiceUnknown => "SERVICE_UNKNOWN",
        }
    }
}

/// `grpc.health.v1.HealthCheckRequest` — field 1 `service` (string).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HealthCheckRequest {
    /// The service to query. Empty string means the overall server health.
    pub service: String,
}

impl Message for HealthCheckRequest {
    fn encode(&self, w: &mut Writer) -> CoreResult<()> {
        if !self.service.is_empty() {
            scalar::encode_string(w, 1, &self.service);
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> CoreResult<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            if tag.field_number == 1 {
                self.service = r.read_string_owned()?;
            } else {
                r.skip(tag.wire_type)?;
            }
        }
        Ok(())
    }
}

/// `grpc.health.v1.HealthCheckResponse` — field 1 `status` (int32).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthCheckResponse {
    /// The serving status.
    pub status: ServingStatus,
}

impl Default for HealthCheckResponse {
    fn default() -> Self {
        HealthCheckResponse {
            status: ServingStatus::Unknown,
        }
    }
}

impl Message for HealthCheckResponse {
    fn encode(&self, w: &mut Writer) -> CoreResult<()> {
        scalar::encode_int32(w, 1, self.status.as_i32());
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> CoreResult<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            if tag.field_number == 1 {
                self.status = ServingStatus::from_i32(scalar::read_int32(r)?);
            } else {
                r.skip(tag.wire_type)?;
            }
        }
        Ok(())
    }
}

type WatchSender = tokio::sync::broadcast::Sender<(String, ServingStatus)>;

/// A thread-safe registry of serving status, with per-service and overall
/// aggregation and change subscriptions for `Watch`.
#[derive(Debug, Clone)]
pub struct HealthRegistry {
    inner: Arc<Mutex<HashMap<String, ServingStatus>>>,
    watchers: Arc<Mutex<Option<WatchSender>>>,
}

impl Default for HealthRegistry {
    fn default() -> Self {
        HealthRegistry::new()
    }
}

impl HealthRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        HealthRegistry {
            inner: Arc::new(Mutex::new(HashMap::new())),
            watchers: Arc::new(Mutex::new(None)),
        }
    }

    /// Set the status of a named service. Use `""` for the overall server.
    pub fn set_status(&self, service: &str, status: ServingStatus) {
        {
            let mut map = self.inner.lock().unwrap();
            map.insert(service.to_string(), status);
        }
        self.broadcast(service, status);
    }

    /// Read the status of a named service, aggregating for the overall (`""`)
    /// query: if any registered service is `NOT_SERVING`, the overall is
    /// `NOT_SERVING`; if no service is registered the overall is `UNKNOWN`;
    /// otherwise `SERVING`.
    pub fn get_status(&self, service: &str) -> ServingStatus {
        let map = self.inner.lock().unwrap();
        if service.is_empty() {
            if map.is_empty() {
                return ServingStatus::Unknown;
            }
            let mut has_serving = false;
            for s in map.values() {
                if *s == ServingStatus::NotServing {
                    return ServingStatus::NotServing;
                }
                if *s == ServingStatus::Serving {
                    has_serving = true;
                }
            }
            if has_serving {
                ServingStatus::Serving
            } else {
                ServingStatus::Unknown
            }
        } else {
            map.get(service).copied().unwrap_or(ServingStatus::Unknown)
        }
    }

    /// `Check` semantics: like [`get_status`](HealthRegistry::get_status), but
    /// an unknown service name returns [`ServingStatus::ServiceUnknown`] (which
    /// the handler maps to a `NOT_FOUND` gRPC status).
    pub fn check(&self, service: &str) -> ServingStatus {
        if service.is_empty() {
            return self.get_status("");
        }
        let map = self.inner.lock().unwrap();
        map.get(service)
            .copied()
            .unwrap_or(ServingStatus::ServiceUnknown)
    }

    /// Subscribe to status changes. The returned receiver yields
    /// `(service, status)` tuples on every [`set_status`](HealthRegistry::set_status)
    /// call. The server runtime converts these into `Watch` responses.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<(String, ServingStatus)> {
        let mut w = self.watchers.lock().unwrap();
        let tx = w.get_or_insert_with(|| tokio::sync::broadcast::channel(64).0);
        tx.subscribe()
    }

    fn broadcast(&self, service: &str, status: ServingStatus) {
        if let Some(tx) = self.watchers.lock().unwrap().as_ref() {
            // Ignore send errors: no active subscribers.
            let _ = tx.send((service.to_string(), status));
        }
    }

    /// All registered service names (excluding the synthetic overall `""`).
    pub fn services(&self) -> Vec<String> {
        let map = self.inner.lock().unwrap();
        map.keys().filter(|k| !k.is_empty()).cloned().collect()
    }
}

/// The `grpc.health.v1.Health` service handler.
#[derive(Clone)]
pub struct HealthService {
    registry: HealthRegistry,
}

impl HealthService {
    /// Construct a health service backed by the given registry.
    pub fn new(registry: HealthRegistry) -> Self {
        HealthService { registry }
    }

    /// The service's fully-qualified name.
    pub const SERVICE_NAME: &'static str = "grpc.health.v1.Health";

    /// Handle a unary `Check` request.
    ///
    /// Returns the current status. If the service is unknown
    /// ([`ServingStatus::ServiceUnknown`]) the call fails with
    /// [`Code::NotFound`].
    pub async fn check(
        &self,
        req: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let service = req.message.service.clone();
        let status = self.registry.check(&service);
        if status == ServingStatus::ServiceUnknown {
            return Err(Status::new(
                Code::NotFound,
                format!("unknown service {service:?}"),
            ));
        }
        Ok(Response::new(HealthCheckResponse { status }))
    }

    /// Handle a server-streaming `Watch` request, yielding a response whenever
    /// the service's status changes (and once immediately with the current
    /// status). The returned stream ends when the registry is dropped.
    pub async fn watch(
        &self,
        req: Request<HealthCheckRequest>,
    ) -> Result<ServerStream<HealthCheckResponse>, Status> {
        let service = req.message.service.clone();
        let registry = self.registry.clone();
        let rx = self.registry.subscribe();
        let current = registry.check(&service);
        let stream = stream::unfold(
            (Some(current), rx, service),
            |(pending, mut rx, service)| async move {
                if let Some(status) = pending {
                    return Some((
                        Ok(HealthCheckResponse { status }),
                        (None, rx, service),
                    ));
                }
                loop {
                    match rx.recv().await {
                        Ok((svc, status)) if svc == service || svc.is_empty() => {
                            return Some((
                                Ok(HealthCheckResponse { status }),
                                (None, rx, service),
                            ));
                        }
                        Ok(_) => continue,
                        Err(_) => return None,
                    }
                }
            },
        );
        Ok(Box::pin(stream) as ServerStream<HealthCheckResponse>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::RpcContext;

    #[test]
    fn status_wire_roundtrip() {
        for v in [0, 1, 2, 3] {
            assert_eq!(ServingStatus::from_i32(v).as_i32(), v);
        }
        assert_eq!(ServingStatus::from_i32(99), ServingStatus::Unknown);
    }

    #[test]
    fn check_request_roundtrip() {
        let req = HealthCheckRequest {
            service: "ex.Svc".into(),
        };
        let bytes = req.encode_to_vec().unwrap();
        let back = HealthCheckRequest::decode(&bytes).unwrap();
        assert_eq!(back.service, "ex.Svc");
    }

    #[test]
    fn check_response_roundtrip() {
        let resp = HealthCheckResponse {
            status: ServingStatus::Serving,
        };
        let bytes = resp.encode_to_vec().unwrap();
        let back = HealthCheckResponse::decode(&bytes).unwrap();
        assert_eq!(back.status, ServingStatus::Serving);
    }

    #[test]
    fn overall_aggregates_not_serving() {
        let reg = HealthRegistry::new();
        assert_eq!(reg.get_status(""), ServingStatus::Unknown);
        reg.set_status("a", ServingStatus::Serving);
        assert_eq!(reg.get_status(""), ServingStatus::Serving);
        reg.set_status("b", ServingStatus::NotServing);
        assert_eq!(reg.get_status(""), ServingStatus::NotServing);
    }

    #[test]
    fn check_returns_service_unknown_for_missing() {
        let reg = HealthRegistry::new();
        reg.set_status("known", ServingStatus::Serving);
        assert_eq!(reg.check("known"), ServingStatus::Serving);
        assert_eq!(reg.check("missing"), ServingStatus::ServiceUnknown);
    }

    #[tokio::test]
    async fn check_handler_maps_unknown_to_not_found() {
        let reg = HealthRegistry::new();
        let svc = HealthService::new(reg);
        let req = Request::with_context(
            HealthCheckRequest {
                service: "ghost".into(),
            },
            RpcContext::new(),
        );
        let res = svc.check(req).await;
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, Code::NotFound);
    }

    #[tokio::test]
    async fn watch_emits_current_then_changes() {
        let reg = HealthRegistry::new();
        reg.set_status("svc", ServingStatus::Serving);
        let svc = HealthService::new(reg.clone());
        let req = Request::with_context(
            HealthCheckRequest {
                service: "svc".into(),
            },
            RpcContext::new(),
        );
        let mut stream = svc.watch(req).await.unwrap();
        let first = stream.next().await.unwrap().unwrap();
        assert_eq!(first.status, ServingStatus::Serving);
        reg.set_status("svc", ServingStatus::NotServing);
        let second = stream.next().await.unwrap().unwrap();
        assert_eq!(second.status, ServingStatus::NotServing);
    }
}
