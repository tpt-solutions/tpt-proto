//! `tpt-proto` command-line entry point.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tpt_proto_cli::{
    binary_to_json, binary_to_text, compile_path, decode_hex, describe, descriptor_set_bytes,
    diff_descriptors, emit_descriptor_bin, generate_code, json_to_binary, lint_files,
    lookup_message, print_message, read_bytes, text_to_binary,
};

#[derive(Parser)]
#[command(name = "tpt-proto", about = "tpt-proto Protocol Buffers toolchain")]
struct Cli {
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Compile { proto, descriptor_out } => {
            let (fd, diags) = compile_path(&proto)?;
            for d in &diags {
                eprintln!("{}: {}", severity(&d.severity), d.message);
            }
            if let Some(out) = descriptor_out {
                emit_descriptor_bin(&fd, &out)?;
                println!("wrote descriptor to {}", out.display());
            } else {
                describe(&fd);
            }
        }
        Command::Generate { proto, out, grpc } => {
            let code = generate_code(&proto, grpc)?;
            if let Some(out) = out {
                std::fs::write(&out, &code)?;
                println!("wrote {}", out.display());
            } else {
                print!("{code}");
            }
        }
        Command::Descriptors { proto, out } => {
            let bytes = descriptor_set_bytes(&proto)?;
            std::fs::write(&out, bytes)?;
            println!("wrote {}", out.display());
        }
        Command::Describe { proto } => {
            let (fd, _diags) = compile_path(&proto)?;
            describe(&fd);
        }
        Command::Decode { proto, message, hex } => {
            let dm = decode_hex(&proto, &message, &hex)?;
            print_message(&dm, 0);
        }
        Command::Encode { proto, message, input, from, out } => {
            let (fd, _diags) = compile_path(&proto)?;
            let pool = tpt_proto_reflect::DescriptorPool::from_file(&fd);
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
            let (fd, _diags) = compile_path(&proto)?;
            let pool = tpt_proto_reflect::DescriptorPool::from_file(&fd);
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
            let (fd, _diags) = compile_path(&proto)?;
            let pool = tpt_proto_reflect::DescriptorPool::from_file(&fd);
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
            let (fd, _diags) = compile_path(&proto)?;
            let pool = tpt_proto_reflect::DescriptorPool::from_file(&fd);
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
            let report = lint_files(&old, &new, json)?;
            print!("{report}");
        }
    }
    Ok(())
}

fn severity(s: &tpt_proto_language::Severity) -> &'static str {
    match s {
        tpt_proto_language::Severity::Error => "error",
        tpt_proto_language::Severity::Warning => "warning",
    }
}
