//! Interactive update-check policy.

// Exercised by its unit tests now; the side-effectful caller that consumes it
// lands next iteration (self-update I/O shell), which removes this allow.
#![allow(dead_code)]

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Check,
    Skip,
}

const DAY_SECS: u64 = 24 * 60 * 60;

/// `now`/`last_check` are Unix epoch seconds; `None` means never checked.
#[must_use]
pub fn update_decision(disabled: bool, is_tty: bool, last_check: Option<u64>, now: u64) -> Decision {
    if disabled || !is_tty {
        return Decision::Skip;
    }
    match last_check {
        Some(t) if now.saturating_sub(t) < DAY_SECS => Decision::Skip,
        _ => Decision::Check,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_000_000;

    #[test]
    fn disabled_skips() {
        assert_eq!(update_decision(true, true, None, NOW), Decision::Skip);
    }

    #[test]
    fn non_tty_skips() {
        assert_eq!(update_decision(false, false, None, NOW), Decision::Skip);
    }

    #[test]
    fn recent_check_skips() {
        assert_eq!(update_decision(false, true, Some(NOW - 3600), NOW), Decision::Skip);
    }

    #[test]
    fn stale_check_checks() {
        assert_eq!(update_decision(false, true, Some(NOW - 25 * 3600), NOW), Decision::Check);
    }

    #[test]
    fn never_checked_checks() {
        assert_eq!(update_decision(false, true, None, NOW), Decision::Check);
    }
}
