#![no_main]
use libfuzzer_sys::fuzz_target;
use tpt_proto_fuzz::targets::text_parser;

fuzz_target!(|data: &[u8]| {
    text_parser(data);
});
