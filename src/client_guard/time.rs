// Monotonic-ish wall clock reference for idle-eviction comparisons; wrap-around and
// clock skew are irrelevant here since we only compare recent deltas.
pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
