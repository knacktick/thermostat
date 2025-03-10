use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameters {
    /// Gain coefficient for proportional term
    pub kp: f32,
    /// Gain coefficient for integral term
    pub ki: f32,
    /// Gain coefficient for derivative term
    pub kd: f32,
    /// Output limit minimum
    pub output_min: f32,
    /// Output limit maximum
    pub output_max: f32,
}

impl Default for Parameters {
    fn default() -> Self {
        Parameters {
            kp: 0.0,
            ki: 0.0,
            kd: 0.0,
            output_min: -2.0,
            output_max: 2.0,
        }
    }
}

#[derive(Clone)]
pub struct Controller {
    pub parameters: Parameters,
    pub target: f64,
    u1: f64,
    x1: f64,
    x2: f64,
    pub y1: f64,
}

impl Controller {
    pub const fn new(parameters: Parameters) -> Controller {
        Controller {
            parameters,
            target: 0.0,
            u1: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
        }
    }

    // Based on https://hackmd.io/IACbwcOTSt6Adj3_F9bKuw PID implementation
    // Input x(t), target u(t), output y(t)
    // y0' =   y1 - ki * u0
    //       + x0 * (kp + ki + kd)
    //       - x1 * (kp + 2kd)
    //       + x2 * kd
    // y0  = clip(y0', ymin, ymax)
    pub fn update(&mut self, input: f64) -> f64 {
        let kp = self.parameters.kp as f64;
        let ki = self.parameters.ki as f64;
        let kd = self.parameters.kd as f64;

        #[rustfmt::skip]
        let mut output = self.y1 - ki * self.target
            + input * (kp + ki + kd)
            - self.x1 * (kp + 2.0 * kd)
            + self.x2 * kd;
        output = output.clamp(self.parameters.output_min.into(), self.parameters.output_max.into());

        self.x2 = self.x1;
        self.x1 = input;
        self.u1 = self.target;
        self.y1 = output;
        output
    }

    pub fn summary(&self, channel: usize) -> Summary {
        Summary {
            channel,
            parameters: self.parameters.clone(),
            target: self.target,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Summary {
    channel: usize,
    parameters: Parameters,
    target: f64,
}

#[cfg(test)]
mod test {
    use super::*;

    const PARAMETERS: Parameters = Parameters {
        kp: 0.03,
        ki: 0.002,
        kd: 0.15,
        output_min: -10.0,
        output_max: 10.0,
    };

    #[test]
    fn test_controller() {
        // Initial and ambient temperature
        const DEFAULT: f64 = 20.0;
        // Target temperature
        const TARGET: f64 = 40.0;
        // Control tolerance
        const ERROR: f64 = 0.01;
        // System response delay
        const DELAY: usize = 10;
        // Heat lost
        const LOSS: f64 = 0.05;
        // Limit simulation cycle, reaching this limit before settling fails test
        const CYCLE_LIMIT: u32 = 1000;

        let mut pid = Controller::new(PARAMETERS.clone());
        pid.target = TARGET;

        let mut values = [DEFAULT; DELAY];
        let mut t = 0;
        let mut total_t = 0;
        let mut output: f64 = 0.0;
        let target = (TARGET - ERROR)..=(TARGET + ERROR);
        while !values.iter().all(|value| target.contains(value)) && total_t < CYCLE_LIMIT {
            let next_t = (t + 1) % DELAY;
            // Feed the oldest temperature
            output = pid.update(values[next_t]);
            // Overwrite oldest with previous temperature - output
            values[next_t] = values[t] - output - (values[t] - DEFAULT) * LOSS;
            t = next_t;
            total_t += 1;
            println!("{}", values[t].to_string());
        }
        assert_ne!(CYCLE_LIMIT, total_t);
    }
}
