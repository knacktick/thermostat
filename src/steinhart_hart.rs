use num_traits::float::Float;
use serde::{Deserialize, Serialize};
use uom::si::{
    electrical_resistance::ohm,
    f64::{ElectricalResistance, ThermodynamicTemperature},
    ratio::ratio,
    thermodynamic_temperature::{degree_celsius, kelvin},
};

/// Steinhart-Hart equation parameters
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameters {
    /// Base temperature
    pub t0: ThermodynamicTemperature,
    /// Base resistance
    pub r0: ElectricalResistance,
    /// Beta
    pub b: f64,
}

impl Parameters {
    /// Perform the voltage to temperature conversion.
    pub fn get_temperature(&self, r: ElectricalResistance) -> ThermodynamicTemperature {
        let inv_temp = 1.0 / self.t0.get::<kelvin>() + (r / self.r0).get::<ratio>().ln() / self.b;
        ThermodynamicTemperature::new::<kelvin>(1.0 / inv_temp)
    }
}

impl Default for Parameters {
    fn default() -> Self {
        Parameters {
            t0: ThermodynamicTemperature::new::<degree_celsius>(25.0),
            r0: ElectricalResistance::new::<ohm>(10_000.0),
            b: 3800.0,
        }
    }
}
