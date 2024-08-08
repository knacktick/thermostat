use crate::{
    ad7172::PostFilter,
    channels::Channels,
    command_parser::{CenterPoint, Polarity},
    pid, steinhart_hart,
};
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use uom::si::f64::{ElectricCurrent, ElectricPotential};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChannelConfig {
    center: CenterPoint,
    pid: pid::Parameters,
    pid_target: f32,
    pid_engaged: bool,
    i_set: ElectricCurrent,
    polarity: Polarity,
    sh: steinhart_hart::Parameters,
    pwm: PwmLimits,
    /// uses variant `PostFilter::Invalid` instead of `None` to save space
    adc_postfilter: PostFilter,
}

impl ChannelConfig {
    pub fn new(channels: &mut Channels, channel: usize) -> Self {
        let pwm = PwmLimits::new(channels, channel);

        let adc_postfilter = channels
            .adc
            .get_postfilter(channel as u8)
            .unwrap()
            .unwrap_or(PostFilter::Invalid);

        let state = channels.channel_state(channel);
        let i_set = if state.pid_engaged {
            ElectricCurrent::zero()
        } else {
            state.i_set
        };
        ChannelConfig {
            center: state.center.clone(),
            pid: state.pid.parameters.clone(),
            pid_target: state.pid.target as f32,
            pid_engaged: state.pid_engaged,
            i_set,
            polarity: state.polarity.clone(),
            sh: state.sh.clone(),
            pwm,
            adc_postfilter,
        }
    }

    pub fn apply(&self, channels: &mut Channels, channel: usize) {
        let state = channels.channel_state(channel);
        state.center = self.center.clone();
        state.pid.parameters = self.pid.clone();
        state.pid.target = self.pid_target.into();
        state.pid_engaged = self.pid_engaged;
        state.sh = self.sh.clone();

        self.pwm.apply(channels, channel);

        let adc_postfilter = match self.adc_postfilter {
            PostFilter::Invalid => None,
            adc_postfilter => Some(adc_postfilter),
        };
        let _ = channels.adc.set_postfilter(channel as u8, adc_postfilter);
        let _ = channels.set_i(channel, self.i_set);
        channels.set_polarity(channel, self.polarity.clone());
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PwmLimits {
    pub max_v: ElectricPotential,
    pub max_i_pos: ElectricCurrent,
    pub max_i_neg: ElectricCurrent,
}

impl PwmLimits {
    pub fn new(channels: &mut Channels, channel: usize) -> Self {
        let max_v = channels.get_max_v(channel);
        let max_i_pos = channels.get_max_i_pos(channel);
        let max_i_neg = channels.get_max_i_neg(channel);
        PwmLimits {
            max_v,
            max_i_pos,
            max_i_neg,
        }
    }

    pub fn apply(&self, channels: &mut Channels, channel: usize) {
        channels.set_max_v(channel, self.max_v);
        channels.set_max_i_pos(channel, self.max_i_pos);
        channels.set_max_i_neg(channel, self.max_i_neg);
    }
}
