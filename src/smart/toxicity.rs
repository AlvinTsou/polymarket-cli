#![allow(dead_code)]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOutcome {
    Yes,
    No,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggressorSide {
    Buy,
    Sell,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TradePrint {
    pub outcome: BinaryOutcome,
    pub side: AggressorSide,
    pub size: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VolumeBucket {
    pub yes_buy_volume: f64,
    pub yes_sell_volume: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ToxicityVerdict {
    Calm,
    Normal,
    Toxic,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VpinSnapshot {
    pub vpin: f64,
    pub buckets: usize,
    pub verdict: ToxicityVerdict,
}

impl VolumeBucket {
    pub fn total_volume(self) -> f64 {
        self.yes_buy_volume + self.yes_sell_volume
    }

    pub fn imbalance(self) -> f64 {
        let total = self.total_volume();
        if total <= 0.0 {
            return 0.0;
        }
        (self.yes_buy_volume - self.yes_sell_volume).abs() / total
    }
}

pub fn normalized_yes_flow(trade: TradePrint) -> Option<f64> {
    if !trade.size.is_finite() || trade.size <= 0.0 {
        return None;
    }

    let signed = match (trade.outcome, trade.side) {
        (BinaryOutcome::Yes, AggressorSide::Buy) | (BinaryOutcome::No, AggressorSide::Sell) => {
            trade.size
        }
        (BinaryOutcome::Yes, AggressorSide::Sell) | (BinaryOutcome::No, AggressorSide::Buy) => {
            -trade.size
        }
    };
    Some(signed)
}

pub fn volume_buckets(trades: &[TradePrint], bucket_volume: f64) -> Vec<VolumeBucket> {
    if !bucket_volume.is_finite() || bucket_volume <= 0.0 {
        return Vec::new();
    }

    let mut buckets = Vec::new();
    let mut current = VolumeBucket::default();
    let mut current_volume = 0.0;

    for trade in trades {
        let Some(flow) = normalized_yes_flow(*trade) else {
            continue;
        };

        let mut remaining = flow.abs();
        let buy_side = flow > 0.0;

        while remaining > 0.0 {
            let capacity = bucket_volume - current_volume;
            let fill = remaining.min(capacity);

            if buy_side {
                current.yes_buy_volume += fill;
            } else {
                current.yes_sell_volume += fill;
            }

            current_volume += fill;
            remaining -= fill;

            if current_volume >= bucket_volume - f64::EPSILON {
                buckets.push(current);
                current = VolumeBucket::default();
                current_volume = 0.0;
            }
        }
    }

    if current_volume > 0.0 {
        buckets.push(current);
    }

    buckets
}

pub fn compute_vpin(buckets: &[VolumeBucket], rolling_window: usize) -> Option<f64> {
    if buckets.is_empty() || rolling_window == 0 {
        return None;
    }

    let start = buckets.len().saturating_sub(rolling_window);
    let window = &buckets[start..];
    let total_volume: f64 = window.iter().map(|bucket| bucket.total_volume()).sum();
    if total_volume <= 0.0 {
        return None;
    }

    let total_imbalance: f64 = window
        .iter()
        .map(|bucket| (bucket.yes_buy_volume - bucket.yes_sell_volume).abs())
        .sum();
    Some((total_imbalance / total_volume).clamp(0.0, 1.0))
}

pub fn classify_vpin(vpin: f64, calm_threshold: f64, toxic_threshold: f64) -> ToxicityVerdict {
    if !vpin.is_finite() {
        return ToxicityVerdict::Normal;
    }
    if vpin >= toxic_threshold {
        ToxicityVerdict::Toxic
    } else if vpin <= calm_threshold {
        ToxicityVerdict::Calm
    } else {
        ToxicityVerdict::Normal
    }
}

pub fn vpin_snapshot(
    trades: &[TradePrint],
    bucket_volume: f64,
    rolling_window: usize,
    calm_threshold: f64,
    toxic_threshold: f64,
) -> Option<VpinSnapshot> {
    let buckets = volume_buckets(trades, bucket_volume);
    let vpin = compute_vpin(&buckets, rolling_window)?;
    Some(VpinSnapshot {
        vpin,
        buckets: buckets.len(),
        verdict: classify_vpin(vpin, calm_threshold, toxic_threshold),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trade(outcome: BinaryOutcome, side: AggressorSide, size: f64) -> TradePrint {
        TradePrint {
            outcome,
            side,
            size,
        }
    }

    #[test]
    fn normalizes_no_side_into_yes_probability_flow() {
        assert_eq!(
            normalized_yes_flow(trade(BinaryOutcome::Yes, AggressorSide::Buy, 10.0)),
            Some(10.0)
        );
        assert_eq!(
            normalized_yes_flow(trade(BinaryOutcome::No, AggressorSide::Sell, 10.0)),
            Some(10.0)
        );
        assert_eq!(
            normalized_yes_flow(trade(BinaryOutcome::No, AggressorSide::Buy, 10.0)),
            Some(-10.0)
        );
        assert_eq!(
            normalized_yes_flow(trade(BinaryOutcome::Yes, AggressorSide::Sell, 10.0)),
            Some(-10.0)
        );
    }

    #[test]
    fn rejects_invalid_trade_sizes() {
        assert_eq!(
            normalized_yes_flow(trade(BinaryOutcome::Yes, AggressorSide::Buy, 0.0)),
            None
        );
        assert_eq!(
            normalized_yes_flow(trade(BinaryOutcome::Yes, AggressorSide::Buy, f64::NAN)),
            None
        );
    }

    #[test]
    fn splits_large_trade_across_volume_buckets() {
        let buckets = volume_buckets(&[trade(BinaryOutcome::Yes, AggressorSide::Buy, 25.0)], 10.0);

        assert_eq!(buckets.len(), 3);
        assert_eq!(buckets[0].total_volume(), 10.0);
        assert_eq!(buckets[1].total_volume(), 10.0);
        assert_eq!(buckets[2].total_volume(), 5.0);
    }

    #[test]
    fn computes_vpin_from_recent_rolling_window() {
        let buckets = vec![
            VolumeBucket {
                yes_buy_volume: 5.0,
                yes_sell_volume: 5.0,
            },
            VolumeBucket {
                yes_buy_volume: 9.0,
                yes_sell_volume: 1.0,
            },
        ];

        let vpin = compute_vpin(&buckets, 1).unwrap();
        assert!((vpin - 0.8).abs() < 1e-9);
    }

    #[test]
    fn classifies_calm_normal_and_toxic_vpin() {
        assert_eq!(classify_vpin(0.05, 0.10, 0.30), ToxicityVerdict::Calm);
        assert_eq!(classify_vpin(0.20, 0.10, 0.30), ToxicityVerdict::Normal);
        assert_eq!(classify_vpin(0.35, 0.10, 0.30), ToxicityVerdict::Toxic);
    }

    #[test]
    fn builds_snapshot_from_trade_prints() {
        let trades = [
            trade(BinaryOutcome::Yes, AggressorSide::Buy, 10.0),
            trade(BinaryOutcome::Yes, AggressorSide::Sell, 5.0),
            trade(BinaryOutcome::No, AggressorSide::Sell, 5.0),
        ];

        let snapshot = vpin_snapshot(&trades, 10.0, 2, 0.10, 0.30).unwrap();
        assert_eq!(snapshot.buckets, 2);
        assert_eq!(snapshot.verdict, ToxicityVerdict::Toxic);
    }
}
