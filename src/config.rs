use crate::{
    ad7172::PostFilter,
    b_parameter,
    channels::Channels,
    command_parser::{CenterPoint, Polarity},
    pid,
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
    bp: b_parameter::Parameters,
    output_limits: OutputLimits,
    /// uses variant `PostFilter::Invalid` instead of `None` to save space
    adc_postfilter: PostFilter,
}

impl ChannelConfig {
    pub fn new(channels: &mut Channels, channel: usize) -> Self {
        let output_limits = OutputLimits::new(channels, channel);

        let adc_postfilter = channels
            .get_postfilter(channel as u8)
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
            bp: state.bp.clone(),
            output_limits,
            adc_postfilter,
        }
    }

    pub fn apply(&self, channels: &mut Channels, channel: usize) {
        let state = channels.channel_state(channel);
        state.center = self.center.clone();
        state.pid.parameters = self.pid.clone();
        state.pid.target = self.pid_target.into();
        state.pid_engaged = self.pid_engaged;
        state.bp = self.bp.clone();

        self.output_limits.apply(channels, channel);

        let adc_postfilter = match self.adc_postfilter {
            PostFilter::Invalid => None,
            adc_postfilter => Some(adc_postfilter),
        };
        let _ = channels.set_postfilter(channel as u8, adc_postfilter);
        let _ = channels.set_i(channel, self.i_set);
        channels.set_polarity(channel, self.polarity.clone());
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputLimits {
    pub max_v: ElectricPotential,
    pub max_i_pos: ElectricCurrent,
    pub max_i_neg: ElectricCurrent,
}

impl OutputLimits {
    pub fn new(channels: &mut Channels, channel: usize) -> Self {
        let max_v = channels.get_max_v(channel);
        let max_i_pos = channels.get_max_i_pos(channel);
        let max_i_neg = channels.get_max_i_neg(channel);
        OutputLimits {
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
