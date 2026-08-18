use anyhow::{bail, Result};
use humansize::{format_size, DECIMAL};

/// Format bytes into human-readable size string (e.g., "1.42 GB", "350 MB")
pub fn format_bytes(bytes: u64) -> String {
    format_size(bytes, DECIMAL)
}

/// Format duration in seconds into human-readable string (e.g. "1.2s", "450ms")
pub fn format_duration(duration: std::time::Duration) -> String {
    let millis = duration.as_millis();
    if millis < 1000 {
        format!("{}ms", millis)
    } else {
        format!("{:.2}s", duration.as_secs_f64())
    }
}

/// Parse human-readable string into bytes (e.g. "500MB", "1.5GB", "100K", "2048")
pub fn parse_size_to_bytes(size_str: &str) -> Result<u64> {
    let trimmed = size_str.trim();
    if trimmed.is_empty() {
        bail!("Empty size string");
    }

    let mut num_str = String::new();
    let mut unit_str = String::new();

    for c in trimmed.chars() {
        if c.is_ascii_digit() || c == '.' {
            if unit_str.is_empty() {
                num_str.push(c);
            } else {
                bail!("Invalid format: numbers after unit in {}", size_str);
            }
        } else if !c.is_whitespace() {
            unit_str.push(c);
        }
    }

    let value: f64 = num_str
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid number in {}", size_str))?;
    let unit = unit_str.to_uppercase();

    let multiplier: f64 = match unit.as_str() {
        "" | "B" | "BYTES" => 1.0,
        "K" | "KB" | "KIB" => 1_000.0,
        "M" | "MB" | "MIB" => 1_000_000.0,
        "G" | "GB" | "GIB" => 1_000_000_000.0,
        "T" | "TB" | "TIB" => 1_000_000_000_000.0,
        _ => bail!(
            "Unknown unit '{}' in {}. Supported: B, KB, MB, GB, TB",
            unit_str,
            size_str
        ),
    };

    Ok((value * multiplier) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert!(format_bytes(1000).contains("kB") || format_bytes(1000).contains("1 kB"));
        assert!(
            format_bytes(1_000_000_000).contains("GB")
                || format_bytes(1_000_000_000).contains("1 GB")
        );
    }

    #[test]
    fn test_parse_size_to_bytes() {
        assert_eq!(parse_size_to_bytes("500B").unwrap(), 500);
        assert_eq!(parse_size_to_bytes("100KB").unwrap(), 100_000);
        assert_eq!(parse_size_to_bytes("500MB").unwrap(), 500_000_000);
        assert_eq!(parse_size_to_bytes("1GB").unwrap(), 1_000_000_000);
        assert_eq!(parse_size_to_bytes("1.5GB").unwrap(), 1_500_000_000);
    }
}
