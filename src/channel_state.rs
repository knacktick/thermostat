use smoltcp::time::{Duration, Instant};
use uom::si::{
    f64::{
        ElectricPotential,
        ElectricCurrent,
        ElectricalResistance,
        ThermodynamicTemperature,
        Time,
    },
    electric_potential::volt,
    electric_current::ampere,
    electrical_resistance::ohm,
    thermodynamic_temperature::degree_celsius,
    time::millisecond,
};
use crate::{
    ad7172,
    pid,
    config::PwmLimits,
    steinhart_hart as sh,
    command_parser::{CenterPoint, Polarity},
};

const R_INNER: f64 = 2.0 * 5100.0;
const VREF_SENS: f64 = 3.3 / 2.0;

pub struct ChannelState {
    pub adc_data: Option<u32>,
    pub adc_calibration: ad7172::ChannelCalibration,
    pub adc_time: Instant,
    pub adc_interval: Duration,
    /// i_set 0A center point
    pub center: CenterPoint,
    pub dac_value: ElectricPotential,
    pub i_set: ElectricCurrent,
    pub pwm_limits: PwmLimits,
    pub pid_engaged: bool,
    pub pid: pid::Controller,
    pub sh: sh::Parameters,
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
            center: CenterPoint::Vref,
            dac_value: ElectricPotential::new::<volt>(0.0),
            i_set: ElectricCurrent::new::<ampere>(0.0),
            pwm_limits: PwmLimits {
                max_v: 0.0,
                max_i_pos: 0.0,
                max_i_neg: 0.0,
            },
            pid_engaged: false,
            pid: pid::Controller::new(pid::Parameters::default()),
            sh: sh::Parameters::default(),
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
        let temperature = self.get_temperature()?
            .get::<degree_celsius>();
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
        let r_inner = ElectricalResistance::new::<ohm>(R_INNER);
        let vref = ElectricPotential::new::<volt>(VREF_SENS);
        let adc_input = self.get_adc()?;
        let r = r_inner * adc_input / (vref - adc_input);
        Some(r)
    }

    pub fn get_temperature(&self) -> Option<ThermodynamicTemperature> {
        let r = self.get_sens()?;
        let temperature = self.sh.get_temperature(r);
        Some(temperature)
    }
}
