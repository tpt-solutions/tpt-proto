#![no_main]
use libfuzzer_sys::fuzz_target;
use tpt_proto_fuzz::targets::json_decoder;

fuzz_target!(|data: &[u8]| {
    json_decoder(data);
});
