#![no_main]
use libfuzzer_sys::fuzz_target;
use tpt_proto_fuzz::targets::dynamic_decoder;

fuzz_target!(|data: &[u8]| {
    dynamic_decoder(data);
});
