use num_traits::float::Float;
use serde::{Deserialize, Serialize};
use uom::si::{
    electrical_resistance::ohm,
    f64::{ElectricalResistance, TemperatureInterval, ThermodynamicTemperature},
    ratio::ratio,
    temperature_interval::kelvin as kelvin_interval,
    thermodynamic_temperature::{degree_celsius, kelvin},
};

/// B-Parameter equation parameters
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameters {
    /// Base temperature
    pub t0: ThermodynamicTemperature,
    /// Thermistor resistance at base temperature
    pub r0: ElectricalResistance,
    /// Beta (average slope of the function ln R vs. 1/T)
    pub b: TemperatureInterval,
}

impl Parameters {
    /// Perform the resistance to temperature conversion.
    pub fn get_temperature(&self, r: ElectricalResistance) -> ThermodynamicTemperature {
        let temp = (self.t0.recip() + (r / self.r0).get::<ratio>().ln() / self.b).recip();
        ThermodynamicTemperature::new::<kelvin>(temp.get::<kelvin_interval>())
    }
}

impl Default for Parameters {
    fn default() -> Self {
        Parameters {
            t0: ThermodynamicTemperature::new::<degree_celsius>(25.0),
            r0: ElectricalResistance::new::<ohm>(10_000.0),
            b: TemperatureInterval::new::<kelvin_interval>(3800.0),
        }
    }
}
