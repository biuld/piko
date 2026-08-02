//! Host environment capture for prompt fragments.
//!
//! Capture is deliberately whitelisted and fail-closed: only well-known,
//! non-secret facts are read, and unavailable facts are omitted rather than
//! fabricated. Full process environment is never exposed to the prompt.

use chrono::Offset;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvironmentSnapshot {
    pub os: Option<String>,
    pub arch: Option<String>,
    pub shell: Option<String>,
    pub hostname: Option<String>,
    pub timezone: Option<String>,
    pub locale: Option<String>,
}

impl EnvironmentSnapshot {
    /// Capture host facts from whitelisted sources only.
    pub fn capture() -> Self {
        Self {
            os: Some(std::env::consts::OS.to_string()),
            arch: Some(std::env::consts::ARCH.to_string()),
            shell: env_var_trimmed("SHELL"),
            hostname: env_var_trimmed("HOSTNAME").or_else(|| env_var_trimmed("COMPUTERNAME")),
            timezone: local_utc_offset(),
            locale: env_var_trimmed("LANG").or_else(|| env_var_trimmed("LC_ALL")),
        }
    }
}

fn env_var_trimmed(name: &str) -> Option<String> {
    let value = std::env::var(name).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn local_utc_offset() -> Option<String> {
    let offset = chrono::Local::now().offset().fix();
    Some(format!("{offset}"))
}

#[cfg(test)]
mod tests {
    use super::EnvironmentSnapshot;

    #[test]
    fn capture_reads_only_expected_fields() {
        let snapshot = EnvironmentSnapshot::capture();
        assert_eq!(snapshot.os.as_deref(), Some(std::env::consts::OS));
        assert_eq!(snapshot.arch.as_deref(), Some(std::env::consts::ARCH));
        assert!(snapshot.timezone.is_some());
    }
}
