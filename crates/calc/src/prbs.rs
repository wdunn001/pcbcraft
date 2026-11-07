//! PRBS pattern generator and eye folding calculator.
//!
//! PRBS generation uses a linear feedback shift register (LFSR) with
//! standard tap positions. Eye folding takes a waveform and bit rate,
//! aligning samples to unit intervals to compute eye diagram metrics.

use crate::{Error, num, finite};

/// PRBS order configuration.
#[derive(Debug, Clone, Copy)]
pub struct PrbsConfig {
    /// LFSR order (7, 15, 23, or 31).
    pub order: u8,
    /// LFSR tap positions (bit positions for XOR feedback).
    pub taps: [u8; 4],
    /// Initial state (seed).
    pub seed: u32,
}

impl PrbsConfig {
    /// Standard PRBS configurations.
    pub fn for_order(order: u8) -> Result<Self, Error> {
        match order {
            7 => Ok(Self {
                order: 7,
                taps: [6, 5, 0, 0],
                seed: 0x7F,
            }),
            15 => Ok(Self {
                order: 15,
                taps: [14, 13, 0, 0],
                seed: 0x7FFF,
            }),
            23 => Ok(Self {
                order: 23,
                taps: [22, 17, 0, 0],
                seed: 0x7FFFFF,
            }),
            31 => Ok(Self {
                order: 31,
                taps: [30, 28, 0, 0],
                seed: 0x7FFFFFFF,
            }),
            _ => Err(Error::Invalid(format!(
                "unsupported PRBS order: {} (must be 7, 15, 23, or 31)",
                order
            ))),
        }
    }
}

/// PRBS generator state.
#[derive(Debug, Clone)]
pub struct PrbsGenerator {
    /// Current LFSR state.
    state: u32,
    /// Tap positions.
    taps: [u8; 4],
    /// LFSR order.
    order: u8,
}

impl PrbsGenerator {
    /// Create a new PRBS generator with the given configuration.
    pub fn new(config: PrbsConfig) -> Result<Self, Error> {
        // Validate seed is non-zero and within range
        let mask = (1u32 << config.order) - 1;
        if config.seed == 0 || config.seed > mask {
            return Err(Error::Invalid(format!(
                "seed {} must be between 1 and {} for order {}",
                config.seed, mask, config.order
            )));
        }

        Ok(Self {
            state: config.seed & mask,
            taps: config.taps,
            order: config.order,
        })
    }

    /// Generate the next bit (0 or 1).
    pub fn next_bit(&mut self) -> u8 {
        let mask = (1u32 << self.order) - 1;
        let msb = (self.state >> (self.order - 1)) & 1;

        // XOR feedback from tap positions
        let feedback = self.taps.iter().fold(0u32, |acc, &tap| {
            if tap > 0 && tap <= self.order {
                acc ^ ((self.state >> (tap - 1)) & 1)
            } else {
                acc
            }
        });

        // Shift and apply feedback
        self.state = ((self.state << 1) | feedback) & mask;

        msb as u8
    }

    /// Generate a sequence of bits.
    pub fn generate(&mut self, count: usize) -> Vec<u8> {
        (0..count).map(|_| self.next_bit()).collect()
    }

    /// Get the current state.
    pub fn state(&self) -> u32 {
        self.state
    }

    /// Reset to the given state.
    pub fn reset(&mut self, state: u32) {
        let mask = (1u32 << self.order) - 1;
        self.state = state & mask;
    }
}

/// Input for PRBS generation.
#[derive(Debug, Clone)]
pub struct PrbsGenerateInput {
    /// LFSR order (7, 15, 23, or 31).
    pub order: u8,
    /// Number of bits to generate.
    pub count: usize,
    /// Optional seed (default: all 1s for order).
    pub seed: Option<u32>,
}

/// Output from PRBS generation.
#[derive(Debug, Clone)]
pub struct PrbsGenerateOutput {
    /// Generated bit sequence.
    pub bits: Vec<u8>,
    /// Actual number of bits generated.
    pub count: usize,
    /// Period of the sequence (2^order - 1).
    pub period: u64,
}

/// Generate a PRBS sequence.
pub fn prbs_generate(input: PrbsGenerateInput) -> Result<PrbsGenerateOutput, Error> {
    let config = PrbsConfig::for_order(input.order)?;
    let seed = input.seed.unwrap_or(config.seed);
    let mut gen = PrbsGenerator::new(PrbsConfig {
        seed,
        ..config
    })?;

    let bits = gen.generate(input.count);
    let period = (1u64 << input.order) - 1;

    Ok(PrbsGenerateOutput {
        bits,
        count: bits.len(),
        period,
    })
}

/// Input for eye folding.
#[derive(Debug, Clone)]
pub struct EyeFoldInput {
    /// Waveform samples (voltage values).
    pub waveform: Vec<f64>,
    /// Sampling rate in Hz.
    pub sample_rate: f64,
    /// Bit rate in bps.
    pub bit_rate: f64,
}

/// Output from eye folding.
#[derive(Debug, Clone)]
pub struct EyeFoldOutput {
    /// Eye height (vertical opening) in Volts.
    pub eye_height: f64,
    /// Eye width (horizontal opening) in seconds.
    pub eye_width: f64,
    /// Unit interval in seconds.
    pub unit_interval: f64,
    /// Number of bits analyzed.
    pub bits_analyzed: usize,
}

/// Fold a waveform into an eye diagram and compute metrics.
pub fn eye_fold(input: EyeFoldInput) -> Result<EyeFoldOutput, Error> {
    let sample_rate = num("sample_rate", input.sample_rate, 1.0, 1.0e15)?;
    let bit_rate = num("bit_rate", input.bit_rate, 1.0, 1.0e12)?;

    if input.waveform.is_empty() {
        return Err(Error::Invalid("waveform must not be empty".to_string()));
    }

    // Compute unit interval
    let ui = finite("unit_interval", 1.0 / bit_rate)?;

    // Find bit boundaries by detecting transitions
    // Simplified: assume uniform bit rate
    let samples_per_bit = (sample_rate / bit_rate) as usize;

    if samples_per_bit == 0 {
        return Err(Error::Invalid(format!(
            "sample_rate must be >= bit_rate (got {} vs {})",
            sample_rate, bit_rate
        )));
    }

    let num_bits = input.waveform.len() / samples_per_bit;
    if num_bits == 0 {
test
