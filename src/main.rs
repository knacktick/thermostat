#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![cfg_attr(test, allow(unused))]
// TODO: #![deny(warnings, unused)]

#[cfg(not(any(feature = "semihosting", test)))]
use panic_halt as _;
#[cfg(all(feature = "semihosting", not(test)))]
use panic_semihosting as _;

use cortex_m::asm::wfi;
use cortex_m_rt::entry;
use log::{error, info, warn};
use smoltcp::{socket::TcpSocket, time::Instant, wire::EthernetAddress};
use stm32f4xx_hal::{
    hal::watchdog::{Watchdog, WatchdogEnable},
    rcc::RccExt,
    stm32::{CorePeripherals, Peripherals, SCB},
    time::{MegaHertz, U32Ext},
    watchdog::IndependentWatchdog,
};

mod init_log;
use init_log::init_log;
mod leds;
mod pins;
mod usb;
use pins::Pins;
mod ad5680;
mod ad7172;
mod net;
mod server;
use server::Server;
mod session;
use session::{Session, SessionInput};
mod command_parser;
use command_parser::Ipv4Config;
mod b_parameter;
mod channels;
mod pid;
mod timer;
use channels::{Channels, CHANNELS};
mod channel;
mod channel_state;
mod config;
use config::ChannelConfig;
mod command_handler;
mod dfu;
mod flash_store;
use command_handler::Handler;
mod fan_ctrl;
use fan_ctrl::FanCtrl;
mod hw_rev;

const HSE: MegaHertz = MegaHertz(8);
#[cfg(not(feature = "semihosting"))]
const WATCHDOG_INTERVAL: u32 = 1_000;
#[cfg(feature = "semihosting")]
const WATCHDOG_INTERVAL: u32 = 30_000;

const CHANNEL_CONFIG_KEY: [&str; 2] = ["ch0", "ch1"];

const TCP_PORT: u16 = 23;

fn send_line(socket: &mut TcpSocket, data: &[u8]) -> bool {
    let send_free = socket.send_capacity() - socket.send_queue();
    if data.len() > send_free + 1 {
        // Not enough buffer space, skip report for now,
        // instead of sending incomplete line
        warn!(
            "TCP socket has only {}/{} needed {}",
            send_free + 1,
            socket.send_capacity(),
            data.len(),
        );
    } else {
        match socket.send_slice(data) {
            Ok(sent) if sent == data.len() => {
                let _ = socket.send_slice(b"\n");
                // success
                return true;
            }
            Ok(sent) => warn!("sent only {}/{} bytes", sent, data.len()),
            Err(e) => error!("error sending line: {:?}", e),
        }
    }
    // not success
    false
}

/// Initialization and main loop
#[cfg(not(test))]
#[entry]
fn main() -> ! {
    init_log();
    info!("thermostat");

    let mut cp = CorePeripherals::take().unwrap();
    cp.SCB.enable_icache();
    cp.SCB.enable_dcache(&mut cp.CPUID);

    let dp = Peripherals::take().unwrap();
    let clocks = dp
        .RCC
        .constrain()
        .cfgr
        .use_hse(HSE)
        .sysclk(168.mhz())
        .hclk(168.mhz())
        .pclk1(32.mhz())
        .pclk2(64.mhz())
        .freeze();

    let mut wd = IndependentWatchdog::new(dp.IWDG);
    wd.start(WATCHDOG_INTERVAL.ms());
    wd.feed();

    timer::setup(cp.SYST, clocks);

    let (pins, mut leds, mut eeprom, eth_pins, usb, fan, hwrev, hw_settings) = Pins::setup(
        clocks,
        (dp.TIM1, dp.TIM3, dp.TIM8),
        (
            dp.GPIOA, dp.GPIOB, dp.GPIOC, dp.GPIOD, dp.GPIOE, dp.GPIOF, dp.GPIOG,
        ),
        dp.I2C1,
        (dp.SPI2, dp.SPI4, dp.SPI5),
        dp.ADC1,
        (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
    );

    leds.r1.on();
    leds.g3.off();
    leds.g4.off();

    usb::State::setup(usb);

    let mut store = flash_store::store(dp.FLASH);

    let mut channels = Channels::new(pins);
    for (c, key) in CHANNEL_CONFIG_KEY.iter().enumerate().take(CHANNELS) {
        match store.read_value::<ChannelConfig>(key) {
            Ok(Some(config)) => config.apply(&mut channels, c),
            Ok(None) => error!("flash config not found for channel {}", c),
            Err(e) => error!("unable to load config {} from flash: {:?}", c, e),
        }
    }

    let mut fan_ctrl = FanCtrl::new(fan, hw_settings);

    // default net config:
    let mut ipv4_config = Ipv4Config {
        address: [192, 168, 1, 26],
        mask_len: 24,
        gateway: None,
    };
    match store.read_value("ipv4") {
        Ok(Some(config)) => ipv4_config = config,
        Ok(None) => {}
        Err(e) => error!("cannot read ipv4 config: {:?}", e),
    }

    // EEPROM ships with a read-only EUI-48 identifier
    let mut eui48 = [0; 6];
    eeprom.read_data(0xFA, &mut eui48).unwrap();
    let hwaddr = EthernetAddress(eui48);
    info!("EEPROM MAC address: {}", hwaddr);

    net::run(
        clocks,
        dp.ETHERNET_MAC,
        dp.ETHERNET_DMA,
        eth_pins,
        hwaddr,
        ipv4_config.clone(),
        |iface| {
            Server::<Session>::run(iface, |server| {
                leds.r1.off();
                let mut should_reset = false;

                loop {
                    let mut new_ipv4_config = None;
                    let instant = Instant::from_millis(i64::from(timer::now()));
                    channels.poll_adc(instant);

                    fan_ctrl.cycle(channels.current_abs_max_tec_i());

                    if channels.pid_engaged() {
                        leds.g3.on();
                    } else {
                        leds.g3.off();
                    }

                    let instant = Instant::from_millis(i64::from(timer::now()));
                    cortex_m::interrupt::free(net::clear_pending);
                    server.poll(instant).unwrap_or_else(|e| {
                        warn!("poll: {:?}", e);
                    });

                    if !should_reset {
                        // TCP protocol handling
                        server.for_each(|mut socket, session| {
                            if !socket.is_active() {
                                let _ = socket.listen(TCP_PORT);
                                session.reset();
                            } else if socket.may_send() && !socket.may_recv() {
                                socket.close()
                            } else if socket.can_send() && socket.can_recv() {
                                match socket.recv(|buf| session.feed(buf)) {
                                    // SessionInput::Nothing happens when the line reader parses a string of characters that is not
                                    // followed by a newline character. Could be due to partial commands not terminated with newline,
                                    // socket RX ring buffer wraps around, or when the command is sent as seperate TCP packets etc.
                                    // Do nothing and feed more data to the line reader in the next loop cycle.
                                    Ok(SessionInput::Nothing) => {}
                                    Ok(SessionInput::Command(command)) => {
                                        match Handler::handle_command(
                                            command,
                                            &mut socket,
                                            &mut channels,
                                            &mut store,
                                            &mut ipv4_config,
                                            &mut fan_ctrl,
                                            hwrev,
                                        ) {
                                            Ok(Handler::NewIPV4(ip)) => new_ipv4_config = Some(ip),
                                            Ok(Handler::Handled) => {}
                                            Ok(Handler::CloseSocket) => socket.close(),
                                            Ok(Handler::Reset) => should_reset = true,
                                            Err(_) => {}
                                        }
                                    }
                                    Ok(SessionInput::Error(e)) => {
                                        error!("session input: {:?}", e);
                                        send_line(&mut socket, b"{ \"error\": \"invalid input\" }");
                                    }
                                    Err(_) => socket.close(),
                                }
                            }
                        });
                    } else {
                        // Should reset, close all TCP sockets.
                        let mut any_socket_alive = false;
                        server.for_each(|mut socket, _| {
                            if socket.is_active() {
                                socket.abort();
                                any_socket_alive = true;
                            }
                        });
                        // Must let loop run for one more cycle to poll server for RST to be sent,
                        // this makes sure system does not reset right after socket.abort() is called.
                        if !any_socket_alive {
                            SCB::sys_reset();
                        }
                    }

                    // Apply new IPv4 address/gateway
                    if let Some(config) = new_ipv4_config.take() {
                        server.set_ipv4_config(config.clone());
                        ipv4_config = config;
                    };

                    // Update watchdog
                    wd.feed();

                    leds.g4.off();
                    cortex_m::interrupt::free(|cs| {
                        if !net::is_pending(cs) {
                            // Wait for interrupts
                            // (Ethernet, SysTick, or USB)
                            wfi();
                        }
                    });
                    leds.g4.on();
                }
            });
        },
    );

    unreachable!()
}
