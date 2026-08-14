#![no_main]
use libfuzzer_sys::fuzz_target;
use tpt_proto_fuzz::targets::descriptor_decoder;

fuzz_target!(|data: &[u8]| {
    descriptor_decoder(data);
});
