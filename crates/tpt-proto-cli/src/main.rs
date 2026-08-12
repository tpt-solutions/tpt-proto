//! `tpt-proto` command-line entry point.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tpt_proto_cli::{compile_path, decode_hex, describe, emit_descriptor_bin, print_message};

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
        Command::Describe { proto } => {
            let (fd, _diags) = compile_path(&proto)?;
            describe(&fd);
        }
        Command::Decode { proto, message, hex } => {
            let dm = decode_hex(&proto, &message, &hex)?;
            print_message(&dm, 0);
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
