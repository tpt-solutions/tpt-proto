#![no_main]
use libfuzzer_sys::fuzz_target;
use tpt_proto_fuzz::targets::binary_decoder;

fuzz_target!(|data: &[u8]| {
    binary_decoder(data);
});
