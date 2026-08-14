#![no_main]
use libfuzzer_sys::fuzz_target;
use tpt_proto_fuzz::targets::language_parser;

fuzz_target!(|data: &[u8]| {
    language_parser(data);
});
