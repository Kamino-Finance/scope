use std::cmp::Ordering;

use anchor_lang::prelude::*;
use intbits::Bits;

use self::utils::update_ema_twaps;
use crate::{
    debug,
    states::{EmaType, OracleMappings, OracleTwaps, TwapEnabledBitmask},
    DatedPrice, ScopeError, ScopeResult,
};

const EMA_1H_DURATION_SECONDS: u64 = 60 * 60;
const EMA_8H_DURATION_SECONDS: u64 = 8 * 60 * 60;
const EMA_24H_DURATION_SECONDS: u64 = 24 * 60 * 60;
const MIN_SAMPLES_IN_PERIOD_1H: u32 = 10;
const MIN_SAMPLES_IN_PERIOD_8H: u32 = 24;
const MIN_SAMPLES_IN_PERIOD_24H: u32 = 48;
const NUM_SUB_PERIODS_1H: usize = 3;
const NUM_SUB_PERIODS_8H: usize = 8;
const NUM_SUB_PERIODS_24H: usize = 24;
const MIN_SAMPLES_IN_FIRST_AND_LAST_PERIOD: u32 = 1;

pub fn update_twaps(
    oracle_twaps: &mut OracleTwaps,
    entry_id: usize,
    price: &DatedPrice,
    twap_enabled_bitmask: TwapEnabledBitmask,
) -> Result<()> {
    let twap = oracle_twaps
        .twaps
        .get_mut(entry_id)
        .ok_or(ScopeError::TwapSourceIndexOutOfRange)?;

    // if there is no previous twap, store the existing
    update_ema_twaps(
        twap,
        price.price,
        price.unix_timestamp,
        price.last_updated_slot,
        twap_enabled_bitmask,
    )?;
    Ok(())
}

pub fn reset_twap(oracle_twaps: &mut OracleTwaps, entry_id: usize) -> Result<()> {
    oracle_twaps
        .twaps
        .get_mut(entry_id)
        .ok_or(ScopeError::TwapSourceIndexOutOfRange)?
        .reset();
    Ok(())
}

pub fn get_price(
    oracle_mappings: &OracleMappings,
    oracle_twaps: &OracleTwaps,
    entry_id: usize,
    twap_type: EmaType,
    clock: &Clock,
) -> ScopeResult<DatedPrice> {
    let source_index = usize::from(oracle_mappings.twap_source[entry_id]);
    debug!("Get twap price at index {source_index} for tk {entry_id}",);

    let twap = oracle_twaps
        .twaps
        .get(source_index)
        .ok_or(ScopeError::TwapSourceIndexOutOfRange)?;

    let current_ts = clock.unix_timestamp.try_into().unwrap();
    utils::validate_ema(twap_type, twap, current_ts)?;

    Ok(twap.as_dated_price(twap_type))
}

mod utils {
    use decimal_wad::decimal::Decimal;

    use super::*;
    use crate::{
        states::{EmaTwap, EmaType},
        warn, Price, ScopeResult,
    };

    /// Get the adjusted smoothing factor (alpha) based on the time between the last two samples.
    ///
    /// N = number of samples per period
    /// alpha = smoothing factor
    /// alpha = 2 / (1 + N)
    /// N' = adjusted number of samples per period
    /// delta t = time between the last two samples
    /// T = ema period
    /// N' = T/delta t
    pub(super) fn get_adjusted_smoothing_factor(
        last_sample_ts: u64,
        current_sample_ts: u64,
        ema_period_s: u64,
    ) -> ScopeResult<Decimal> {
        let last_sample_delta = current_sample_ts.saturating_sub(last_sample_ts);

        if last_sample_delta >= ema_period_s {
            // Smoothing factor is capped at 1
            Ok(Decimal::one())
        // If the new sample is too close to the last one, we skip it (min 30 seconds)
        } else if last_sample_delta < 30 {
            Err(ScopeError::TwapSampleTooFrequent)
        } else {
            let n = Decimal::from(ema_period_s) / last_sample_delta;

            let adjusted_denom = n + Decimal::one();

            Ok(Decimal::from(2) / adjusted_denom)
        }
    }

    fn update_ema_twap(
        twap: &mut EmaTwap,
        price: Price,
        price_ts: u64,
        twap_enabled_bitmask: TwapEnabledBitmask,
        ema_type: EmaType,
        ema_duration_seconds: u64,
    ) -> ScopeResult<bool> {
        if !twap_enabled_bitmask.is_twap_enabled_for_ema_type(ema_type) {
            return Ok(false);
        }

        let (current_ema, updates_tracker) = match ema_type {
            EmaType::Ema1h => (&mut twap.current_ema_1h, &mut twap.updates_tracker_1h),
            EmaType::Ema8h => (&mut twap.current_ema_8h, &mut twap.updates_tracker_8h),
            EmaType::Ema24h => (&mut twap.current_ema_24h, &mut twap.updates_tracker_24h),
        };

        if twap.last_update_slot == 0 {
            *current_ema = Decimal::from(price).to_scaled_val().unwrap();
            return Ok(true);
        }

        let ema_decimal = Decimal::from_scaled_val(*current_ema);
        let price_decimal = Decimal::from(price);

        let smoothing_factor = get_adjusted_smoothing_factor(
            twap.last_update_unix_timestamp,
            price_ts,
            ema_duration_seconds,
        )?;
        let new_ema =
            price_decimal * smoothing_factor + (Decimal::one() - smoothing_factor) * ema_decimal;

        let value = new_ema.to_scaled_val().map_err(|e| {
            msg!("Error when scaling ema value: {e:?}",);
            ScopeError::IntegerOverflow
        })?;

        *current_ema = value;

        let mut tracker: EmaTracker = (*updates_tracker).into();
        tracker.update_tracker(
            ema_duration_seconds,
            price_ts,
            twap.last_update_unix_timestamp,
        );
        *updates_tracker = tracker.into();

        Ok(true)
    }

    /// update the EMAs time weighted on how recent the last price is. EMAs are calculated as:
    /// EMA = (price * smoothing_factor) + (1 - smoothing_factor) * previous_EMA. The smoothing factor is calculated as: (last_sample_delta / sampling_rate_in_seconds) * (2 / (1 + samples_number_per_period)).
    pub(super) fn update_ema_twaps(
        twap: &mut EmaTwap,
        price: Price,
        price_ts: u64,
        price_slot: u64,
        twap_enabled_bitmask: TwapEnabledBitmask,
    ) -> ScopeResult<()> {
        // Skip update if the price is the same as the last one
        if price_slot > twap.last_update_slot {
            let mut performed_update = false;

            performed_update |= update_ema_twap(
                twap,
                price,
                price_ts,
                twap_enabled_bitmask,
                EmaType::Ema1h,
                EMA_1H_DURATION_SECONDS,
            )?;

            performed_update |= update_ema_twap(
                twap,
                price,
                price_ts,
                twap_enabled_bitmask,
                EmaType::Ema8h,
                EMA_8H_DURATION_SECONDS,
            )?;

            performed_update |= update_ema_twap(
                twap,
                price,
                price_ts,
                twap_enabled_bitmask,
                EmaType::Ema24h,
                EMA_24H_DURATION_SECONDS,
            )?;

            if performed_update {
                twap.last_update_slot = price_slot;
                twap.last_update_unix_timestamp = price_ts;
            }
        }
        Ok(())
    }

    pub(super) fn validate_ema(
        twap_type: EmaType,
        twap: &EmaTwap,
        current_ts: u64,
    ) -> ScopeResult<()> {
        if current_ts < twap.last_update_unix_timestamp {
            warn!("Current timestamp is older than the last update timestamp");
            return Err(ScopeError::BadTimestamp);
        }
        let (mut tracker, ema_duration_seconds, min_samples_in_period) = match twap_type {
            EmaType::Ema1h => (
                Into::<EmaTracker>::into(twap.updates_tracker_1h),
                EMA_1H_DURATION_SECONDS,
                MIN_SAMPLES_IN_PERIOD_1H,
            ),
            EmaType::Ema8h => (
                Into::<EmaTracker>::into(twap.updates_tracker_8h),
                EMA_8H_DURATION_SECONDS,
                MIN_SAMPLES_IN_PERIOD_8H,
            ),
            EmaType::Ema24h => (
                Into::<EmaTracker>::into(twap.updates_tracker_24h),
                EMA_24H_DURATION_SECONDS,
                MIN_SAMPLES_IN_PERIOD_24H,
            ),
        };
        tracker.erase_old_samples(
            ema_duration_seconds,
            current_ts,
            twap.last_update_unix_timestamp,
        );

        if tracker.get_samples_count() < min_samples_in_period {
            return Err(ScopeError::TwapNotEnoughSamplesInPeriod);
        }

        let (samples_count_first_subperiod, samples_count_last_subperiod) = match twap_type {
            EmaType::Ema1h => {
                let samples_count_per_subperiods = tracker
                    .get_samples_count_per_subperiods::<NUM_SUB_PERIODS_1H>(
                        ema_duration_seconds,
                        twap.last_update_unix_timestamp,
                    );
                (
                    samples_count_per_subperiods[0],
                    samples_count_per_subperiods[NUM_SUB_PERIODS_1H - 1],
                )
            }
            EmaType::Ema8h => {
                let samples_count_per_subperiods = tracker
                    .get_samples_count_per_subperiods::<NUM_SUB_PERIODS_8H>(
                        ema_duration_seconds,
                        twap.last_update_unix_timestamp,
                    );
                (
                    samples_count_per_subperiods[0],
                    samples_count_per_subperiods[NUM_SUB_PERIODS_8H - 1],
                )
            }
            EmaType::Ema24h => {
                let samples_count_per_subperiods = tracker
                    .get_samples_count_per_subperiods::<NUM_SUB_PERIODS_24H>(
                        ema_duration_seconds,
                        twap.last_update_unix_timestamp,
                    );
                (
                    samples_count_per_subperiods[0],
                    samples_count_per_subperiods[NUM_SUB_PERIODS_24H - 1],
                )
            }
        };

        if samples_count_first_subperiod < MIN_SAMPLES_IN_FIRST_AND_LAST_PERIOD
            || samples_count_last_subperiod < MIN_SAMPLES_IN_FIRST_AND_LAST_PERIOD
        {
            return Err(ScopeError::TwapNotEnoughSamplesInPeriod);
        }

        Ok(())
    }
}

/// The sample tracker is a 64 bit number where each bit represents a point in time.
/// We only track one point per time slot. The time slot being the ema_period / 64.
/// The bit is set to 1 if there is a sample at that point in time slot.
#[derive(Debug, Eq, PartialEq, Clone, Copy, Default)]
#[repr(transparent)]
pub struct EmaTracker(u64);

impl From<EmaTracker> for u64 {
    fn from(tracker: EmaTracker) -> Self {
        tracker.0
    }
}

impl From<u64> for EmaTracker {
    fn from(tracker: u64) -> Self {
        Self(tracker)
    }
}

impl EmaTracker {
    const NB_POINTS: u64 = u64::N_BITS as u64;
    /// Convert a timestamp to a point in the sample tracker
    const fn ts_to_point(ts: u64, ema_period: u64) -> u64 {
        assert!(
            ema_period >= Self::NB_POINTS,
            "EMA period must be bigger than 64 seconds"
        );
        // point_window_size = ema_period / 64
        // points_since_epoch = ts / point_window_size
        // point_index = points_since_epoch % 64
        (ts * Self::NB_POINTS / ema_period) % Self::NB_POINTS
    }

    /// Erase the sample tracker points that are older than the ema_period.
    pub(super) fn erase_old_samples(
        &mut self,
        ema_period: u64,
        current_update_ts: u64,
        last_update_ts: u64,
    ) {
        assert!(
            current_update_ts >= last_update_ts,
            "current_update_ts must be bigger than last_update_ts"
        );
        let sample_tracker = &mut self.0;

        let ts_to_point = |ts| Self::ts_to_point(ts, ema_period);

        let current_point = ts_to_point(current_update_ts);
        // 1. Reset all points up to the current one if needed.
        if last_update_ts + ema_period <= current_update_ts {
            // Reset all points
            *sample_tracker = 0;
        } else {
            let last_update_point = ts_to_point(last_update_ts);
            if last_update_point == current_point {
                // Nothing to reset
                return;
            }

            let first_point_to_clean = (last_update_point + 1) % Self::NB_POINTS; // +1 because we want to reset the point after the last one we updated
            let last_point_to_clean = current_point;

            match first_point_to_clean.cmp(&last_point_to_clean) {
                Ordering::Equal => {
                    // Nothing to reset
                }
                Ordering::Less => {
                    // Reset all points between the first and the last one
                    sample_tracker.set_bits(first_point_to_clean..=last_point_to_clean, 0);
                }
                Ordering::Greater => {
                    sample_tracker.set_bits(first_point_to_clean..Self::NB_POINTS, 0);
                    sample_tracker.set_bits(0..=last_point_to_clean, 0);
                }
            }
        }
    }

    /// Track updates to the EMA
    pub(super) fn update_tracker(
        &mut self,
        ema_period: u64,
        current_update_ts: u64,
        last_update_ts: u64,
    ) {
        // 1. Reset all points up to the current one if needed.
        self.erase_old_samples(ema_period, current_update_ts, last_update_ts);

        // 2. Update the current point.
        let current_point = Self::ts_to_point(current_update_ts, ema_period);
        self.0.set_bit(current_point, true);
    }

    /// Get the number of samples in the last ema_period.
    pub(super) fn get_samples_count(&self) -> u32 {
        self.0.count_ones()
    }

    /// Get the number of samples per each sub-period of the last ema_period.
    /// The number of sub-periods is defined by the const generic parameter N.
    /// The returned array contains the number of samples in each sub-period sorted from the oldest to the newest.
    pub(super) fn get_samples_count_per_subperiods<const N: usize>(
        &self,
        ema_period: u64,
        current_ts: u64,
    ) -> [u32; N] {
        // Sort the points so that the oldest one is the first one.
        let unsorted_points = self.0;
        let current_point = Self::ts_to_point(current_ts, ema_period);
        let pivot_point = (current_point + 1) % Self::NB_POINTS;
        let jonction_point = Self::NB_POINTS - pivot_point;
        let points_oldest = unsorted_points.bits(pivot_point..Self::NB_POINTS);
        let points_newest = unsorted_points.bits(0..pivot_point);
        let sorted_points = points_oldest.with_bits(jonction_point..Self::NB_POINTS, points_newest);

        // Count the number of samples in each sub-period
        let n_u64 = N as u64;
        let sub_period_size = Self::NB_POINTS / n_u64;
        let mut num_sub_periods_with_greater_size: u64 = Self::NB_POINTS - n_u64 * sub_period_size;
        let mut counts = [0; N];

        let count_in_period = |start_point: u64, end_point: u64| -> u32 {
            sorted_points.bits(start_point..end_point).count_ones()
        };

        let mut start_period_point = 0;
        for count in counts.iter_mut() {
            let end_period_point = if num_sub_periods_with_greater_size > 0 {
                num_sub_periods_with_greater_size -= 1;
                start_period_point + sub_period_size + 1
            } else {
                start_period_point + sub_period_size
            };
            *count = count_in_period(start_period_point, end_period_point);
            start_period_point = end_period_point;
        }

        counts
    }
}
