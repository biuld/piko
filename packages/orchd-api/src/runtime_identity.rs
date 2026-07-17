//! Stable identities used only by runtime, recovery, and diagnostic records.

/// Derives a deterministic opaque identifier with a deliberately fixed hash
/// algorithm. Do not use `DefaultHasher` for persisted identities: its output
/// is not a stable Rust contract.
pub fn stable_internal_id(prefix: &str, parts: &[&str]) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = FNV_OFFSET;
    for part in parts {
        for byte in part.len().to_le_bytes().iter().chain(part.as_bytes()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    format!("{prefix}_{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_stable_and_part_boundaries_are_unambiguous() {
        assert_eq!(
            stable_internal_id("exec", &["session", "agent", "request"]),
            stable_internal_id("exec", &["session", "agent", "request"])
        );
        assert_ne!(
            stable_internal_id("exec", &["ab", "c"]),
            stable_internal_id("exec", &["a", "bc"])
        );
    }
}
