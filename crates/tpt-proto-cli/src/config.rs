//! `tpt-proto.toml` project configuration with Cargo-style discovery.
//!
//! A `tpt-proto.toml` file may live at a project root; it is discovered by
//! walking upward from the current directory until a file named
//! [`CONFIG_FILE_NAME`] is found (or the filesystem root is reached). All
//! relative paths inside the file resolve against **the directory containing
//! the config file** — exactly like `Cargo.toml`.
//!
//! The config supplies *defaults*; explicit CLI flags are merged on top and
//! never overridden by it:
//!
//! * include paths from `[includes]` are prepended to `-I` flags;
//! * `[codegen]` provides defaults for `generate`'s `--out` / `--grpc` when
//!   those flags are absent;
//! * `[lint.severity]` remaps or drops lint findings by rule code
//!   (`"error" | "warning" | "ignore"`).
//!
//! Example:
//!
//! ```toml
//! [includes]
//! paths = ["proto", "vendor/protos"]
//!
//! [lint.severity]
//! DEFAULT_VALUE_CHANGED = "warning"
//! JSON_NAME_REMOVED = "ignore"
//!
//! [codegen]
//! out = "src/generated"
//! grpc = false
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The file name searched for during upward discovery.
pub const CONFIG_FILE_NAME: &str = "tpt-proto.toml";

/// Top-level project configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectConfig {
    /// Directory the config file was loaded from; all relative paths inside
    /// the config resolve against this.
    pub base_dir: PathBuf,
    /// `[includes]`
    pub includes: IncludesSection,
    /// `[lint]`
    pub lint: LintSection,
    /// `[codegen]`
    pub codegen: CodegenSection,
}

/// Include search paths shared by every schema-compiling command.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct IncludesSection {
    /// Paths added before any explicit `-I` flags.
    #[serde(default)]
    pub paths: Vec<String>,
}

/// Per-rule-code severity overrides for `tpt-proto lint`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct LintSection {
    /// Rule code → `"error" | "warning" | "ignore"` (case-insensitive keys).
    #[serde(default)]
    pub severity: HashMap<String, String>,
}

/// Defaults for `tpt-proto generate`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct CodegenSection {
    /// Default output path for generated Rust source.
    #[serde(default)]
    pub out: Option<String>,
    /// Emit gRPC stubs by default (`--grpc` can only add it explicitly).
    #[serde(default)]
    pub grpc: bool,
}

impl ProjectConfig {
    /// `[includes].paths`, resolved against the config directory.
    pub fn include_paths(&self) -> Vec<PathBuf> {
        self.includes
            .paths
            .iter()
            .map(|p| self.base_dir.join(p))
            .collect()
    }

    /// `[codegen].out`, resolved against the config directory.
    pub fn codegen_out(&self) -> Option<PathBuf> {
        self.codegen.out.as_ref().map(|p| self.base_dir.join(p))
    }
}

/// Find the nearest `tpt-proto.toml` at or above `start` (defaults to the
/// current directory). Returns the config file's path.
pub fn discover(start: Option<&Path>) -> Option<PathBuf> {
    let start = match start {
        Some(p) => p.to_path_buf(),
        None => std::env::current_dir().ok()?,
    };
    let mut dir = Some(start.as_path());
    while let Some(d) = dir {
        let candidate = d.join(CONFIG_FILE_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

/// Parse a config file at `path`. Unknown keys are rejected so typos in rule
/// codes / sections fail loudly instead of being silently ignored.
pub fn load(path: &Path) -> Result<ProjectConfig, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("reading {}: {e}", path.display()))?;
    #[derive(Deserialize, Default)]
    #[serde(deny_unknown_fields)]
    struct Raw {
        #[serde(default)]
        includes: IncludesSectionRaw,
        #[serde(default)]
        lint: LintSectionRaw,
        #[serde(default)]
        codegen: CodegenSectionRaw,
    }
    #[derive(Deserialize, Default)]
    #[serde(deny_unknown_fields)]
    struct IncludesSectionRaw {
        #[serde(default)]
        paths: Vec<String>,
    }
    #[derive(Deserialize, Default)]
    #[serde(deny_unknown_fields)]
    struct LintSectionRaw {
        #[serde(default)]
        severity: HashMap<String, String>,
    }
    #[derive(Deserialize, Default)]
    #[serde(deny_unknown_fields)]
    struct CodegenSectionRaw {
        #[serde(default)]
        out: Option<String>,
        #[serde(default)]
        grpc: bool,
    }
    let raw: Raw = toml::from_str(&raw).map_err(|e| format!("parsing {}: {e}", path.display()))?;
    let base_dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(ProjectConfig {
        base_dir,
        includes: IncludesSection { paths: raw.includes.paths },
        lint: LintSection { severity: raw.lint.severity },
        codegen: CodegenSection { out: raw.codegen.out, grpc: raw.codegen.grpc },
    })
}

/// Discover and load the nearest config, returning `None` when absent.
/// Malformed configs surface as errors rather than being silently ignored.
pub fn load_discovered() -> Result<Option<ProjectConfig>, String> {
    match discover(None) {
        Some(path) => load(&path).map(Some),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_example_and_discovers_upward() {
        let tmp = std::env::temp_dir().join(format!("tpt_cfg_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("proto/deep")).unwrap();
        std::fs::write(
            tmp.join(CONFIG_FILE_NAME),
            "[includes]\npaths = [\"proto\", \"vendor\"]\n\n[lint.severity]\nDEFAULT_VALUE_CHANGED = \"ignore\"\nJSON_NAME_REMOVED = \"error\"\n\n[codegen]\nout = \"src/gen\"\ngrpc = true\n",
        )
        .unwrap();

        let cfg = load(&tmp.join(CONFIG_FILE_NAME)).unwrap();
        assert_eq!(
            cfg.include_paths(),
            vec![tmp.join("proto"), tmp.join("vendor")]
        );
        assert_eq!(cfg.lint.severity.get("DEFAULT_VALUE_CHANGED").unwrap(), "ignore");
        assert_eq!(cfg.codegen_out(), Some(tmp.join("src/gen")));
        assert!(cfg.codegen.grpc);

        // Discovery walks upward from a nested directory.
        assert_eq!(
            discover(Some(&tmp.join("proto/deep"))),
            Some(tmp.join(CONFIG_FILE_NAME))
        );

        // No config above an isolated dir → None.
        let empty =
            std::env::temp_dir().join(format!("tpt_cfg_none_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(discover(Some(&empty)), None);

        let _ = std::fs::remove_dir_all(tmp);
    }

    #[test]
    fn empty_file_yields_defaults() {
        let tmp = std::env::temp_dir().join(format!("tpt_cfg_e_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join(CONFIG_FILE_NAME), "").unwrap();
        let cfg = load(&tmp.join(CONFIG_FILE_NAME)).unwrap();
        assert_eq!(
            cfg,
            ProjectConfig {
                base_dir: tmp.clone(),
                ..Default::default()
            }
        );
        let _ = std::fs::remove_dir_all(tmp);
    }

    #[test]
    fn unknown_keys_rejected() {
        let tmp = std::env::temp_dir().join(format!("tpt_cfg_u_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join(CONFIG_FILE_NAME), "bogus = true\n").unwrap();
        assert!(load(&tmp.join(CONFIG_FILE_NAME)).is_err());
        // Unknown rule codes inside lint.severity are allowed (they may
        // target codes from other versions); unknown *sections* are not.
        std::fs::write(
            tmp.join(CONFIG_FILE_NAME),
            "[no_such_section]\nx = 1\n",
        )
        .unwrap();
        assert!(load(&tmp.join(CONFIG_FILE_NAME)).is_err());
        let _ = std::fs::remove_dir_all(tmp);
    }
}
