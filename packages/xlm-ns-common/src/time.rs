use crate::{GRACE_PERIOD_SECONDS, YEAR_SECONDS};

pub fn expiry_from_now(now_unix: u64, years: u64) -> u64 {
    now_unix.saturating_add(YEAR_SECONDS.saturating_mul(years))
}

pub fn grace_period_ends_at(expiry_unix: u64) -> u64 {
    expiry_unix.saturating_add(GRACE_PERIOD_SECONDS)
}

pub fn is_active_at(expires_at: u64, now_unix: u64) -> bool {
    now_unix <= expires_at
}

pub fn within_grace_period(expiry_unix: u64, now_unix: u64) -> bool {
    now_unix > expiry_unix && now_unix <= grace_period_ends_at(expiry_unix)
}

pub fn is_claimable_at(grace_period_ends_at: u64, now_unix: u64) -> bool {
    now_unix > grace_period_ends_at
}

pub fn is_time_window_open(now_unix: u64, starts_at: u64, ends_at: u64) -> bool {
    now_unix >= starts_at && now_unix <= ends_at
}
