use crate::{
    channel::{Channel0, Channel1},
    fan_ctrl::FanPin,
    hw_rev::{HWRev, HWSettings},
    leds::Leds,
};
use eeprom24x::{self, Eeprom24x};
use stm32_eth::EthPins;
use stm32f4xx_hal::{
    adc::Adc,
    gpio::{
        gpioa::*, gpiob::*, gpioc::*, gpioe::*, gpiof::*, gpiog::*, Alternate, AlternateOD, Analog,
        Floating, GpioExt, Input, Output, PushPull, AF5,
    },
    hal::{self, blocking::spi::Transfer, digital::v2::OutputPin},
    i2c::I2c,
    otg_fs::USB,
    pac::{
        ADC1, GPIOA, GPIOB, GPIOC, GPIOD, GPIOE, GPIOF, GPIOG, I2C1, OTG_FS_DEVICE, OTG_FS_GLOBAL,
        OTG_FS_PWRCLK, SPI2, SPI4, SPI5, TIM1, TIM3, TIM8,
    },
    pwm::{self, PwmChannels},
    rcc::Clocks,
    spi::{NoMiso, Spi, TransferModeNormal},
    time::U32Ext,
    timer::Timer,
};

pub type Eeprom = Eeprom24x<
    I2c<
        I2C1,
        (
            PB8<AlternateOD<{ stm32f4xx_hal::gpio::AF4 }>>,
            PB9<AlternateOD<{ stm32f4xx_hal::gpio::AF4 }>>,
        ),
    >,
    eeprom24x::page_size::B8,
    eeprom24x::addr_size::OneByte,
>;

pub type EthernetPins = EthPins<
    PA1<Input<Floating>>,
    PA7<Input<Floating>>,
    PB11<Input<Floating>>,
    PG13<Input<Floating>>,
    PB13<Input<Floating>>,
    PC4<Input<Floating>>,
    PC5<Input<Floating>>,
>;

pub trait ChannelPins {
    type DacSpi: Transfer<u8>;
    type DacSync: OutputPin;
    type Shdn: OutputPin;
    type VRefPin;
    type ITecPin;
    type DacFeedbackPin;
    type TecUMeasPin;
}

pub enum Channel0VRef {
    Analog(PA0<Analog>),
    Disabled(PA0<Input<Floating>>),
}

impl ChannelPins for Channel0 {
    type DacSpi = Dac0Spi;
    type DacSync = PE4<Output<PushPull>>;
    type Shdn = PE10<Output<PushPull>>;
    type VRefPin = Channel0VRef;
    type ITecPin = PA6<Analog>;
    type DacFeedbackPin = PA4<Analog>;
    type TecUMeasPin = PC2<Analog>;
}

pub enum Channel1VRef {
    Analog(PA3<Analog>),
    Disabled(PA3<Input<Floating>>),
}

impl ChannelPins for Channel1 {
    type DacSpi = Dac1Spi;
    type DacSync = PF6<Output<PushPull>>;
    type Shdn = PE15<Output<PushPull>>;
    type VRefPin = Channel1VRef;
    type ITecPin = PB0<Analog>;
    type DacFeedbackPin = PA5<Analog>;
    type TecUMeasPin = PC3<Analog>;
}

/// SPI peripheral used for communication with the ADC
pub type AdcSpi = Spi<
    SPI2,
    (
        PB10<Alternate<AF5>>,
        PB14<Alternate<AF5>>,
        PB15<Alternate<AF5>>,
    ),
    TransferModeNormal,
>;
pub type AdcNss = PB12<Output<PushPull>>;
type Dac0Spi = Spi<SPI4, (PE2<Alternate<AF5>>, NoMiso, PE6<Alternate<AF5>>), TransferModeNormal>;
type Dac1Spi = Spi<SPI5, (PF7<Alternate<AF5>>, NoMiso, PF9<Alternate<AF5>>), TransferModeNormal>;
pub type PinsAdc = Adc<ADC1>;

pub struct ChannelPinSet<C: ChannelPins> {
    pub dac_spi: C::DacSpi,
    pub dac_sync: C::DacSync,
    pub shdn: C::Shdn,
    pub vref_pin: C::VRefPin,
    pub itec_pin: C::ITecPin,
    pub dac_feedback_pin: C::DacFeedbackPin,
    pub tec_u_meas_pin: C::TecUMeasPin,
}

pub struct HWRevPins {
    pub hwrev0: stm32f4xx_hal::gpio::gpiod::PD0<Input<Floating>>,
    pub hwrev1: stm32f4xx_hal::gpio::gpiod::PD1<Input<Floating>>,
    pub hwrev2: stm32f4xx_hal::gpio::gpiod::PD2<Input<Floating>>,
    pub hwrev3: stm32f4xx_hal::gpio::gpiod::PD3<Input<Floating>>,
}

pub struct Pins {
    pub adc_spi: AdcSpi,
    pub adc_nss: AdcNss,
    pub pins_adc: PinsAdc,
    pub pwm: PwmPins,
    pub channel0: ChannelPinSet<Channel0>,
    pub channel1: ChannelPinSet<Channel1>,
}

impl Pins {
    /// Setup GPIO pins and configure MCU peripherals
    pub fn setup(
        clocks: Clocks,
        (tim1, tim3, tim8): (TIM1, TIM3, TIM8),
        (gpioa, gpiob, gpioc, gpiod, gpioe, gpiof, gpiog): (
            GPIOA,
            GPIOB,
            GPIOC,
            GPIOD,
            GPIOE,
            GPIOF,
            GPIOG,
        ),
        i2c1: I2C1,
        (spi2, spi4, spi5): (SPI2, SPI4, SPI5),
        adc1: ADC1,
        (otg_fs_global, otg_fs_device, otg_fs_pwrclk): (
            OTG_FS_GLOBAL,
            OTG_FS_DEVICE,
            OTG_FS_PWRCLK,
        ),
    ) -> (
        Self,
        Leds,
        Eeprom,
        EthernetPins,
        USB,
        Option<FanPin>,
        HWRev,
        HWSettings,
    ) {
        let gpioa = gpioa.split();
        let gpiob = gpiob.split();
        let gpioc = gpioc.split();
        let gpiod = gpiod.split();
        let gpioe = gpioe.split();
        let gpiof = gpiof.split();
        let gpiog = gpiog.split();

        let adc_spi = Self::setup_spi_adc(clocks, spi2, gpiob.pb10, gpiob.pb14, gpiob.pb15);
        let adc_nss = gpiob.pb12.into_push_pull_output();

        let pins_adc = Adc::adc1(adc1, true, Default::default());

        let pwm = PwmPins::setup(
            clocks,
            (tim1, tim3),
            (gpioc.pc6, gpioc.pc7),
            (gpioe.pe9, gpioe.pe11),
            (gpioe.pe13, gpioe.pe14),
        );

        let hwrev = HWRev::detect_hw_rev(&HWRevPins {
            hwrev0: gpiod.pd0,
            hwrev1: gpiod.pd1,
            hwrev2: gpiod.pd2,
            hwrev3: gpiod.pd3,
        });
        let hw_settings = hwrev.settings();

        let (dac0_spi, dac0_sync) = Self::setup_dac0(clocks, spi4, gpioe.pe2, gpioe.pe4, gpioe.pe6);
        let mut shdn0 = gpioe.pe10.into_push_pull_output();
        shdn0.set_low();
        let vref0_pin = if hwrev.major > 2 {
            Channel0VRef::Analog(gpioa.pa0.into_analog())
        } else {
            Channel0VRef::Disabled(gpioa.pa0)
        };
        let itec0_pin = gpioa.pa6.into_analog();
        let dac_feedback0_pin = gpioa.pa4.into_analog();
        let tec_u_meas0_pin = gpioc.pc2.into_analog();
        let channel0 = ChannelPinSet {
            dac_spi: dac0_spi,
            dac_sync: dac0_sync,
            shdn: shdn0,
            vref_pin: vref0_pin,
            itec_pin: itec0_pin,
            dac_feedback_pin: dac_feedback0_pin,
            tec_u_meas_pin: tec_u_meas0_pin,
        };

        let (dac1_spi, dac1_sync) = Self::setup_dac1(clocks, spi5, gpiof.pf7, gpiof.pf6, gpiof.pf9);
        let mut shdn1 = gpioe.pe15.into_push_pull_output();
        shdn1.set_low();
        let vref1_pin = if hwrev.major > 2 {
            Channel1VRef::Analog(gpioa.pa3.into_analog())
        } else {
            Channel1VRef::Disabled(gpioa.pa3)
        };
        let itec1_pin = gpiob.pb0.into_analog();
        let dac_feedback1_pin = gpioa.pa5.into_analog();
        let tec_u_meas1_pin = gpioc.pc3.into_analog();
        let channel1 = ChannelPinSet {
            dac_spi: dac1_spi,
            dac_sync: dac1_sync,
            shdn: shdn1,
            vref_pin: vref1_pin,
            itec_pin: itec1_pin,
            dac_feedback_pin: dac_feedback1_pin,
            tec_u_meas_pin: tec_u_meas1_pin,
        };

        let pins = Pins {
            adc_spi,
            adc_nss,
            pins_adc,
            pwm,
            channel0,
            channel1,
        };

        let leds = Leds::new(
            gpiod.pd9,
            gpiod.pd10.into_push_pull_output(),
            gpiod.pd11.into_push_pull_output(),
        );

        let eeprom_scl = gpiob.pb8.into_alternate().set_open_drain();
        let eeprom_sda = gpiob.pb9.into_alternate().set_open_drain();
        let eeprom_i2c = I2c::new(i2c1, (eeprom_scl, eeprom_sda), 400.khz(), clocks);
        let eeprom = Eeprom24x::new_24x02(eeprom_i2c, eeprom24x::SlaveAddr::default());

        let eth_pins = EthPins {
            ref_clk: gpioa.pa1,
            crs: gpioa.pa7,
            tx_en: gpiob.pb11,
            tx_d0: gpiog.pg13,
            tx_d1: gpiob.pb13,
            rx_d0: gpioc.pc4,
            rx_d1: gpioc.pc5,
        };

        let usb = USB {
            usb_global: otg_fs_global,
            usb_device: otg_fs_device,
            usb_pwrclk: otg_fs_pwrclk,
            pin_dm: gpioa.pa11.into_alternate(),
            pin_dp: gpioa.pa12.into_alternate(),
            hclk: clocks.hclk(),
        };

        let fan = if hw_settings.fan_available {
            Some(
                Timer::new(tim8, &clocks)
                    .pwm(gpioc.pc9.into_alternate(), hw_settings.fan_pwm_freq_hz.hz()),
            )
        } else {
            None
        };

        (pins, leds, eeprom, eth_pins, usb, fan, hwrev, hw_settings)
    }

    /// Configure the GPIO pins for SPI operation, and initialize SPI
    fn setup_spi_adc<M1, M2, M3>(
        clocks: Clocks,
        spi2: SPI2,
        sck: PB10<M1>,
        miso: PB14<M2>,
        mosi: PB15<M3>,
    ) -> AdcSpi {
        let sck = sck.into_alternate();
        let miso = miso.into_alternate();
        let mosi = mosi.into_alternate();
        Spi::new(
            spi2,
            (sck, miso, mosi),
            crate::ad7172::SPI_MODE,
            crate::ad7172::SPI_CLOCK,
            clocks,
        )
    }

    fn setup_dac0<M1, M2, M3>(
        clocks: Clocks,
        spi4: SPI4,
        sclk: PE2<M1>,
        sync: PE4<M2>,
        sdin: PE6<M3>,
    ) -> (Dac0Spi, <Channel0 as ChannelPins>::DacSync) {
        let sclk = sclk.into_alternate();
        let sdin = sdin.into_alternate();
        let spi = Spi::new(
            spi4,
            (sclk, NoMiso {}, sdin),
            crate::ad5680::SPI_MODE,
            crate::ad5680::SPI_CLOCK,
            clocks,
        );
        let sync = sync.into_push_pull_output();

        (spi, sync)
    }

    fn setup_dac1<M1, M2, M3>(
        clocks: Clocks,
        spi5: SPI5,
        sclk: PF7<M1>,
        sync: PF6<M2>,
        sdin: PF9<M3>,
    ) -> (Dac1Spi, <Channel1 as ChannelPins>::DacSync) {
        let sclk = sclk.into_alternate();
        let sdin = sdin.into_alternate();
        let spi = Spi::new(
            spi5,
            (sclk, NoMiso {}, sdin),
            crate::ad5680::SPI_MODE,
            crate::ad5680::SPI_CLOCK,
            clocks,
        );
        let sync = sync.into_push_pull_output();

        (spi, sync)
    }
}

pub struct PwmPins {
    pub max_v0: PwmChannels<TIM3, pwm::C1>,
    pub max_v1: PwmChannels<TIM3, pwm::C2>,
    pub max_i_pos0: PwmChannels<TIM1, pwm::C1>,
    pub max_i_pos1: PwmChannels<TIM1, pwm::C2>,
    pub max_i_neg0: PwmChannels<TIM1, pwm::C3>,
    pub max_i_neg1: PwmChannels<TIM1, pwm::C4>,
}

impl PwmPins {
    fn setup<M1, M2, M3, M4, M5, M6>(
        clocks: Clocks,
        (tim1, tim3): (TIM1, TIM3),
        (max_v0, max_v1): (PC6<M1>, PC7<M2>),
        (max_i_pos0, max_i_pos1): (PE9<M3>, PE11<M4>),
        (max_i_neg0, max_i_neg1): (PE13<M5>, PE14<M6>),
    ) -> PwmPins {
        let freq = 20u32.khz();

        fn init_pwm_pin<P: hal::PwmPin<Duty = u16>>(pin: &mut P) {
            pin.set_duty(0);
            pin.enable();
        }
        let channels = (max_v0.into_alternate(), max_v1.into_alternate());
        //let (mut max_v0, mut max_v1) = pwm::tim3(tim3, channels, clocks, freq);
        let (mut max_v0, mut max_v1) = Timer::new(tim3, &clocks).pwm(channels, freq);
        init_pwm_pin(&mut max_v0);
        init_pwm_pin(&mut max_v1);

        let channels = (
            max_i_pos0.into_alternate(),
            max_i_pos1.into_alternate(),
            max_i_neg0.into_alternate(),
            max_i_neg1.into_alternate(),
        );
        let (mut max_i_pos0, mut max_i_pos1, mut max_i_neg0, mut max_i_neg1) =
            Timer::new(tim1, &clocks).pwm(channels, freq);
        init_pwm_pin(&mut max_i_pos0);
        init_pwm_pin(&mut max_i_neg0);
        init_pwm_pin(&mut max_i_pos1);
        init_pwm_pin(&mut max_i_neg1);

        PwmPins {
            max_v0,
            max_v1,
            max_i_pos0,
            max_i_pos1,
            max_i_neg0,
            max_i_neg1,
        }
    }
}
