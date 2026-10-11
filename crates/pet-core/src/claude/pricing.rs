//! Rough API-price estimates for Claude models.
//!
//! IMPORTANT: these figures are indicative only. Subscription users do not pay
//! per token; the number is just a *relative* measure of how heavy a session or
//! an agent is. Prices change and model families are matched by substring.

use crate::TokenTotals;

/// Prices in USD per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Price {
    input: f64,
    output: f64,
}

/// Cache writes cost 1.25x the input price, cache reads 0.1x.
const CACHE_WRITE_FACTOR: f64 = 1.25;
const CACHE_READ_FACTOR: f64 = 0.1;

const OPUS: Price = Price {
    input: 15.0,
    output: 75.0,
};
const SONNET: Price = Price {
    input: 3.0,
    output: 15.0,
};
const HAIKU: Price = Price {
    input: 1.0,
    output: 5.0,
};

fn price_for(model: &str) -> Option<Price> {
    let m = model.to_ascii_lowercase();
    if m.contains("opus") || m.contains("fable") {
        // "fable" is priced like opus (indicative).
        Some(OPUS)
    } else if m.contains("sonnet") {
        Some(SONNET)
    } else if m.contains("haiku") {
        Some(HAIKU)
    } else {
        None
    }
}

/// Estimated API-equivalent cost in USD, or `None` for an unknown model family.
pub fn estimate_cost_usd(model: &str, t: &TokenTotals) -> Option<f64> {
    let p = price_for(model)?;
    let per_million = t.input as f64 * p.input
        + t.output as f64 * p.output
        + t.cache_creation as f64 * p.input * CACHE_WRITE_FACTOR
        + t.cache_read as f64 * p.input * CACHE_READ_FACTOR;
    Some(per_million / 1_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn totals(i: u64, o: u64, cc: u64, cr: u64) -> TokenTotals {
        TokenTotals {
            input: i,
            output: o,
            cache_creation: cc,
            cache_read: cr,
        }
    }

    #[test]
    fn opus_and_sonnet() {
        let t = totals(1_000_000, 1_000_000, 0, 0);
        assert_eq!(estimate_cost_usd("claude-opus-4-1", &t), Some(90.0));
        assert_eq!(estimate_cost_usd("claude-sonnet-4-5", &t), Some(18.0));
        assert_eq!(estimate_cost_usd("claude-haiku-4-5", &t), Some(6.0));
    }

    #[test]
    fn cache_factors() {
        let t = totals(0, 0, 1_000_000, 1_000_000);
        let c = estimate_cost_usd("Sonnet", &t).unwrap();
        assert!((c - (3.75 + 0.3)).abs() < 1e-9);
    }

    #[test]
    fn fable_like_opus_and_unknown_none() {
        let t = totals(1_000_000, 0, 0, 0);
        assert_eq!(estimate_cost_usd("fable-x", &t), Some(15.0));
        assert_eq!(estimate_cost_usd("<synthetic>", &t), None);
    }
}
