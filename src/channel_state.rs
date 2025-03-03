use crate::{
    ad7172, b_parameter as bp,
    command_parser::{CenterPoint, Polarity},
    config::OutputLimits,
    pid,
};
use core::marker::PhantomData;
use smoltcp::time::{Duration, Instant};
use uom::{
    si::{
        f64::{
            ElectricCurrent, ElectricPotential, ElectricalResistance, ThermodynamicTemperature,
            Time,
        },
        thermodynamic_temperature::degree_celsius,
        time::millisecond,
    },
    ConstZero,
};

const R_INNER: ElectricalResistance = ElectricalResistance {
    dimension: PhantomData,
    units: PhantomData,
    value: 2.0 * 5100.0,
};
const VREF_SENS: ElectricPotential = ElectricPotential {
    dimension: PhantomData,
    units: PhantomData,
    value: 3.3 / 2.0,
};

pub struct ChannelState {
    pub adc_data: Option<u32>,
    pub adc_calibration: ad7172::ChannelCalibration,
    pub adc_time: Instant,
    pub adc_interval: Duration,
    /// i_set 0A center point
    pub center: CenterPoint,
    pub dac_value: ElectricPotential,
    pub i_set: ElectricCurrent,
    pub output_limits: OutputLimits,
    pub pid_engaged: bool,
    pub pid: pid::Controller,
    pub bp: bp::Parameters,
    pub polarity: Polarity,
}

impl ChannelState {
    pub fn new(adc_calibration: ad7172::ChannelCalibration) -> Self {
        ChannelState {
            adc_data: None,
            adc_calibration,
            adc_time: Instant::from_secs(0),
            // default: 10 Hz
            adc_interval: Duration::from_millis(100),
            center: CenterPoint::VRef,
            dac_value: ElectricPotential::ZERO,
            i_set: ElectricCurrent::ZERO,
            output_limits: OutputLimits {
                max_v: ElectricPotential::ZERO,
                max_i_pos: ElectricCurrent::ZERO,
                max_i_neg: ElectricCurrent::ZERO,
            },
            pid_engaged: false,
            pid: pid::Controller::new(pid::Parameters::default()),
            bp: bp::Parameters::default(),
            polarity: Polarity::Normal,
        }
    }

    pub fn update(&mut self, now: Instant, adc_data: u32) {
        self.adc_data = if adc_data == ad7172::MAX_VALUE {
            // this means there is no thermistor plugged into the ADC.
            None
        } else {
            Some(adc_data)
        };
        self.adc_interval = now - self.adc_time;
        self.adc_time = now;
    }

    /// Update PID state on ADC input, calculate new DAC output
    pub fn update_pid(&mut self) -> Option<f64> {
        let temperature = self.get_temperature()?.get::<degree_celsius>();
        let pid_output = self.pid.update(temperature);
        Some(pid_output)
    }

    pub fn get_adc_time(&self) -> Time {
        Time::new::<millisecond>(self.adc_time.total_millis() as f64)
    }

    pub fn get_adc_interval(&self) -> Time {
        Time::new::<millisecond>(self.adc_interval.total_millis() as f64)
    }

    pub fn get_adc(&self) -> Option<ElectricPotential> {
        Some(self.adc_calibration.convert_data(self.adc_data?))
    }

    /// Get `SENS[01]` input resistance
    pub fn get_sens(&self) -> Option<ElectricalResistance> {
        let adc_input = self.get_adc()?;
        let r = R_INNER * adc_input / (VREF_SENS - adc_input);
        Some(r)
    }

    pub fn get_temperature(&self) -> Option<ThermodynamicTemperature> {
        let r = self.get_sens()?;
        let temperature = self.bp.get_temperature(r);
        Some(temperature)
    }
}
