use core::fmt;
use core::num::ParseIntError;
use core::str::{from_utf8, Utf8Error};
use nom::{
    branch::alt,
    bytes::complete::{is_a, tag, take_while1},
    character::{
        complete::{char, one_of},
        is_digit,
    },
    combinator::{complete, map, opt, value},
    error::ErrorKind,
    multi::{fold_many0, fold_many1},
    sequence::preceded,
    IResult, Needed,
};
use num_traits::{Num, ParseFloatError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    Parser(ErrorKind),
    Incomplete,
    UnexpectedInput(u8),
    Utf8(Utf8Error),
    ParseInt(ParseIntError),
    // `num_traits::ParseFloatError` does not impl Clone
    ParseFloat,
}

impl<'t> From<nom::Err<(&'t [u8], ErrorKind)>> for Error {
    fn from(e: nom::Err<(&'t [u8], ErrorKind)>) -> Self {
        match e {
            nom::Err::Incomplete(_) => Error::Incomplete,
            nom::Err::Error((_, e)) => Error::Parser(e),
            nom::Err::Failure((_, e)) => Error::Parser(e),
        }
    }
}

impl From<Utf8Error> for Error {
    fn from(e: Utf8Error) -> Self {
        Error::Utf8(e)
    }
}

impl From<ParseIntError> for Error {
    fn from(e: ParseIntError) -> Self {
        Error::ParseInt(e)
    }
}

impl From<ParseFloatError> for Error {
    fn from(_: ParseFloatError) -> Self {
        Error::ParseFloat
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Error::Incomplete => "incomplete input".fmt(fmt),
            Error::UnexpectedInput(c) => {
                "unexpected input: ".fmt(fmt)?;
                c.fmt(fmt)
            }
            Error::Parser(e) => {
                "parser: ".fmt(fmt)?;
                (e as &dyn core::fmt::Debug).fmt(fmt)
            }
            Error::Utf8(e) => {
                "utf8: ".fmt(fmt)?;
                (e as &dyn core::fmt::Debug).fmt(fmt)
            }
            Error::ParseInt(e) => {
                "parsing int: ".fmt(fmt)?;
                (e as &dyn core::fmt::Debug).fmt(fmt)
            }
            Error::ParseFloat => "parsing float".fmt(fmt),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ipv4Config {
    pub address: [u8; 4],
    pub mask_len: u8,
    pub gateway: Option<[u8; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShowCommand {
    Input,
    Output,
    Pid,
    SteinhartHart,
    PostFilter,
    Ipv4,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PidParameter {
    Target,
    KP,
    KI,
    KD,
    OutputMin,
    OutputMax,
}

/// Steinhart-Hart equation parameter
#[derive(Debug, Clone, PartialEq)]
pub enum ShParameter {
    T0,
    B,
    R0,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PwmPin {
    ISet,
    MaxIPos,
    MaxINeg,
    MaxV,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CenterPoint {
    VRef,
    Override(f32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Polarity {
    Normal,
    Reversed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Quit,
    Load {
        channel: Option<usize>,
    },
    Save {
        channel: Option<usize>,
    },
    Reset,
    Ipv4(Ipv4Config),
    Show(ShowCommand),
    /// PWM parameter setting
    Output {
        channel: usize,
        pin: PwmPin,
        value: f64,
    },
    /// Enable PID control for `i_set`
    OutputPid {
        channel: usize,
    },
    OutputPolarity {
        channel: usize,
        polarity: Polarity,
    },
    CenterPoint {
        channel: usize,
        center: CenterPoint,
    },
    /// PID parameter setting
    Pid {
        channel: usize,
        parameter: PidParameter,
        value: f64,
    },
    SteinhartHart {
        channel: usize,
        parameter: ShParameter,
        value: f64,
    },
    PostFilter {
        channel: usize,
        rate: Option<f32>,
    },
    Dfu,
    FanSet {
        fan_pwm: u32,
    },
    FanAuto,
    ShowFan,
    FanCurve {
        k_a: f32,
        k_b: f32,
        k_c: f32,
    },
    FanCurveDefaults,
    ShowHWRev,
}

fn end(input: &[u8]) -> IResult<&[u8], ()> {
    complete(fold_many0(one_of("\r\n\t "), (), |(), _| ()))(input)
}

fn whitespace(input: &[u8]) -> IResult<&[u8], ()> {
    fold_many1(char(' '), (), |(), _| ())(input)
}

fn unsigned(input: &[u8]) -> IResult<&[u8], Result<u32, Error>> {
    take_while1(is_digit)(input).map(|(input, digits)| {
        let result = from_utf8(digits)
            .map_err(|e| e.into())
            .and_then(|digits| digits.parse::<u32>().map_err(|e| e.into()));
        (input, result)
    })
}

fn float(input: &[u8]) -> IResult<&[u8], Result<f64, Error>> {
    let (input, sign) = opt(is_a("-"))(input)?;
    let negative = sign.is_some();
    let (input, digits) = take_while1(|c| is_digit(c) || c == b'.')(input)?;
    let result = from_utf8(digits)
        .map_err(|e| e.into())
        .and_then(|digits| f64::from_str_radix(digits, 10).map_err(|e| e.into()))
        .map(|result: f64| if negative { -result } else { result });
    Ok((input, result))
}

fn channel(input: &[u8]) -> IResult<&[u8], usize> {
    map(one_of("01"), |c| (c as usize) - ('0' as usize))(input)
}

fn report(input: &[u8]) -> IResult<&[u8], Command> {
    preceded(
        tag("report"),
        // `report` - Report once
        value(Command::Show(ShowCommand::Input), end),
    )(input)
}

fn pwm_setup(input: &[u8]) -> IResult<&[u8], Result<(PwmPin, f64), Error>> {
    let result_with_pin =
        |pin: PwmPin| move |result: Result<f64, Error>| result.map(|value| (pin, value));

    alt((
        map(
            preceded(tag("i_set"), preceded(whitespace, float)),
            result_with_pin(PwmPin::ISet),
        ),
        map(
            preceded(tag("max_i_pos"), preceded(whitespace, float)),
            result_with_pin(PwmPin::MaxIPos),
        ),
        map(
            preceded(tag("max_i_neg"), preceded(whitespace, float)),
            result_with_pin(PwmPin::MaxINeg),
        ),
        map(
            preceded(tag("max_v"), preceded(whitespace, float)),
            result_with_pin(PwmPin::MaxV),
        ),
    ))(input)
}

/// `output <0-1> pid` - Set output to be controlled by PID
fn output_pid(input: &[u8]) -> IResult<&[u8], ()> {
    value((), tag("pid"))(input)
}

fn output_polarity(input: &[u8]) -> IResult<&[u8], Polarity> {
    preceded(
        tag("polarity"),
        preceded(
            whitespace,
            alt((
                value(Polarity::Normal, tag("normal")),
                value(Polarity::Reversed, tag("reversed")),
            )),
        ),
    )(input)
}

fn output(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("output")(input)?;
    alt((
        |input| {
            let (input, _) = whitespace(input)?;
            let (input, channel) = channel(input)?;
            let (input, _) = whitespace(input)?;
            let (input, result) = alt((
                |input| {
                    let (input, ()) = output_pid(input)?;
                    Ok((input, Ok(Command::OutputPid { channel })))
                },
                |input| {
                    let (input, polarity) = output_polarity(input)?;
                    Ok((input, Ok(Command::OutputPolarity { channel, polarity })))
                },
                |input| {
                    let (input, config) = pwm_setup(input)?;
                    match config {
                        Ok((pin, value)) => Ok((
                            input,
                            Ok(Command::Output {
                                channel,
                                pin,
                                value,
                            }),
                        )),
                        Err(e) => Ok((input, Err(e))),
                    }
                },
            ))(input)?;
            end(input)?;
            Ok((input, result))
        },
        value(Ok(Command::Show(ShowCommand::Output)), end),
    ))(input)
}

fn center_point(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("center")(input)?;
    let (input, _) = whitespace(input)?;
    let (input, channel) = channel(input)?;
    let (input, _) = whitespace(input)?;
    let (input, center) = alt((value(Ok(CenterPoint::VRef), tag("vref")), |input| {
        let (input, value) = float(input)?;
        Ok((
            input,
            value.map(|value| CenterPoint::Override(value as f32)),
        ))
    }))(input)?;
    end(input)?;
    Ok((
        input,
        center.map(|center| Command::CenterPoint { channel, center }),
    ))
}

/// `pid <0-1> <parameter> <value>`
fn pid_parameter(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, channel) = channel(input)?;
    let (input, _) = whitespace(input)?;
    let (input, parameter) = alt((
        value(PidParameter::Target, tag("target")),
        value(PidParameter::KP, tag("kp")),
        value(PidParameter::KI, tag("ki")),
        value(PidParameter::KD, tag("kd")),
        value(PidParameter::OutputMin, tag("output_min")),
        value(PidParameter::OutputMax, tag("output_max")),
    ))(input)?;
    let (input, _) = whitespace(input)?;
    let (input, value) = float(input)?;
    let result = value.map(|value| Command::Pid {
        channel,
        parameter,
        value,
    });
    Ok((input, result))
}

/// `pid` | `pid <pid_parameter>`
fn pid(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("pid")(input)?;
    alt((
        preceded(whitespace, pid_parameter),
        value(Ok(Command::Show(ShowCommand::Pid)), end),
    ))(input)
}

/// `s-h <0-1> <parameter> <value>`
fn steinhart_hart_parameter(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, channel) = channel(input)?;
    let (input, _) = whitespace(input)?;
    let (input, parameter) = alt((
        value(ShParameter::T0, tag("t0")),
        value(ShParameter::B, tag("b")),
        value(ShParameter::R0, tag("r0")),
    ))(input)?;
    let (input, _) = whitespace(input)?;
    let (input, value) = float(input)?;
    let result = value.map(|value| Command::SteinhartHart {
        channel,
        parameter,
        value,
    });
    Ok((input, result))
}

/// `s-h` | `s-h <steinhart_hart_parameter>`
fn steinhart_hart(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("s-h")(input)?;
    alt((
        preceded(whitespace, steinhart_hart_parameter),
        value(Ok(Command::Show(ShowCommand::SteinhartHart)), end),
    ))(input)
}

fn postfilter(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("postfilter")(input)?;
    alt((
        preceded(whitespace, |input| {
            let (input, channel) = channel(input)?;
            let (input, _) = whitespace(input)?;
            alt((
                value(
                    Ok(Command::PostFilter {
                        channel,
                        rate: None,
                    }),
                    tag("off"),
                ),
                move |input| {
                    let (input, _) = tag("rate")(input)?;
                    let (input, _) = whitespace(input)?;
                    let (input, rate) = float(input)?;
                    let result = rate.map(|rate| Command::PostFilter {
                        channel,
                        rate: Some(rate as f32),
                    });
                    Ok((input, result))
                },
            ))(input)
        }),
        value(Ok(Command::Show(ShowCommand::PostFilter)), end),
    ))(input)
}

fn load(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("load")(input)?;
    let (input, channel) = alt((
        |input| {
            let (input, _) = whitespace(input)?;
            let (input, channel) = channel(input)?;
            let (input, _) = end(input)?;
            Ok((input, Some(channel)))
        },
        value(None, end),
    ))(input)?;

    let result = Ok(Command::Load { channel });
    Ok((input, result))
}

fn save(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("save")(input)?;
    let (input, channel) = alt((
        |input| {
            let (input, _) = whitespace(input)?;
            let (input, channel) = channel(input)?;
            let (input, _) = end(input)?;
            Ok((input, Some(channel)))
        },
        value(None, end),
    ))(input)?;

    let result = Ok(Command::Save { channel });
    Ok((input, result))
}

fn ipv4_addr(input: &[u8]) -> IResult<&[u8], Result<[u8; 4], Error>> {
    let (input, a) = unsigned(input)?;
    let (input, _) = tag(".")(input)?;
    let (input, b) = unsigned(input)?;
    let (input, _) = tag(".")(input)?;
    let (input, c) = unsigned(input)?;
    let (input, _) = tag(".")(input)?;
    let (input, d) = unsigned(input)?;
    let address = move || Ok([a? as u8, b? as u8, c? as u8, d? as u8]);
    Ok((input, address()))
}

fn ipv4(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("ipv4")(input)?;
    alt((
        |input| {
            let (input, _) = whitespace(input)?;
            let (input, address) = ipv4_addr(input)?;
            let (input, _) = tag("/")(input)?;
            let (input, mask_len) = unsigned(input)?;
            let (input, gateway) = alt((
                |input| {
                    let (input, _) = whitespace(input)?;
                    let (input, gateway) = ipv4_addr(input)?;
                    Ok((input, gateway.map(Some)))
                },
                value(Ok(None), end),
            ))(input)?;

            let result = move || {
                Ok(Command::Ipv4(Ipv4Config {
                    address: address?,
                    mask_len: mask_len? as u8,
                    gateway: gateway?,
                }))
            };
            Ok((input, result()))
        },
        value(Ok(Command::Show(ShowCommand::Ipv4)), end),
    ))(input)
}

fn fan(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("fan")(input)?;
    alt((
        |input| {
            let (input, _) = whitespace(input)?;

            let (input, result) = alt((
                |input| {
                    let (input, _) = tag("auto")(input)?;
                    Ok((input, Ok(Command::FanAuto)))
                },
                |input| {
                    let (input, value) = unsigned(input)?;
                    Ok((
                        input,
                        Ok(Command::FanSet {
                            fan_pwm: value.unwrap_or(0),
                        }),
                    ))
                },
            ))(input)?;
            Ok((input, result))
        },
        value(Ok(Command::ShowFan), end),
    ))(input)
}

fn fan_curve(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("fcurve")(input)?;
    alt((
        |input| {
            let (input, _) = whitespace(input)?;
            let (input, result) = alt((
                |input| {
                    let (input, _) = tag("default")(input)?;
                    Ok((input, Ok(Command::FanCurveDefaults)))
                },
                |input| {
                    let (input, k_a) = float(input)?;
                    let (input, _) = whitespace(input)?;
                    let (input, k_b) = float(input)?;
                    let (input, _) = whitespace(input)?;
                    let (input, k_c) = float(input)?;
                    if let (Ok(k_a), Ok(k_b), Ok(k_c)) = (k_a, k_b, k_c) {
                        Ok((
                            input,
                            Ok(Command::FanCurve {
                                k_a: k_a as f32,
                                k_b: k_b as f32,
                                k_c: k_c as f32,
                            }),
                        ))
                    } else {
                        Err(nom::Err::Incomplete(Needed::Size(3)))
                    }
                },
            ))(input)?;
            Ok((input, result))
        },
        value(Err(Error::Incomplete), end),
    ))(input)
}

fn command(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    alt((
        value(Ok(Command::Quit), tag("quit")),
        load,
        save,
        value(Ok(Command::Reset), tag("reset")),
        ipv4,
        map(report, Ok),
        output,
        center_point,
        pid,
        steinhart_hart,
        postfilter,
        value(Ok(Command::Dfu), tag("dfu")),
        fan,
        fan_curve,
        value(Ok(Command::ShowHWRev), tag("hwrev")),
    ))(input)
}

impl Command {
    pub fn parse(input: &[u8]) -> Result<Self, Error> {
        match command(input) {
            Ok((input_remain, result)) if input_remain.is_empty() => result,
            Ok((input_remain, _)) => Err(Error::UnexpectedInput(input_remain[0])),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_quit() {
        let command = Command::parse(b"quit");
        assert_eq!(command, Ok(Command::Quit));
    }

    #[test]
    fn parse_load() {
        let command = Command::parse(b"load");
        assert_eq!(command, Ok(Command::Load { channel: None }));
    }

    #[test]
    fn parse_load_channel() {
        let command = Command::parse(b"load 0");
        assert_eq!(command, Ok(Command::Load { channel: Some(0) }));
    }

    #[test]
    fn parse_save() {
        let command = Command::parse(b"save");
        assert_eq!(command, Ok(Command::Save { channel: None }));
    }

    #[test]
    fn parse_save_channel() {
        let command = Command::parse(b"save 0");
        assert_eq!(command, Ok(Command::Save { channel: Some(0) }));
    }

    #[test]
    fn parse_show_ipv4() {
        let command = Command::parse(b"ipv4");
        assert_eq!(command, Ok(Command::Show(ShowCommand::Ipv4)));
    }

    #[test]
    fn parse_ipv4() {
        let command = Command::parse(b"ipv4 192.168.1.26/24");
        assert_eq!(
            command,
            Ok(Command::Ipv4(Ipv4Config {
                address: [192, 168, 1, 26],
                mask_len: 24,
                gateway: None,
            }))
        );
    }

    #[test]
    fn parse_ipv4_and_gateway() {
        let command = Command::parse(b"ipv4 10.42.0.126/8 10.1.0.1");
        assert_eq!(
            command,
            Ok(Command::Ipv4(Ipv4Config {
                address: [10, 42, 0, 126],
                mask_len: 8,
                gateway: Some([10, 1, 0, 1]),
            }))
        );
    }

    #[test]
    fn parse_report() {
        let command = Command::parse(b"report");
        assert_eq!(command, Ok(Command::Show(ShowCommand::Input)));
    }

    #[test]
    fn parse_output_i_set() {
        let command = Command::parse(b"output 1 i_set 16383");
        assert_eq!(
            command,
            Ok(Command::Output {
                channel: 1,
                pin: PwmPin::ISet,
                value: 16383.0,
            })
        );
    }

    #[test]
    fn parse_output_polarity() {
        let command = Command::parse(b"pwm 0 polarity reversed");
        assert_eq!(
            command,
            Ok(Command::OutputPolarity {
                channel: 0,
                polarity: Polarity::Reversed,
            })
        );
    }

    #[test]
    fn parse_output_pid() {
        let command = Command::parse(b"output 0 pid");
        assert_eq!(command, Ok(Command::OutputPid { channel: 0 }));
    }

    #[test]
    fn parse_output_max_i_pos() {
        let command = Command::parse(b"output 0 max_i_pos 7");
        assert_eq!(
            command,
            Ok(Command::Output {
                channel: 0,
                pin: PwmPin::MaxIPos,
                value: 7.0,
            })
        );
    }

    #[test]
    fn parse_output_max_i_neg() {
        let command = Command::parse(b"output 0 max_i_neg 128");
        assert_eq!(
            command,
            Ok(Command::Output {
                channel: 0,
                pin: PwmPin::MaxINeg,
                value: 128.0,
            })
        );
    }

    #[test]
    fn parse_output_max_v() {
        let command = Command::parse(b"output 0 max_v 32768");
        assert_eq!(
            command,
            Ok(Command::Output {
                channel: 0,
                pin: PwmPin::MaxV,
                value: 32768.0,
            })
        );
    }

    #[test]
    fn parse_pid() {
        let command = Command::parse(b"pid");
        assert_eq!(command, Ok(Command::Show(ShowCommand::Pid)));
    }

    #[test]
    fn parse_pid_target() {
        let command = Command::parse(b"pid 0 target 36.5");
        assert_eq!(
            command,
            Ok(Command::Pid {
                channel: 0,
                parameter: PidParameter::Target,
                value: 36.5,
            })
        );
    }

    #[test]
    fn parse_steinhart_hart() {
        let command = Command::parse(b"s-h");
        assert_eq!(command, Ok(Command::Show(ShowCommand::SteinhartHart)));
    }

    #[test]
    fn parse_steinhart_hart_set() {
        let command = Command::parse(b"s-h 1 t0 23.05");
        assert_eq!(
            command,
            Ok(Command::SteinhartHart {
                channel: 1,
                parameter: ShParameter::T0,
                value: 23.05,
            })
        );
    }

    #[test]
    fn parse_postfilter() {
        let command = Command::parse(b"postfilter");
        assert_eq!(command, Ok(Command::Show(ShowCommand::PostFilter)));
    }

    #[test]
    fn parse_postfilter_off() {
        let command = Command::parse(b"postfilter 1 off");
        assert_eq!(
            command,
            Ok(Command::PostFilter {
                channel: 1,
                rate: None,
            })
        );
    }

    #[test]
    fn parse_postfilter_rate() {
        let command = Command::parse(b"postfilter 0 rate 21");
        assert_eq!(
            command,
            Ok(Command::PostFilter {
                channel: 0,
                rate: Some(21.0),
            })
        );
    }

    #[test]
    fn parse_center_point() {
        let command = Command::parse(b"center 0 1.5");
        assert_eq!(
            command,
            Ok(Command::CenterPoint {
                channel: 0,
                center: CenterPoint::Override(1.5),
            })
        );
    }

    #[test]
    fn parse_center_point_vref() {
        let command = Command::parse(b"center 1 vref");
        assert_eq!(
            command,
            Ok(Command::CenterPoint {
                channel: 1,
                center: CenterPoint::VRef,
            })
        );
    }

    #[test]
    fn parse_fan_show() {
        let command = Command::parse(b"fan");
        assert_eq!(command, Ok(Command::ShowFan));
    }

    #[test]
    fn parse_fan_set() {
        let command = Command::parse(b"fan 42");
        assert_eq!(command, Ok(Command::FanSet { fan_pwm: 42 }));
    }

    #[test]
    fn parse_fan_auto() {
        let command = Command::parse(b"fan auto");
        assert_eq!(command, Ok(Command::FanAuto));
    }

    #[test]
    fn parse_fcurve_set() {
        let command = Command::parse(b"fcurve 1.2 3.4 5.6");
        assert_eq!(
            command,
            Ok(Command::FanCurve {
                k_a: 1.2,
                k_b: 3.4,
                k_c: 5.6
            })
        );
    }

    #[test]
    fn parse_fcurve_default() {
        let command = Command::parse(b"fcurve default");
        assert_eq!(command, Ok(Command::FanCurveDefaults));
    }

    #[test]
    fn parse_hwrev() {
        let command = Command::parse(b"hwrev");
        assert_eq!(command, Ok(Command::ShowHWRev));
    }
}
