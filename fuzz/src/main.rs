//! Portable corpus + randomised smoke harness.
//!
//! Runs every fuzz target over the seeded `corpus/` directory and a bounded
//! amount of randomised input, reporting (rather than aborting on) any panic.
//! Run with `cargo run` from the `fuzz/` directory. For real continuous
//! fuzzing use `cargo fuzz run <target>` (libFuzzer) on a supported platform.

use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push("corpus");
    d
}

fn main() {
    let dir = corpus_dir();
    let mut failures: Vec<(String, &'static str)> = Vec::new();

    if dir.exists() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = match entry {
                Ok(e) => e.path(),
                Err(_) => continue,
            };
            if !path.is_file() {
                continue;
            }
            let data = match std::fs::read(&path) {
                Ok(d) => d,
                Err(_) => continue,
            };
            for crash in tpt_proto_fuzz::harness::run_all_catch(&data) {
                failures.push((path.display().to_string(), crash));
            }
        }
    }

    // Bounded randomised fuzzing with a tiny xorshift PRNG. Input is kept small
    // so the recursive language parser cannot blow the stack in this smoke
    // mode (real fuzzing via cargo-fuzz tolerates crashes as findings).
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut rng = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..4000 {
        let len = (rng() % 256) as usize + 1;
        let mut buf = vec![0u8; len];
        for b in buf.iter_mut() {
            *b = rng() as u8;
        }
        for crash in tpt_proto_fuzz::harness::run_all_catch(&buf) {
            failures.push(("random".to_string(), crash));
        }
    }

    if failures.is_empty() {
        println!("fuzz smoke harness: OK (no crashes across corpus + randomised inputs)");
    } else {
        println!(
            "fuzz smoke harness: {} crash(es) found",
            failures.len()
        );
        for (src, t) in &failures {
            println!("  - {t} in {src}");
        }
        std::process::exit(1);
    }
}
