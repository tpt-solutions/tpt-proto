//! Integration tests for Phase 15 server features: observability (metrics),
//! security (auth/authz), the health-checking protocol, and server reflection.

use std::sync::Arc;

use futures::stream;
use futures::StreamExt;
use tpt_proto_core::Message;
use tpt_proto_grpc::health::{
    HealthCheckRequest, HealthCheckResponse, HealthRegistry, HealthService, ServingStatus,
};
use tpt_proto_grpc::metadata::Metadata;
use tpt_proto_grpc::observability::{InMemoryMetricsRecorder, Observability};
use tpt_proto_grpc::reflection::{ReflectionService, ServerReflectionRequest};
use tpt_proto_grpc::security::{
    BearerTokenAuthenticator, SecurityPolicy,
};
use tpt_proto_grpc::transport::Transport;
use tpt_proto_grpc::{H2Transport, Server, ServerStream};
use tpt_proto_descriptor::{FileDescriptorProto, FileDescriptorSet, ServiceDescriptorProto};

async fn spawn_server(server: Server) -> (std::net::SocketAddr, tpt_proto_grpc::CancellationToken) {
    use tokio::net::TcpListener;
    let shutdown = server.shutdown_token();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = server.serve_listener(listener).await;
    });
    (addr, shutdown)
}

#[tokio::test]
async fn observability_records_metrics_for_health_check() {
    let metrics = InMemoryMetricsRecorder::new();
    let obs = Observability::default().with_metrics(Arc::new(metrics.clone()));

    let mut registry = HealthRegistry::new();
    registry.set_status("", ServingStatus::Serving);
    let server = Server::builder()
        .with_observability(obs)
        .build();
    server.add_service(HealthService::new(registry));

    let (addr, shutdown) = spawn_server(server).await;
    let transport = H2Transport::new(addr.to_string());

    let req = HealthCheckRequest {
        service: String::new(),
    };
    let bytes = req.encode_to_vec().unwrap();
    eprintln!("TEST: calling unary");
    let (resp, _trailers) = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        transport.unary("/grpc.health.v1.Health/Check", Metadata::new(), bytes),
    )
    .await
    .expect("unary did not time out")
    .expect("check succeeded");
    eprintln!("TEST: got response");
    let resp = HealthCheckResponse::decode(&resp).unwrap();
    assert_eq!(resp.status, ServingStatus::Serving);

    assert!(metrics.total_calls_started() >= 1);
    let snap = metrics.snapshot();
    let labels: Vec<_> = snap.keys().cloned().collect();
    assert!(
        labels.iter().any(|l| l.service == "grpc.health.v1.Health" && l.method == "Check"),
        "expected metrics labelled for health Check, got {labels:?}"
    );
    shutdown.cancel();
}

#[tokio::test]
async fn security_policy_enforces_authentication() {
    let policy = SecurityPolicy::none()
        .with_authenticator(Arc::new(BearerTokenAuthenticator::new("secret", "alice")));
    let server = Server::builder()
        .with_security(Arc::new(policy))
        .build();
    let mut registry = HealthRegistry::new();
    registry.set_status("", ServingStatus::Serving);
    server.add_service(HealthService::new(registry));
    let (addr, shutdown) = spawn_server(server).await;

    let transport = H2Transport::new(addr.to_string());

    // No token -> UNAUTHENTICATED.
    let req = HealthCheckRequest {
        service: String::new(),
    };
    let bytes = req.encode_to_vec().unwrap();
    let err = transport
        .unary("/grpc.health.v1.Health/Check", Metadata::new(), bytes)
        .await
        .unwrap_err();
    assert_eq!(err.code, tpt_proto_grpc::Code::Unauthenticated);

    // Valid token -> passes.
    let mut md = Metadata::new();
    md.insert_text("authorization", "Bearer secret").unwrap();
    let (resp, _trailers) = transport
        .unary("/grpc.health.v1.Health/Check", md, req.encode_to_vec().unwrap())
        .await
        .expect("authenticated check succeeds");
    let resp = HealthCheckResponse::decode(&resp).unwrap();
    assert_eq!(resp.status, ServingStatus::Serving);
    shutdown.cancel();
}

#[tokio::test]
async fn reflection_lists_registered_services() {
    // Build a tiny descriptor set describing one service.
    let svc = ServiceDescriptorProto {
        name: Some("UserService".to_string()),
        ..Default::default()
    };
    let file = FileDescriptorProto {
        name: Some("ex.proto".to_string()),
        package: Some("ex".to_string()),
        service: vec![svc],
        ..Default::default()
    };
    let set = FileDescriptorSet {
        file: vec![file],
    };

    let server = Server::builder().build();
    server.add_service(ReflectionService::new(set));
    let (addr, shutdown) = spawn_server(server).await;

    let transport = H2Transport::new(addr.to_string());
    let req = ServerReflectionRequest {
        list_services_marker: true,
        ..Default::default()
    };
    let bytes = req.encode_to_vec().unwrap();
    let stream: ServerStream<Vec<u8>> =
        Box::pin(stream::iter(vec![Ok::<_, tpt_proto_grpc::Status>(bytes)]));
    let (mut resp_stream, _trailers) = transport
        .bidi_streaming(
            "/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo",
            Metadata::new(),
            stream,
        )
        .await
        .expect("reflection call");

    let first = resp_stream.next().await.unwrap().expect("response message");
    let resp = tpt_proto_grpc::reflection::ServerReflectionResponse::decode(&first).unwrap();
    let names: Vec<String> = resp
        .list_services_response
        .unwrap()
        .service
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(names, vec!["ex.UserService".to_string()]);
    shutdown.cancel();
}
