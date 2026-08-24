//! `tpt-proto` command-line entry point.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tpt_proto_cli::{
    binary_to_json, binary_to_text, compile_set_with_includes, decode_hex, describe,
    descriptor_set_bytes, diff_descriptors, emit_descriptor_bin, generate_code, json_to_binary,
    lint_files, load_pool, lookup_message, print_message, read_bytes, text_to_binary,
};

#[derive(Parser)]
#[command(name = "tpt-proto", about = "tpt-proto Protocol Buffers toolchain")]
struct Cli {
    /// Import search directory (repeatable). `import` statements resolve
    /// relative to these paths, in order.
    #[arg(
        short = 'I',
        long = "include",
        value_name = "DIR",
        global = true
    )]
    include: Vec<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse and compile a `.proto` file, printing its structure.
    Compile {
        /// Path to the `.proto` file.
        proto: PathBuf,
        /// Also write the serialized descriptor to this path.
        #[arg(long)]
        descriptor_out: Option<PathBuf>,
    },
    /// Generate Rust source code for a `.proto` file.
    Generate {
        /// Path to the `.proto` file.
        proto: PathBuf,
        /// Optional output file (defaults to stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Emit async gRPC server traits and client stubs.
        #[arg(long)]
        grpc: bool,
    },
    /// Emit the serialized `FileDescriptorSet` for a `.proto` file.
    Descriptors {
        /// Path to the `.proto` file.
        proto: PathBuf,
        /// Output file.
        out: PathBuf,
    },
    /// Describe the messages/enums/services in a `.proto` file.
    Describe {
        /// Path to the `.proto` file.
        proto: PathBuf,
    },
    /// Decode a hex-encoded message using a `.proto` schema.
    Decode {
        /// Path to the `.proto` file.
        proto: PathBuf,
        /// Fully-qualified (or simple) message name.
        message: String,
        /// Hex-encoded wire bytes.
        hex: String,
    },
    /// Decode a binary message file using a `.proto` schema.
    Encode {
        /// Path to the `.proto` file.
        proto: PathBuf,
        /// Message name.
        message: String,
        /// Input file (binary, JSON, or text depending on `--from`).
        input: PathBuf,
        /// Input format: `binary`, `json`, or `text`.
        #[arg(long, default_value = "json")]
        from: String,
        /// Output binary file.
        #[arg(long)]
        out: PathBuf,
    },
    /// Convert a binary message file to JSON.
    Json {
        /// Path to the `.proto` file.
        proto: PathBuf,
        /// Message name.
        message: String,
        /// Input binary file.
        input: PathBuf,
        /// Output JSON file (defaults to stdout).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Convert a binary message file to text format.
    Text {
        /// Path to the `.proto` file.
        proto: PathBuf,
        /// Message name.
        message: String,
        /// Input binary file.
        input: PathBuf,
        /// Output text file (defaults to stdout).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Convert a JSON or text file into binary.
    ToBinary {
        /// Input format: `json` or `text`.
        #[arg(long, default_value = "json")]
        from: String,
        /// Path to the `.proto` file.
        proto: PathBuf,
        /// Message name.
        message: String,
        /// Input file.
        input: PathBuf,
        /// Output binary file.
        out: PathBuf,
    },
    /// Diff two descriptor-set files.
    Diff {
        /// First descriptor-set file.
        a: PathBuf,
        /// Second descriptor-set file.
        b: PathBuf,
    },
    /// Compare two `.proto` files for breaking changes.
    Lint {
        /// Baseline (old) `.proto` file.
        old: PathBuf,
        /// Candidate (new) `.proto` file.
        new: PathBuf,
        /// Emit JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// Format `.proto` files in place (or verify with `--check`).
    Fmt {
        /// Files to format.
        files: Vec<PathBuf>,
        /// Do not write; exit non-zero if any file would change (CI mode).
        #[arg(long)]
        check: bool,
    },
    /// Create a new tpt-proto starter project (`cargo new`-style scaffold).
    Init {
        /// Name of the new crate and directory.
        name: String,
        /// Overwrite files if the directory already exists.
        #[arg(long)]
        force: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Project config supplies defaults; explicit flags win.
    let project = tpt_proto_cli::config::load_discovered()
        .map_err(|e| anyhow::anyhow!("invalid project config: {e}"))?;
    let mut include_dirs: Vec<PathBuf> = Vec::new();
    if let Some(p) = &project {
        include_dirs.extend(p.include_paths());
    }
    include_dirs.extend(cli.include.iter().cloned());
    let includes: Vec<PathBuf> = include_dirs;
    let empty_overrides = std::collections::HashMap::new();
    let lint_overrides = project
        .as_ref()
        .map(|p| &p.lint.severity)
        .unwrap_or(&empty_overrides);

    match cli.command {
        Command::Compile { proto, descriptor_out } => {
            let set = compile_set_with_includes(&proto, &includes)?;
            let fd = set.file.last().expect("non-empty resolved set");
            if let Some(out) = descriptor_out {
                emit_descriptor_bin(fd, &out)?;
                println!("wrote descriptor to {}", out.display());
            } else {
                describe(fd);
            }
        }
        Command::Generate { proto, out, grpc } => {
            let out =
                out.or_else(|| project.as_ref().and_then(|p| p.codegen_out()));
            let code = generate_code(&proto, &includes, grpc || project.as_ref().is_some_and(|p| p.codegen.grpc))?;
            match out {
                Some(out) => {
                    std::fs::write(&out, &code)?;
                    println!("wrote {}", out.display());
                }
                None => print!("{code}"),
            }
        }
        Command::Descriptors { proto, out } => {
            let bytes = descriptor_set_bytes(&proto, &includes)?;
            std::fs::write(&out, bytes)?;
            println!("wrote {}", out.display());
        }
        Command::Describe { proto } => {
            let (_pool, fd) = load_pool(&proto, &includes)?;
            describe(&fd);
        }
        Command::Decode { proto, message, hex } => {
            let dm = decode_hex(&proto, includes, &message, &hex)?;
            print_message(&dm, 0);
        }
        Command::Encode { proto, message, input, from, out } => {
            let (pool, fd) = load_pool(&proto, &includes)?;
            let desc = lookup_message(&pool, &fd, &message)?;
            let raw = read_bytes(&input)?;
            let bytes = match from.as_str() {
                "binary" => raw,
                "json" => json_to_binary(&pool, &desc, &String::from_utf8_lossy(&raw))?,
                "text" => text_to_binary(&pool, &desc, &String::from_utf8_lossy(&raw))?,
                other => anyhow::bail!("unknown --from format `{other}`"),
            };
            std::fs::write(&out, bytes)?;
            println!("wrote {}", out.display());
        }
        Command::Json { proto, message, input, out } => {
            let (pool, fd) = load_pool(&proto, &includes)?;
            let desc = lookup_message(&pool, &fd, &message)?;
            let bytes = read_bytes(&input)?;
            let json = binary_to_json(&pool, &desc, &bytes)?;
            if let Some(out) = out {
                std::fs::write(&out, json)?;
            } else {
                println!("{json}");
            }
        }
        Command::Text { proto, message, input, out } => {
            let (pool, fd) = load_pool(&proto, &includes)?;
            let desc = lookup_message(&pool, &fd, &message)?;
            let bytes = read_bytes(&input)?;
            let text = binary_to_text(&pool, &desc, &bytes)?;
            if let Some(out) = out {
                std::fs::write(&out, text)?;
            } else {
                print!("{text}");
            }
        }
        Command::ToBinary { from, proto, message, input, out } => {
            let (pool, fd) = load_pool(&proto, &includes)?;
            let desc = lookup_message(&pool, &fd, &message)?;
            let raw = read_bytes(&input)?;
            let text = String::from_utf8_lossy(&raw);
            let bytes = match from.as_str() {
                "json" => json_to_binary(&pool, &desc, &text)?,
                "text" => text_to_binary(&pool, &desc, &text)?,
                other => anyhow::bail!("unknown --from format `{other}`"),
            };
            std::fs::write(&out, bytes)?;
            println!("wrote {}", out.display());
        }
        Command::Diff { a, b } => {
            let ba = read_bytes(&a)?;
            let bb = read_bytes(&b)?;
            let diff = diff_descriptors(&ba, &bb)?;
            print!("{diff}");
        }
        Command::Lint { old, new, json } => {
            let report = lint_files(&old, &new, json, &includes, lint_overrides)?;
            print!("{report}");
        }
        Command::Fmt { files, check } => {
            if files.is_empty() {
                anyhow::bail!("fmt requires at least one `.proto` file");
            }
            let mut changed = 0usize;
            for f in &files {
                let src = std::fs::read_to_string(f)
                    .map_err(|e| anyhow::anyhow!("reading {}: {e}", f.display()))?;
                let formatted = tpt_proto_fmt::format_source(&src)
                    .map_err(|e| anyhow::anyhow!("{}: {e}", f.display()))?;
                if formatted != src {
                    changed += 1;
                    if check {
                        println!("would reformat {}", f.display());
                    } else {
                        std::fs::write(f, formatted)?;
                        println!("formatted {}", f.display());
                    }
                }
            }
            if check && changed > 0 {
                anyhow::bail!("{changed} file(s) need formatting");
            }
        }
        Command::Init { name, force } => {
            let path = tpt_proto_cli::init_project(&name, force)?;
            println!("created new tpt-proto project at {}", path.display());
            println!("next steps:");
            println!("  cd {name}");
            println!("  cargo run");
        }
    }
    Ok(())
}
