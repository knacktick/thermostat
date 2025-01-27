use core::fmt;
use num_traits::float::Float;
use serde::{Deserialize, Serialize};
use stm32f4xx_hal::{spi, time::MegaHertz};

mod checksum;
pub mod regs;
pub use checksum::ChecksumMode;
mod adc;
pub use adc::*;

/// SPI Mode 3
pub const SPI_MODE: spi::Mode = spi::Mode {
    polarity: spi::Polarity::IdleHigh,
    phase: spi::Phase::CaptureOnSecondTransition,
};
/// 2 MHz
pub const SPI_CLOCK: MegaHertz = MegaHertz(2);

pub const MAX_VALUE: u32 = 0xFF_FFFF;

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum Mode {
    ContinuousConversion = 0b000,
    SingleConversion = 0b001,
    Standby = 0b010,
    PowerDown = 0b011,
    InternalOffsetCalibration = 0b100,
    Invalid,
    SystemOffsetCalibration = 0b110,
    SystemGainCalibration = 0b111,
}

impl From<u8> for Mode {
    fn from(x: u8) -> Self {
        use Mode::*;
        match x {
            0b000 => ContinuousConversion,
            0b001 => SingleConversion,
            0b010 => Standby,
            0b011 => PowerDown,
            0b100 => InternalOffsetCalibration,
            0b110 => SystemOffsetCalibration,
            0b111 => SystemGainCalibration,
            _ => Invalid,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum Input {
    Ain0 = 0,
    Ain1 = 1,
    Ain2 = 2,
    Ain3 = 3,
    Ain4 = 4,
    TemperaturePos = 17,
    TemperatureNeg = 18,
    AnalogSupplyPos = 19,
    AnalogSupplyNeg = 20,
    RefPos = 21,
    RefNeg = 22,
    Invalid = 0b11111,
}

impl From<u8> for Input {
    fn from(x: u8) -> Self {
        match x {
            0 => Input::Ain0,
            1 => Input::Ain1,
            2 => Input::Ain2,
            3 => Input::Ain3,
            4 => Input::Ain4,
            17 => Input::TemperaturePos,
            18 => Input::TemperatureNeg,
            19 => Input::AnalogSupplyPos,
            20 => Input::AnalogSupplyNeg,
            21 => Input::RefPos,
            22 => Input::RefNeg,
            _ => Input::Invalid,
        }
    }
}

impl fmt::Display for Input {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        use Input::*;

        match self {
            Ain0 => "ain0",
            Ain1 => "ain1",
            Ain2 => "ain2",
            Ain3 => "ain3",
            Ain4 => "ain4",
            TemperaturePos => "temperature+",
            TemperatureNeg => "temperature-",
            AnalogSupplyPos => "analogsupply+",
            AnalogSupplyNeg => "analogsupply-",
            RefPos => "ref+",
            RefNeg => "ref-",
            _ => "<INVALID>",
        }
        .fmt(fmt)
    }
}

/// Reference source for ADC conversion
#[repr(u8)]
pub enum RefSource {
    /// External reference
    External = 0b00,
    /// Internal 2.5V reference
    Internal = 0b10,
    /// AVDD1 − AVSS
    Avdd1MinusAvss = 0b11,
    Invalid = 0b01,
}

impl From<u8> for RefSource {
    fn from(x: u8) -> Self {
        match x {
            0 => RefSource::External,
            1 => RefSource::Internal,
            2 => RefSource::Avdd1MinusAvss,
            _ => RefSource::Invalid,
        }
    }
}

impl fmt::Display for RefSource {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        use RefSource::*;

        match self {
            External => "external",
            Internal => "internal",
            Avdd1MinusAvss => "avdd1-avss",
            _ => "<INVALID>",
        }
        .fmt(fmt)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
/// Simultaneous Rejection of 50 Hz +/- 1 Hz and 60 Hz +/- 1 Hz
pub enum PostFilter {
    /// Output Data Rate: 27.27 SPS,
    /// Settling Time: 36.67 ms,
    /// Rejection: 47 dB
    F27SPS = 0b010,

    /// Output Data Rate: 25 SPS,
    /// Settling Time: 40.0 ms,
    /// Rejection: 62 dB
    F25SPS = 0b011,

    /// Output Data Rate: 20 SPS,
    /// Settling Time: 50.0 ms,
    /// Rejection: 85 dB
    F20SPS = 0b101,

    /// Output Data Rate: 16.667 SPS,
    /// Settling Time: 60.0 ms,
    /// Rejection: 90 dB
    F16SPS = 0b110,

    Invalid = 0b111,
}

impl PostFilter {
    pub const VALID_VALUES: &'static [Self] = &[
        PostFilter::F27SPS,
        PostFilter::F25SPS,
        PostFilter::F20SPS,
        PostFilter::F16SPS,
    ];

    pub fn closest(rate: f32) -> Option<Self> {
        let mut best: Option<(f32, Self)> = None;
        for value in Self::VALID_VALUES {
            let error = (rate - value.output_rate().unwrap()).abs();
            let better = best
                .map(|(best_error, _)| error < best_error)
                .unwrap_or(true);
            if better {
                best = Some((error, *value));
            }
        }
        best.map(|(_, best)| best)
    }

    /// Samples per Second
    pub fn output_rate(&self) -> Option<f32> {
        match self {
            PostFilter::F27SPS => Some(27.27),
            PostFilter::F25SPS => Some(25.0),
            PostFilter::F20SPS => Some(20.0),
            PostFilter::F16SPS => Some(16.667),
            PostFilter::Invalid => None,
        }
    }
}

impl From<u8> for PostFilter {
    fn from(x: u8) -> Self {
        match x {
            0b010 => PostFilter::F27SPS,
            0b011 => PostFilter::F25SPS,
            0b101 => PostFilter::F20SPS,
            0b110 => PostFilter::F16SPS,
            _ => PostFilter::Invalid,
        }
    }
}

#[repr(u8)]
pub enum DigitalFilterOrder {
    Sinc5Sinc1 = 0b00,
    Sinc3 = 0b11,
    Invalid = 0b10,
}

impl From<u8> for DigitalFilterOrder {
    fn from(x: u8) -> Self {
        match x {
            0b00 => DigitalFilterOrder::Sinc5Sinc1,
            0b11 => DigitalFilterOrder::Sinc3,
            _ => DigitalFilterOrder::Invalid,
        }
    }
}
