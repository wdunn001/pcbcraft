//! Pseudo-random bit sequences and eye folding: `prbs_generate` makes a
//! maximal-length bit pattern and `eye_fold` reads the opening of an eye
//! from a sampled waveform.
//!
//! Context: signal integrity work (#1864). Both functions are pure,
//! bounded and deterministic, so the same inputs give the same bits and
//! the same numbers on every machine. No simulation and no screen use
//! them yet; mask comparisons wait for the mask data in #1865.
//!
//! Local rules:
//! - A sequence is a Fibonacci shift register with two taps, the highest
//!   bit and one lower bit, which are the trinomials x^7+x^6+1,
//!   x^15+x^14+1, x^23+x^18+1 and x^31+x^28+1. Each has period 2^n-1.
//!   The output bit at each step is the feedback bit shifted in.
//! - A seed of zero locks the register, so it is refused.
//! - `eye_fold` assumes the waveform starts on a unit interval boundary
//!   and has a whole number of samples per interval. The eye centre is
//!   the sample at half an interval. Eye height is the lowest sample
//!   above the threshold minus the highest sample at or below it, taken
//!   at the centre phase over all intervals. Eye width is the run of
//!   consecutive sample steps around the centre in which no trace
//!   crosses the threshold, as a fraction of one interval. The threshold
//!   is the midpoint of the lowest and highest sample.
//!
//! # Dependencies
//! - crates/calc/src/lib.rs (`Error`, `num`, `int_num`, `finite`)
//!
//! # Do not
//! - Add a screen, a simulation or mask data here; those have their own
//!   issues.
//! - Read the clock or a random source; the output must repeat exactly.

use crate::{Error, finite, int_num, num};

/// Most bits one call may produce.
pub const MAX_BITS: usize = 1_000_000;

/// Fewest unit intervals `eye_fold` accepts, so every phase sees both levels.
pub const MIN_INTERVALS: usize = 4;

/// Input for `prbs_generate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrbsGenerateInput {
    /// Register length: 7, 15, 23 or 31.
    pub order: u8,
    /// Number of bits to produce, 1 to `MAX_BITS`.
    pub count: usize,
    /// Starting register value, 1 to 2^order-1; all ones when absent.
    pub seed: Option<u32>,
}

/// Output of `prbs_generate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrbsGenerateOutput {
    /// The bits, each 0 or 1.
    pub bits: Vec<u8>,
    /// How many bits the pattern has before it repeats, 2^order-1.
    pub period: u64,
}

/// The lower tap of the trinomial for a supported order.
fn lower_tap(order: u8) -> Result<u8, Error> {
    match order {
        7 => Ok(6),
        15 => Ok(14),
        23 => Ok(18),
        31 => Ok(28),
        other => Err(Error::Invalid(format!(
            "order must be 7, 15, 23 or 31, got {other}"
        ))),
    }
}

/// Make a maximal-length bit pattern.
pub fn prbs_generate(input: &PrbsGenerateInput) -> Result<PrbsGenerateOutput, Error> {
    let low = lower_tap(input.order)?;
    let order = u32::from(input.order);
    let mask: u32 = ((1u64 << order) - 1) as u32;
    let count = int_num("count", input.count as i64, 1, MAX_BITS as i64)? as usize;
    let seed = match input.seed {
        None => mask,
        Some(s) => int_num("seed", i64::from(s), 1, i64::from(mask))? as u32,
    };
    let (high_shift, low_shift) = (order - 1, u32::from(low) - 1);
    let mut state = seed;
    let mut bits = Vec::with_capacity(count);
    for _ in 0..count {
        let feedback = ((state >> high_shift) ^ (state >> low_shift)) & 1;
        state = ((state << 1) | feedback) & mask;
        bits.push(feedback as u8);
    }
    Ok(PrbsGenerateOutput {
        bits,
        period: (1u64 << order) - 1,
    })
}

/// Input for `eye_fold`.
#[derive(Debug, Clone, PartialEq)]
pub struct EyeFoldInput {
    /// Waveform samples in volts, starting on a unit interval boundary.
    pub waveform: Vec<f64>,
    /// Whole samples in one unit interval, 4 to 4096.
    pub samples_per_ui: usize,
}

/// Output of `eye_fold`.
#[derive(Debug, Clone, PartialEq)]
pub struct EyeFoldOutput {
    /// Vertical opening at the centre phase in volts, never below zero.
    pub eye_height: f64,
    /// Horizontal opening at the threshold as a fraction of one interval.
    pub eye_width_ui: f64,
    /// The decision threshold in volts.
    pub threshold: f64,
    /// Whole unit intervals folded.
    pub intervals: usize,
    /// Sample index of the eye centre inside an interval.
    pub centre_phase: usize,
}

/// Fold a waveform into one unit interval and read the eye opening.
pub fn eye_fold(input: &EyeFoldInput) -> Result<EyeFoldOutput, Error> {
    let spu = int_num("samples_per_ui", input.samples_per_ui as i64, 4, 4096)? as usize;
    let wave = &input.waveform;
    if wave.len() < spu * MIN_INTERVALS {
        return Err(Error::Invalid(format!(
            "waveform needs at least {} samples ({MIN_INTERVALS} intervals of {spu}), got {}",
            spu * MIN_INTERVALS,
            wave.len()
        )));
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for &v in wave {
        let v = num("sample", v, -1.0e6, 1.0e6)?;
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if hi <= lo {
        return Err(Error::Invalid(
            "waveform is flat, so there is no eye to measure".to_string(),
        ));
    }
    let threshold = finite("threshold", (hi + lo) / 2.0)?;
    let intervals = wave.len() / spu;
    let centre = spu / 2;
    let mut top = f64::INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    for k in 0..intervals {
        let v = wave[k * spu + centre];
        if v > threshold {
            top = top.min(v);
        } else {
            bottom = bottom.max(v);
        }
    }
    if !top.is_finite() || !bottom.is_finite() {
        return Err(Error::Invalid(
            "the centre samples all sit on one side of the threshold, so the eye is closed"
                .to_string(),
        ));
    }
    // A sample step is crossed when its two ends are strictly on opposite
    // sides of the threshold. Step p runs from phase p to phase p+1, and the
    // last step of an interval runs into the first sample of the next one.
    let mut crossed = vec![false; spu];
    for i in 0..(intervals * spu).saturating_sub(1) {
        if (wave[i] - threshold) * (wave[i + 1] - threshold) < 0.0 {
            crossed[i % spu] = true;
        }
    }
    // Count clear steps outward from the centre: right first, then left, each
    // side stopping at the first crossed step, and never more than one lap.
    let mut right = 0usize;
    while right < spu && !crossed[(centre + right) % spu] {
        right += 1;
    }
    let mut left = 0usize;
    while right + left < spu && !crossed[(centre + spu - 1 - left) % spu] {
        left += 1;
    }
    let run = right + left;
    Ok(EyeFoldOutput {
        eye_height: finite("eye_height", (top - bottom).max(0.0))?,
        eye_width_ui: run as f64 / spu as f64,
        threshold,
        intervals,
        centre_phase: centre,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(order: u8, count: usize, seed: Option<u32>) -> Result<Vec<u8>, Error> {
        Ok(prbs_generate(&PrbsGenerateInput { order, count, seed })?.bits)
    }

    /// Steps until the register returns to its start, and whether every
    /// register value from 1 to 2^n-1 was visited exactly once on the way.
    fn cycle(order: u8) -> Result<(u64, bool), Error> {
        let low = lower_tap(order)?;
        let n = u32::from(order);
        let mask = ((1u64 << n) - 1) as u32;
        let mut seen = vec![false; (mask as usize) + 1];
        let mut state = mask;
        let mut steps = 0u64;
        let mut distinct = true;
        loop {
            if seen[state as usize] {
                distinct = false;
            }
            seen[state as usize] = true;
            let fb = ((state >> (n - 1)) ^ (state >> (u32::from(low) - 1))) & 1;
            state = ((state << 1) | fb) & mask;
            steps += 1;
            if state == mask {
                break;
            }
        }
        let all = (1..=mask as usize).all(|s| seen[s]);
        Ok((steps, distinct && all))
    }

    #[test]
    fn prbs7_starts_with_the_hand_traced_bits() -> Result<(), Error> {
        // Register 1111111, taps on bits 7 and 6 (counting from 1). The first
        // six steps shift the ones out and feed back 0; the seventh sees
        // bits 1 and 0 and feeds back 1; five more zeros follow; then
        // 0100000 and 1000001 each feed back 1.
        let got = bits(7, 14, None)?;
        let want = [0u8, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 1];
        assert_eq!(got, want);
        Ok(())
    }

    #[test]
    fn the_period_is_two_to_the_order_minus_one_and_visits_every_state() -> Result<(), Error> {
        for order in [7u8, 15, 23] {
            let (steps, all_once) = cycle(order)?;
            assert_eq!(steps, (1u64 << order) - 1, "period of PRBS{order}");
            assert!(all_once, "PRBS{order} visits every non-zero state once");
        }
        let out = prbs_generate(&PrbsGenerateInput {
            order: 31,
            count: 8,
            seed: None,
        })?;
        assert_eq!(out.period, (1u64 << 31) - 1);
        Ok(())
    }

    #[test]
    fn the_sequence_repeats_after_one_period() -> Result<(), Error> {
        let all = bits(7, 254, None)?;
        assert_eq!(all[..127], all[127..]);
        // Balanced: a maximal sequence has 64 ones and 63 zeros per period.
        let ones = all[..127].iter().filter(|&&b| b == 1).count();
        assert_eq!(ones, 64);
        Ok(())
    }

    #[test]
    fn a_seed_changes_where_the_sequence_starts_not_what_it_is() -> Result<(), Error> {
        let a = bits(7, 254, None)?;
        let b = bits(7, 127, Some(0x55))?;
        let doubled: Vec<u8> = a.iter().chain(a.iter()).copied().collect();
        let found = (0..127).any(|s| doubled[s..s + 127] == b[..]);
        assert!(found, "a seeded run is a rotation of the default run");
        Ok(())
    }

    #[test]
    fn bad_prbs_inputs_are_refused_with_a_plain_message() {
        let base = PrbsGenerateInput {
            order: 7,
            count: 8,
            seed: None,
        };
        let bad_order = prbs_generate(&PrbsGenerateInput {
            order: 9,
            ..base.clone()
        });
        assert!(matches!(bad_order, Err(Error::Invalid(m)) if m.contains("7, 15, 23 or 31")));
        let zero = prbs_generate(&PrbsGenerateInput {
            seed: Some(0),
            ..base.clone()
        });
        assert!(matches!(zero, Err(Error::OutOfRange { .. })));
        let big = prbs_generate(&PrbsGenerateInput {
            seed: Some(128),
            ..base.clone()
        });
        assert!(matches!(big, Err(Error::OutOfRange { .. })));
        let none = prbs_generate(&PrbsGenerateInput {
            count: 0,
            ..base.clone()
        });
        assert!(matches!(none, Err(Error::OutOfRange { .. })));
        let many = prbs_generate(&PrbsGenerateInput {
            count: MAX_BITS + 1,
            ..base
        });
        assert!(matches!(many, Err(Error::OutOfRange { .. })));
    }

    /// An NRZ waveform with a straight four-sample edge across each interval
    /// boundary: the two samples before it and the two after it step from the
    /// old level to the new one in fifths (rising: -0.6 -0.2 +0.2 +0.6).
    fn nrz(levels: &[f64], spu: usize) -> Vec<f64> {
        let mut out: Vec<f64> = levels
            .iter()
            .flat_map(|&level| std::iter::repeat_n(level, spu))
            .collect();
        for k in 1..levels.len() {
            let (from, to) = (levels[k - 1], levels[k]);
            if from == to {
                continue;
            }
            let b = k * spu;
            for (j, frac) in [0.2, 0.4, 0.6, 0.8].into_iter().enumerate() {
                out[b - 2 + j] = from + (to - from) * frac;
            }
        }
        out
    }

    #[test]
    fn a_newcomer_reads_eye_height_and_width_from_a_waveform() -> Result<(), Error> {
        // Levels of plus and minus one volt, eight samples per interval, a
        // four-sample edge across each boundary. Rising edge samples, in
        // order: -0.6 -0.2 +0.2 +0.6. Only the step between -0.2 and +0.2
        // crosses the threshold, and that step is the last one of an
        // interval (phase 7). The centre is phase 4. Clear steps run from
        // phase 4 through phase 6 on the right and 3 down to 0 on the left,
        // which is 7 of 8 steps, so the width is 7/8 of an interval. The
        // centre samples are all plus or minus one, so the height is 2.
        let levels = [
            -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0,
        ];
        let wave = nrz(&levels, 8);
        let eye = eye_fold(&EyeFoldInput {
            waveform: wave,
            samples_per_ui: 8,
        })?;
        assert!(
            (eye.eye_height - 2.0).abs() < 1e-12,
            "height {}",
            eye.eye_height
        );
        assert!(
            (eye.eye_width_ui - 0.875).abs() < 1e-12,
            "width {}",
            eye.eye_width_ui
        );
        assert!(eye.threshold.abs() < 1e-12);
        assert_eq!(eye.intervals, 12);
        assert_eq!(eye.centre_phase, 4);
        Ok(())
    }

    #[test]
    fn a_noisy_centre_narrows_the_height_by_the_noise() -> Result<(), Error> {
        let levels = [-1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0];
        let mut wave = nrz(&levels, 8);
        // Pull one high centre sample down to 0.7 and one low one up to -0.5.
        wave[8 + 4] = 0.7;
        wave[2 * 8 + 4] = -0.5;
        let eye = eye_fold(&EyeFoldInput {
            waveform: wave,
            samples_per_ui: 8,
        })?;
        assert!(
            (eye.eye_height - 1.2).abs() < 1e-12,
            "height {}",
            eye.eye_height
        );
        Ok(())
    }

    #[test]
    fn bad_waveforms_are_refused_with_a_plain_message() {
        let ok = vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0];
        let short = eye_fold(&EyeFoldInput {
            waveform: ok,
            samples_per_ui: 4,
        });
        assert!(matches!(short, Err(Error::Invalid(m)) if m.contains("at least 16 samples")));
        let flat = eye_fold(&EyeFoldInput {
            waveform: vec![0.5; 32],
            samples_per_ui: 4,
        });
        assert!(matches!(flat, Err(Error::Invalid(m)) if m.contains("flat")));
        let nan = eye_fold(&EyeFoldInput {
            waveform: {
                let mut w = vec![0.0; 32];
                w[3] = f64::NAN;
                w
            },
            samples_per_ui: 4,
        });
        assert!(matches!(nan, Err(Error::Invalid(m)) if m.contains("finite")));
        let narrow = eye_fold(&EyeFoldInput {
            waveform: vec![0.0; 64],
            samples_per_ui: 2,
        });
        assert!(matches!(narrow, Err(Error::OutOfRange { .. })));
    }
}
