#![no_main]
use libfuzzer_sys::fuzz_target;
use tpt_proto_fuzz::targets::grpc_metadata;

fuzz_target!(|data: &[u8]| {
    grpc_metadata(data);
});