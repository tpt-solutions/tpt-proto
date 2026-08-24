#![no_main]
use libfuzzer_sys::fuzz_target;
use tpt_proto_fuzz::targets::codec_decode_message;

fuzz_target!(|data: &[u8]| {
    codec_decode_message(data);
});