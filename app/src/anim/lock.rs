//! Locked rings: per limit, the generation (resetSeq of the batch) that locked it.

use limits_core::LimitKey;

/// Indexed by `LimitKey` (session 0, weekly 1).
pub type Locks = [Option<u64>; 2];

fn idx(key: LimitKey) -> usize {
    match key {
        LimitKey::Session => 0,
        LimitKey::Weekly => 1,
    }
}

/// Locks `keys` for batch `gen`; a newer batch takes over a ring that is still locked.
pub fn lock_keys(mut locks: Locks, keys: &[LimitKey], gen: u64) -> Locks {
    for &k in keys {
        locks[idx(k)] = Some(gen);
    }
    locks
}

/// Unlocks `key` only if batch `gen` still owns it (an older batch finishing late changes nothing).
pub fn release(mut locks: Locks, key: LimitKey, gen: u64) -> Locks {
    if locks[idx(key)] == Some(gen) {
        locks[idx(key)] = None;
    }
    locks
}

#[cfg(test)]
mod tests {
    use super::*;
    use LimitKey::{Session, Weekly};

    #[test]
    fn locks_the_affected_keys_with_the_batch_generation() {
        assert_eq!(lock_keys([None; 2], &[Session], 3), [Some(3), None]);
        assert_eq!(
            lock_keys([None, Some(2)], &[Session], 3),
            [Some(3), Some(2)]
        );
    }

    #[test]
    fn newer_batch_takes_over_an_already_locked_ring() {
        assert_eq!(
            lock_keys([Some(2), None], &[Session, Weekly], 5),
            [Some(5), Some(5)]
        );
    }

    #[test]
    fn releases_only_when_the_generation_matches() {
        assert_eq!(release([Some(3), Some(3)], Session, 3), [None, Some(3)]);
    }

    #[test]
    fn stale_release_keeps_the_newer_lock() {
        assert_eq!(release([Some(5), None], Session, 3), [Some(5), None]);
    }

    #[test]
    fn releasing_an_unlocked_key_is_a_noop() {
        assert_eq!(release([None; 2], Weekly, 1), [None; 2]);
    }
}
