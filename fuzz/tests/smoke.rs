//! Smoke regression test: the fuzz targets must not panic on a representative
//! set of inputs (valid encodings, empty input, and adversarial garbage).
//!
//! Run with `cargo test` from the `fuzz/` directory.

use tpt_proto_fuzz::harness::run_all_catch;

#[test]
fn empty_input_does_not_crash() {
    assert!(run_all_catch(&[]).is_empty());
}

#[test]
fn arbitrary_garbage_does_not_crash() {
    let samples: &[&[u8]] = &[
        &[0u8; 1],
        &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        &[0x08, 0x96, 0x01],
        &[0x00, 0x00, 0x00, 0x00],
        b"\x0a\x03abc\x10\x2a",
        b"{not json",
        b"name: \"x\" id: 1",
        b"syntax = \"proto3\";\nmessage M { int32 x = 1; }\n",
    ];
    for s in samples {
        assert!(
            run_all_catch(s).is_empty(),
            "crash on sample {s:?}: {:?}",
            run_all_catch(s)
        );
    }
}

#[test]
fn randomised_inputs_do_not_crash_within_bounds() {
    let mut seed: u64 = 0x1234_5678_9ABC_DEF0;
    let mut rng = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..500 {
        let len = (rng() % 128) as usize + 1;
        let mut buf = vec![0u8; len];
        for b in buf.iter_mut() {
            *b = rng() as u8;
        }
        assert!(run_all_catch(&buf).is_empty());
    }
}
