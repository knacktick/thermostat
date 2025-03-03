use crate::timer::sleep;
use crate::{
    ad5680,
    ad7172::{self, PostFilter},
    b_parameter,
    channel::{Channel, Channel0, Channel1},
    channel_state::ChannelState,
    command_handler::JsonBuffer,
    command_parser::{CenterPoint, Polarity, PwmPin},
    pins::{self, Channel0VRef, Channel1VRef},
};
use core::marker::PhantomData;
use heapless::{consts::U2, Vec};
use num_traits::Zero;
use serde::{Serialize, Serializer};
use smoltcp::time::Instant;
use stm32f4xx_hal::hal;
use uom::si::{
    electric_current::ampere,
    electric_potential::{millivolt, volt},
    electrical_resistance::ohm,
    f64::{ElectricCurrent, ElectricPotential, ElectricalResistance, Time},
    ratio::ratio,
    thermodynamic_temperature::degree_celsius,
};

pub enum PinsAdcReadTarget {
    VRef,
    DacVfb,
    ITec,
    VTec,
}

pub const CHANNELS: usize = 2;
const R_SENSE: ElectricalResistance = ElectricalResistance {
    dimension: PhantomData,
    units: PhantomData,
    value: 0.05,
};

const CPU_ADC_VREF: ElectricPotential = ElectricPotential {
    dimension: PhantomData,
    units: PhantomData,
    value: 3.3,
};

// From design specs
pub const MAX_TEC_I: ElectricCurrent = ElectricCurrent {
    dimension: PhantomData,
    units: PhantomData,
    value: 2.0,
};
pub const MAX_TEC_V: ElectricPotential = ElectricPotential {
    dimension: PhantomData,
    units: PhantomData,
    value: 4.0,
};
// DAC chip outputs 0-5v, which is then passed through a resistor dividor to provide 0-3v range
const DAC_OUT_V_MAX: ElectricPotential = ElectricPotential {
    dimension: PhantomData,
    units: PhantomData,
    value: 3.0,
};

pub struct Channels {
    channel0: Channel<Channel0>,
    channel1: Channel<Channel1>,
    adc: ad7172::Adc<pins::AdcSpi, pins::AdcNss>,
    /// stm32f4 integrated adc
    pins_adc: pins::PinsAdc,
    pwm: pins::PwmPins,
}

impl Channels {
    pub fn new(pins: pins::Pins) -> Self {
        let mut adc = ad7172::Adc::new(pins.adc_spi, pins.adc_nss).unwrap();
        // Feature not used
        adc.set_sync_enable(false).unwrap();

        // Setup channels and start ADC
        adc.setup_channel(0, ad7172::Input::Ain2, ad7172::Input::Ain3)
            .unwrap();
        let adc_calibration0 = adc.get_calibration(0).expect("adc_calibration0");
        adc.setup_channel(1, ad7172::Input::Ain0, ad7172::Input::Ain1)
            .unwrap();
        let adc_calibration1 = adc.get_calibration(1).expect("adc_calibration1");
        adc.start_continuous_conversion().unwrap();

        let channel0 = Channel::new(pins.channel0, adc_calibration0);
        let channel1 = Channel::new(pins.channel1, adc_calibration1);
        let pins_adc = pins.pins_adc;
        let pwm = pins.pwm;
        let mut channels = Channels {
            channel0,
            channel1,
            adc,
            pins_adc,
            pwm,
        };
        for channel in 0..CHANNELS {
            channels.calibrate_dac_value(channel);
            channels.set_i(channel, ElectricCurrent::new::<ampere>(0.0));
        }
        channels
    }

    pub fn channel_state<I: Into<usize>>(&mut self, channel: I) -> &mut ChannelState {
        match channel.into() {
            0 => &mut self.channel0.state,
            1 => &mut self.channel1.state,
            _ => unreachable!(),
        }
    }

    /// ADC input + PID processing
    pub fn poll_adc(&mut self, instant: Instant) -> Option<u8> {
        self.adc.data_ready().unwrap().map(|channel| {
            let data = self.adc.read_data().unwrap();
            let state = self.channel_state(channel);
            state.update(instant, data);
            match state.update_pid() {
                Some(pid_output) if state.pid_engaged => {
                    // Forward PID output to i_set DAC
                    self.set_i(channel.into(), ElectricCurrent::new::<ampere>(pid_output));
                    self.power_up(channel);
                }
                None if state.pid_engaged => {
                    self.power_down(channel);
                }
                _ => {}
            }

            channel
        })
    }

    /// calculate the TEC i_set centerpoint
    pub fn get_center(&mut self, channel: usize) -> ElectricPotential {
        match self.channel_state(channel).center {
            CenterPoint::VRef => self.adc_read(channel, PinsAdcReadTarget::VRef, 8),
            CenterPoint::Override(center_point) => {
                ElectricPotential::new::<volt>(center_point.into())
            }
        }
    }

    /// i_set DAC
    fn get_dac(&mut self, channel: usize) -> ElectricPotential {
        let voltage = self.channel_state(channel).dac_value;
        voltage
    }

    pub fn get_i_set(&mut self, channel: usize) -> ElectricCurrent {
        let i_set = self.channel_state(channel).i_set;
        i_set
    }

    /// i_set DAC
    fn set_dac(&mut self, channel: usize, voltage: ElectricPotential) -> ElectricPotential {
        let value = ((voltage / DAC_OUT_V_MAX).get::<ratio>() * (ad5680::MAX_VALUE as f64)) as u32;
        match channel {
            0 => self.channel0.dac.set(value).unwrap(),
            1 => self.channel1.dac.set(value).unwrap(),
            _ => unreachable!(),
        };
        self.channel_state(channel).dac_value = voltage;
        voltage
    }

    pub fn set_i(&mut self, channel: usize, i_set: ElectricCurrent) -> ElectricCurrent {
        let i_set = i_set.min(MAX_TEC_I).max(-MAX_TEC_I);
        self.channel_state(channel).i_set = i_set;
        let negate = match self.channel_state(channel).polarity {
            Polarity::Normal => 1.0,
            Polarity::Reversed => -1.0,
        };
        let vref_meas = match channel {
            0 => self.channel0.vref_meas,
            1 => self.channel1.vref_meas,
            _ => unreachable!(),
        };
        let center_point = vref_meas;
        let voltage = negate * i_set * 10.0 * R_SENSE + center_point;
        let voltage = self.set_dac(channel, voltage);

        negate * (voltage - center_point) / (10.0 * R_SENSE)
    }

    /// AN4073: ADC Reading Dispersion can be reduced through Averaging
    pub fn adc_read(
        &mut self,
        channel: usize,
        adc_read_target: PinsAdcReadTarget,
        avg_pt: u16,
    ) -> ElectricPotential {
        let mut sample: u32 = 0;
        match channel {
            0 => {
                sample = match adc_read_target {
                    PinsAdcReadTarget::VRef => match &self.channel0.vref_pin {
                        Channel0VRef::Analog(vref_pin) => {
                            for _ in (0..avg_pt).rev() {
                                sample += self.pins_adc.convert(
                                    vref_pin,
                                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480,
                                ) as u32;
                            }
                            sample / avg_pt as u32
                        }
                        Channel0VRef::Disabled(_) => 2048_u32,
                    },
                    PinsAdcReadTarget::DacVfb => {
                        for _ in (0..avg_pt).rev() {
                            sample += self.pins_adc.convert(
                                &self.channel0.dac_feedback_pin,
                                stm32f4xx_hal::adc::config::SampleTime::Cycles_480,
                            ) as u32;
                        }
                        sample / avg_pt as u32
                    }
                    PinsAdcReadTarget::ITec => {
                        for _ in (0..avg_pt).rev() {
                            sample += self.pins_adc.convert(
                                &self.channel0.itec_pin,
                                stm32f4xx_hal::adc::config::SampleTime::Cycles_480,
                            ) as u32;
                        }
                        sample / avg_pt as u32
                    }
                    PinsAdcReadTarget::VTec => {
                        for _ in (0..avg_pt).rev() {
                            sample += self.pins_adc.convert(
                                &self.channel0.tec_u_meas_pin,
                                stm32f4xx_hal::adc::config::SampleTime::Cycles_480,
                            ) as u32;
                        }
                        sample / avg_pt as u32
                    }
                };
                let mv = self.pins_adc.sample_to_millivolts(sample as u16);
                ElectricPotential::new::<millivolt>(mv as f64)
            }
            1 => {
                sample = match adc_read_target {
                    PinsAdcReadTarget::VRef => match &self.channel1.vref_pin {
                        Channel1VRef::Analog(vref_pin) => {
                            for _ in (0..avg_pt).rev() {
                                sample += self.pins_adc.convert(
                                    vref_pin,
                                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480,
                                ) as u32;
                            }
                            sample / avg_pt as u32
                        }
                        Channel1VRef::Disabled(_) => 2048_u32,
                    },
                    PinsAdcReadTarget::DacVfb => {
                        for _ in (0..avg_pt).rev() {
                            sample += self.pins_adc.convert(
                                &self.channel1.dac_feedback_pin,
                                stm32f4xx_hal::adc::config::SampleTime::Cycles_480,
                            ) as u32;
                        }
                        sample / avg_pt as u32
                    }
                    PinsAdcReadTarget::ITec => {
                        for _ in (0..avg_pt).rev() {
                            sample += self.pins_adc.convert(
                                &self.channel1.itec_pin,
                                stm32f4xx_hal::adc::config::SampleTime::Cycles_480,
                            ) as u32;
                        }
                        sample / avg_pt as u32
                    }
                    PinsAdcReadTarget::VTec => {
                        for _ in (0..avg_pt).rev() {
                            sample += self.pins_adc.convert(
                                &self.channel1.tec_u_meas_pin,
                                stm32f4xx_hal::adc::config::SampleTime::Cycles_480,
                            ) as u32;
                        }
                        sample / avg_pt as u32
                    }
                };
                let mv = self.pins_adc.sample_to_millivolts(sample as u16);
                ElectricPotential::new::<millivolt>(mv as f64)
            }
            _ => unreachable!(),
        }
    }

    /// Calibrates the DAC output to match vref of the MAX driver to reduce zero-current offset of the MAX driver output.
    ///
    /// The thermostat DAC applies a control voltage signal to the CTLI pin of MAX driver chip to control its output current.
    /// The CTLI input signal is centered around VREF of the MAX chip. Applying VREF to CTLI sets the output current to 0.
    ///
    /// This calibration routine measures the VREF voltage and the DAC output with the STM32 ADC, and uses a breadth-first     
    /// search to find the DAC setting that will produce a DAC output voltage closest to VREF. This DAC output voltage will
    /// be stored and used in subsequent i_set routines to bias the current control signal to the measured VREF, reducing
    /// the offset error of the current control signal.
    ///
    /// The input offset of the STM32 ADC is eliminated by using the same ADC for the measurements, and by only using the
    /// difference in VREF and DAC output for the calibration.
    ///
    /// This routine should be called only once after boot, repeated reading of the vref signal and changing of the stored
    /// VREF measurement can introduce significant noise at the current output, degrading the stabilily performance of the
    /// thermostat.
    pub fn calibrate_dac_value(&mut self, channel: usize) {
        let samples = 50;
        let mut target_voltage = ElectricPotential::new::<volt>(0.0);
        for _ in 0..samples {
            target_voltage += self.get_center(channel);
        }
        target_voltage /= samples as f64;
        let mut start_value = 1;
        let mut best_error = ElectricPotential::new::<volt>(100.0);

        for step in (5..18).rev() {
            for value in (start_value..=ad5680::MAX_VALUE).step_by(1 << step) {
                match channel {
                    0 => {
                        self.channel0.dac.set(value).unwrap();
                    }
                    1 => {
                        self.channel1.dac.set(value).unwrap();
                    }
                    _ => unreachable!(),
                }
                sleep(10);

                let dac_feedback = self.adc_read(channel, PinsAdcReadTarget::DacVfb, 64);
                let error = target_voltage - dac_feedback;
                if error < ElectricPotential::new::<volt>(0.0) {
                    break;
                } else if error < best_error {
                    best_error = error;
                    start_value = value;

                    let vref = (value as f64 / ad5680::MAX_VALUE as f64) * DAC_OUT_V_MAX;
                    match channel {
                        0 => self.channel0.vref_meas = vref,
                        1 => self.channel1.vref_meas = vref,
                        _ => unreachable!(),
                    }
                }
            }
        }

        // Reset
        self.set_dac(channel, ElectricPotential::new::<volt>(0.0));
    }

    // power up TEC
    pub fn power_up<I: Into<usize>>(&mut self, channel: I) {
        match channel.into() {
            0 => self.channel0.power_up(),
            1 => self.channel1.power_up(),
            _ => unreachable!(),
        }
    }

    // power down TEC
    pub fn power_down<I: Into<usize>>(&mut self, channel: I) {
        match channel.into() {
            0 => self.channel0.power_down(),
            1 => self.channel1.power_down(),
            _ => unreachable!(),
        }
    }

    pub fn get_max_v(&mut self, channel: usize) -> ElectricPotential {
        self.channel_state(channel).output_limits.max_v
    }

    pub fn get_max_i_pos(&mut self, channel: usize) -> ElectricCurrent {
        self.channel_state(channel).output_limits.max_i_pos
    }

    pub fn get_max_i_neg(&mut self, channel: usize) -> ElectricCurrent {
        self.channel_state(channel).output_limits.max_i_neg
    }

    pub fn get_postfilter(&mut self, index: u8) -> Option<PostFilter> {
        self.adc.get_postfilter(index).unwrap()
    }

    // Get current passing through TEC
    pub fn get_tec_i(&mut self, channel: usize) -> ElectricCurrent {
        let tec_i = (self.adc_read(channel, PinsAdcReadTarget::ITec, 16)
            - self.adc_read(channel, PinsAdcReadTarget::VRef, 16))
            / ElectricalResistance::new::<ohm>(0.4);
        match self.channel_state(channel).polarity {
            Polarity::Normal => tec_i,
            Polarity::Reversed => -tec_i,
        }
    }

    // Get voltage across TEC
    pub fn get_tec_v(&mut self, channel: usize) -> ElectricPotential {
        (self.adc_read(channel, PinsAdcReadTarget::VTec, 16) - ElectricPotential::new::<volt>(1.5))
            * 4.0
    }

    fn set_pwm(&mut self, channel: usize, pin: PwmPin, duty: f64) -> f64 {
        fn set<P: hal::PwmPin<Duty = u16>>(pin: &mut P, duty: f64) -> f64 {
            let max = pin.get_max_duty();
            let value = ((duty * (max as f64)) as u16).min(max);
            pin.set_duty(value);
            value as f64 / (max as f64)
        }
        match (channel, pin) {
            (_, PwmPin::ISet) => panic!("i_set is no pwm pin"),
            (0, PwmPin::MaxIPos) => set(&mut self.pwm.max_i_pos0, duty),
            (0, PwmPin::MaxINeg) => set(&mut self.pwm.max_i_neg0, duty),
            (0, PwmPin::MaxV) => set(&mut self.pwm.max_v0, duty),
            (1, PwmPin::MaxIPos) => set(&mut self.pwm.max_i_pos1, duty),
            (1, PwmPin::MaxINeg) => set(&mut self.pwm.max_i_neg1, duty),
            (1, PwmPin::MaxV) => set(&mut self.pwm.max_v1, duty),
            _ => unreachable!(),
        }
    }

    pub fn set_max_v(
        &mut self,
        channel: usize,
        max_v: ElectricPotential,
    ) -> (ElectricPotential, ElectricPotential) {
        let max_v = max_v.min(MAX_TEC_V).max(ElectricPotential::zero());
        self.channel_state(channel).output_limits.max_v = max_v;
        let v_maxv = max_v / 4.0;
        let duty = (v_maxv / CPU_ADC_VREF).get::<ratio>();

        let duty = self.set_pwm(channel, PwmPin::MaxV, duty);
        let v_maxv = duty * CPU_ADC_VREF;
        let max_v = 4.0 * v_maxv;

        (max_v, MAX_TEC_V)
    }

    pub fn set_max_i_pos(
        &mut self,
        channel: usize,
        max_i_pos: ElectricCurrent,
    ) -> (ElectricCurrent, ElectricCurrent) {
        let pin = match self.channel_state(channel).polarity {
            Polarity::Normal => PwmPin::MaxIPos,
            Polarity::Reversed => PwmPin::MaxINeg,
        };

        let max_i_pos = max_i_pos.min(MAX_TEC_I).max(ElectricCurrent::zero());
        self.channel_state(channel).output_limits.max_i_pos = max_i_pos;
        let v_maxip = 10.0 * (max_i_pos * R_SENSE);
        let duty = (v_maxip / CPU_ADC_VREF).get::<ratio>();

        let duty = self.set_pwm(channel, pin, duty);
        let v_maxip = duty * CPU_ADC_VREF;
        let max_i_pos = v_maxip / 10.0 / R_SENSE;

        (max_i_pos, MAX_TEC_I)
    }

    pub fn set_max_i_neg(
        &mut self,
        channel: usize,
        max_i_neg: ElectricCurrent,
    ) -> (ElectricCurrent, ElectricCurrent) {
        let pin = match self.channel_state(channel).polarity {
            Polarity::Normal => PwmPin::MaxINeg,
            Polarity::Reversed => PwmPin::MaxIPos,
        };

        let max_i_neg = max_i_neg.min(MAX_TEC_I).max(ElectricCurrent::zero());
        self.channel_state(channel).output_limits.max_i_neg = max_i_neg;
        let v_maxin = 10.0 * (max_i_neg * R_SENSE);
        let duty = (v_maxin / CPU_ADC_VREF).get::<ratio>();

        let duty = self.set_pwm(channel, pin, duty);
        let v_maxin = duty * CPU_ADC_VREF;
        let max_i_neg = v_maxin / 10.0 / R_SENSE;

        (max_i_neg, MAX_TEC_I)
    }

    pub fn set_postfilter(&mut self, index: u8, filter: Option<PostFilter>) {
        self.adc.set_postfilter(index, filter).unwrap()
    }

    pub fn set_polarity(&mut self, channel: usize, polarity: Polarity) {
        if self.channel_state(channel).polarity != polarity {
            let i_set = self.channel_state(channel).i_set;
            let max_i_pos = self.get_max_i_pos(channel);
            let max_i_neg = self.get_max_i_neg(channel);
            self.channel_state(channel).polarity = polarity;

            self.set_i(channel, i_set);
            self.set_max_i_pos(channel, max_i_pos);
            self.set_max_i_neg(channel, max_i_neg);
        }
    }

    fn report(&mut self, channel: usize) -> Report {
        let i_set = self.get_i_set(channel);
        let i_tec = self.adc_read(channel, PinsAdcReadTarget::ITec, 16);
        let tec_i = self.get_tec_i(channel);
        let dac_value = self.get_dac(channel);
        let state = self.channel_state(channel);
        let pid_output = ElectricCurrent::new::<ampere>(state.pid.y1);
        Report {
            channel,
            time: state.get_adc_time(),
            interval: state.get_adc_interval(),
            adc: state.get_adc(),
            sens: state.get_sens(),
            temperature: state
                .get_temperature()
                .map(|temperature| temperature.get::<degree_celsius>()),
            pid_engaged: state.pid_engaged,
            i_set,
            dac_value,
            dac_feedback: self.adc_read(channel, PinsAdcReadTarget::DacVfb, 1),
            i_tec,
            tec_i,
            tec_u_meas: self.get_tec_v(channel),
            pid_output,
        }
    }

    pub fn reports_json(&mut self) -> Result<JsonBuffer, serde_json_core::ser::Error> {
        let mut reports = Vec::<_, U2>::new();
        for channel in 0..CHANNELS {
            let _ = reports.push(self.report(channel));
        }
        serde_json_core::to_vec(&reports)
    }

    pub fn pid_summaries_json(&mut self) -> Result<JsonBuffer, serde_json_core::ser::Error> {
        let mut summaries = Vec::<_, U2>::new();
        for channel in 0..CHANNELS {
            let _ = summaries.push(self.channel_state(channel).pid.summary(channel));
        }
        serde_json_core::to_vec(&summaries)
    }

    pub fn pid_engaged(&mut self) -> bool {
        for channel in 0..CHANNELS {
            if self.channel_state(channel).pid_engaged {
                return true;
            }
        }
        false
    }

    fn output_summary(&mut self, channel: usize) -> OutputSummary {
        OutputSummary {
            channel,
            center: CenterPointJson(self.channel_state(channel).center.clone()),
            i_set: self.get_i_set(channel),
            max_v: self.get_max_v(channel),
            max_i_pos: self.get_max_i_pos(channel),
            max_i_neg: self.get_max_i_neg(channel),
            polarity: PolarityJson(self.channel_state(channel).polarity.clone()),
        }
    }

    pub fn output_summaries_json(&mut self) -> Result<JsonBuffer, serde_json_core::ser::Error> {
        let mut summaries = Vec::<_, U2>::new();
        for channel in 0..CHANNELS {
            let _ = summaries.push(self.output_summary(channel));
        }
        serde_json_core::to_vec(&summaries)
    }

    fn postfilter_summary(&mut self, channel: usize) -> PostFilterSummary {
        let rate = self
            .get_postfilter(channel as u8)
            .and_then(|filter| filter.output_rate());
        PostFilterSummary { channel, rate }
    }

    pub fn postfilter_summaries_json(&mut self) -> Result<JsonBuffer, serde_json_core::ser::Error> {
        let mut summaries = Vec::<_, U2>::new();
        for channel in 0..CHANNELS {
            let _ = summaries.push(self.postfilter_summary(channel));
        }
        serde_json_core::to_vec(&summaries)
    }

    fn b_parameter_summary(&mut self, channel: usize) -> BParameterSummary {
        let params = self.channel_state(channel).bp.clone();
        BParameterSummary { channel, params }
    }

    pub fn b_parameter_summaries_json(
        &mut self,
    ) -> Result<JsonBuffer, serde_json_core::ser::Error> {
        let mut summaries = Vec::<_, U2>::new();
        for channel in 0..CHANNELS {
            let _ = summaries.push(self.b_parameter_summary(channel));
        }
        serde_json_core::to_vec(&summaries)
    }

    pub fn current_abs_max_tec_i(&mut self) -> ElectricCurrent {
        (0..CHANNELS)
            .map(|channel| self.get_tec_i(channel).abs())
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal))
            .unwrap()
    }
}

#[derive(Serialize)]
pub struct Report {
    channel: usize,
    time: Time,
    interval: Time,
    adc: Option<ElectricPotential>,
    sens: Option<ElectricalResistance>,
    temperature: Option<f64>,
    pid_engaged: bool,
    i_set: ElectricCurrent,
    dac_value: ElectricPotential,
    dac_feedback: ElectricPotential,
    i_tec: ElectricPotential,
    tec_i: ElectricCurrent,
    tec_u_meas: ElectricPotential,
    pid_output: ElectricCurrent,
}

pub struct CenterPointJson(CenterPoint);

// used in JSON encoding, not for config
impl Serialize for CenterPointJson {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0 {
            CenterPoint::VRef => serializer.serialize_str("vref"),
            CenterPoint::Override(vref) => serializer.serialize_f32(vref),
        }
    }
}

pub struct PolarityJson(Polarity);

// used in JSON encoding, not for config
impl Serialize for PolarityJson {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self.0 {
            Polarity::Normal => "normal",
            Polarity::Reversed => "reversed",
        })
    }
}

#[derive(Serialize)]
pub struct OutputSummary {
    channel: usize,
    center: CenterPointJson,
    i_set: ElectricCurrent,
    max_v: ElectricPotential,
    max_i_pos: ElectricCurrent,
    max_i_neg: ElectricCurrent,
    polarity: PolarityJson,
}

#[derive(Serialize)]
pub struct PostFilterSummary {
    channel: usize,
    rate: Option<f32>,
}

#[derive(Serialize)]
pub struct BParameterSummary {
    channel: usize,
    params: b_parameter::Parameters,
}
