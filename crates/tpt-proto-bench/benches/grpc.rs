//! gRPC wire benchmarks (Phase 18).
//!
//! The full server/client *runtime* benchmarks (unary throughput, streaming,
//! many concurrent streams, cancellation/deadline storms, TLS overhead) require
//! the Phase 14 server runtime, which is not yet complete. This suite covers the
//! pieces that are fully implemented and self-contained: message framing and
//! compression (the hot path every gRPC call shares).

use tpt_proto_bench::bench;
use tpt_proto_grpc::{decode_message, encode_message, Compression};

fn payload(size: usize) -> Vec<u8> {
    // Pseudo-random but deterministic content so gzip has something to compress.
    (0..size).map(|i| ((i * 31 + 7) % 251) as u8).collect()
}

fn main() {
    println!("=== grpc framing + compression ===");

    for size in [1usize, 64, 1024, 64 * 1024, 512 * 1024] {
        let msg = payload(size);
        let frame = encode_message(&msg, Compression::Identity, usize::MAX).unwrap();
        let msg_c = msg.clone();
        bench(
            &format!("grpc/frame/encode_identity/{size}"),
            size as u64,
            50_000,
            || encode_message(&msg_c, Compression::Identity, usize::MAX).unwrap(),
        );
        let frame_c = frame.clone();
        bench(
            &format!("grpc/frame/decode_identity/{size}"),
            size as u64,
            50_000,
            || decode_message(&frame_c, Compression::Identity, usize::MAX).unwrap(),
        );
        bench(
            &format!("grpc/frame/roundtrip_identity/{size}"),
            size as u64,
            50_000,
            || {
                let f = encode_message(&msg_c, Compression::Identity, usize::MAX).unwrap();
                decode_message(&f, Compression::Identity, usize::MAX).unwrap()
            },
        );

        // Compression overhead: gzip encode/decode vs identity for the same payload.
        let gz = encode_message(&msg, Compression::Gzip, usize::MAX).unwrap();
        let msg_c = msg.clone();
        bench(
            &format!("grpc/frame/encode_gzip/{size}"),
            size as u64,
            5_000,
            || encode_message(&msg_c, Compression::Gzip, usize::MAX).unwrap(),
        );
        let gz_c = gz.clone();
        bench(
            &format!("grpc/frame/decode_gzip/{size}"),
            size as u64,
            5_000,
            || decode_message(&gz_c, Compression::Gzip, usize::MAX).unwrap(),
        );
        let frame_ratio = gz.len() as f64 / size.max(1) as f64;
        println!("    (gzip frame size {}/{} = {:.2}x payload)", gz.len(), size, frame_ratio);
    }

    println!(
        "\nNOTE: unary/streaming throughput, concurrent-stream, cancellation/deadline storm,\n\
         and TLS-overhead benchmarks require the Phase 14 server runtime and are tracked\n\
         separately (see docs/performance.md)."
    );
}
