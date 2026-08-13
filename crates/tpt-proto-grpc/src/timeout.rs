//! Parsing and formatting of the gRPC `grpc-timeout` header.
//!
//! The timeout is an ASCII string `<non-negative integer><unit>` where the
//! unit is one of `H` (hours), `M` (minutes), `S` (seconds), `m`
//! (milliseconds), `u` (microseconds), or `n` (nanoseconds).

use anyhow::{bail, Result};
use std::time::Duration;

/// Parse a `grpc-timeout` header value into a [`Duration`].
pub fn parse_timeout(value: &str) -> Result<Duration> {
    if value.len() < 2 {
        bail!("invalid grpc-timeout `{value}`");
    }
    let (digits, unit) = value.split_at(value.len() - 1);
    let n: u64 = digits
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid grpc-timeout value `{value}`"))?;
    let d = match unit {
        "H" => n.checked_mul(3600).map(Duration::from_secs),
        "M" => n.checked_mul(60).map(Duration::from_secs),
        "S" => Some(Duration::from_secs(n)),
        "m" => Some(Duration::from_millis(n)),
        "u" => Some(Duration::from_micros(n)),
        "n" => Some(Duration::from_nanos(n)),
        other => bail!("unknown grpc-timeout unit `{other}`"),
    }
    .ok_or_else(|| anyhow::anyhow!("grpc-timeout `{value}` overflows"))?;
    Ok(d)
}

/// Format a [`Duration`] as the most compact `grpc-timeout` string.
///
/// Prefers seconds/minutes/hours when the value is a whole number of those
/// units, otherwise falls back to milliseconds, microseconds, then nanoseconds.
pub fn format_timeout(d: Duration) -> String {
    let total_nanos = d.as_nanos();
    if total_nanos % 1_000_000_000 == 0 {
        let secs = d.as_secs();
        if secs % 3600 == 0 {
            return format!("{}H", secs / 3600);
        }
        if secs % 60 == 0 {
            return format!("{}M", secs / 60);
        }
        return format!("{}S", secs);
    }
    if total_nanos % 1_000_000 == 0 {
        return format!("{}m", d.as_millis());
    }
    if total_nanos % 1_000 == 0 {
        return format!("{}u", d.as_micros());
    }
    format!("{}n", total_nanos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_units() {
        assert_eq!(parse_timeout("1H").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_timeout("2M").unwrap(), Duration::from_secs(120));
        assert_eq!(parse_timeout("30S").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_timeout("500m").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_timeout("250u").unwrap(), Duration::from_micros(250));
        assert_eq!(parse_timeout("999n").unwrap(), Duration::from_nanos(999));
    }

    #[test]
    fn parse_errors() {
        assert!(parse_timeout("S").is_err());
        assert!(parse_timeout("1X").is_err());
        assert!(parse_timeout("-1S").is_err());
        assert!(parse_timeout("99999999999999999999999999999H").is_err());
    }

    #[test]
    fn format_roundtrip() {
        for d in [
            Duration::from_secs(3600),
            Duration::from_secs(120),
            Duration::from_secs(30),
            Duration::from_millis(500),
            Duration::from_micros(250),
            Duration::from_nanos(999),
        ] {
            let s = format_timeout(d);
            assert_eq!(parse_timeout(&s).unwrap(), d, "format `{s}` did not roundtrip");
        }
    }

    #[test]
    fn format_prefers_largest_unit() {
        assert_eq!(format_timeout(Duration::from_secs(7200)), "2H");
        assert_eq!(format_timeout(Duration::from_secs(90)), "90S");
    }
}
