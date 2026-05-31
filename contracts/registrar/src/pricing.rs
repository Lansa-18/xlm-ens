/// Version of the registrar pricing policy. Bump this whenever the tier
/// boundaries or amounts in [`price_for_label_length`] change so off-chain
/// clients can detect quote-policy changes without diffing every quote.
pub const PRICING_POLICY_VERSION: u32 = 1;

pub fn price_for_label_length(length: usize) -> u64 {
    match length {
        0..=3 => 1_000_000_000,
        4..=6 => 250_000_000,
        _ => 100_000_000,
    }
}
