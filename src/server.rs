use crate::command_parser::Ipv4Config;
use crate::net::split_ipv4_config;
use smoltcp::{
    iface::EthernetInterface,
    socket::{SocketHandle, SocketRef, SocketSet, TcpSocket, TcpSocketBuffer},
    time::Instant,
    wire::{IpAddress, IpCidr, Ipv4Address, Ipv4Cidr},
};

pub struct SocketState<S> {
    handle: SocketHandle,
    state: S,
}

impl<'a, S: Default> SocketState<S> {
    fn new(
        sockets: &mut SocketSet<'a>,
        tcp_rx_storage: &'a mut [u8; TCP_RX_BUFFER_SIZE],
        tcp_tx_storage: &'a mut [u8; TCP_TX_BUFFER_SIZE],
    ) -> SocketState<S> {
        let tcp_rx_buffer = TcpSocketBuffer::new(&mut tcp_rx_storage[..]);
        let tcp_tx_buffer = TcpSocketBuffer::new(&mut tcp_tx_storage[..]);
        let tcp_socket = TcpSocket::new(tcp_rx_buffer, tcp_tx_buffer);
        SocketState::<S> {
            handle: sockets.add(tcp_socket),
            state: S::default(),
        }
    }
}

/// Number of server sockets and therefore concurrent client
/// sessions. Many data structures in `Server::run()` correspond to
/// this const.
const SOCKET_COUNT: usize = 4;

const TCP_RX_BUFFER_SIZE: usize = 2048;
const TCP_TX_BUFFER_SIZE: usize = 2048;

/// Contains a number of server sockets that get all sent the same
/// data (through `fmt::Write`).
pub struct Server<'a, 'b, S> {
    net: EthernetInterface<'a, &'a mut stm32_eth::Eth<'static, 'static>>,
    sockets: SocketSet<'b>,
    states: [SocketState<S>; SOCKET_COUNT],
}

impl<'a, 'b, S: Default> Server<'a, 'b, S> {
    /// Run a server with stack-allocated sockets
    pub fn run<F>(net: EthernetInterface<'a, &'a mut stm32_eth::Eth<'static, 'static>>, f: F)
    where
        F: FnOnce(&mut Server<'a, '_, S>),
    {
        macro_rules! create_rtx_storage {
            ($rx_storage:ident, $tx_storage:ident) => {
                let mut $rx_storage = [0; TCP_RX_BUFFER_SIZE];
                let mut $tx_storage = [0; TCP_TX_BUFFER_SIZE];
            };
        }

        create_rtx_storage!(tcp_rx_storage0, tcp_tx_storage0);
        create_rtx_storage!(tcp_rx_storage1, tcp_tx_storage1);
        create_rtx_storage!(tcp_rx_storage2, tcp_tx_storage2);
        create_rtx_storage!(tcp_rx_storage3, tcp_tx_storage3);

        let mut sockets_storage: [_; SOCKET_COUNT] = Default::default();
        let mut sockets = SocketSet::new(&mut sockets_storage[..]);

        let states: [SocketState<S>; SOCKET_COUNT] = [
            SocketState::<S>::new(&mut sockets, &mut tcp_rx_storage0, &mut tcp_tx_storage0),
            SocketState::<S>::new(&mut sockets, &mut tcp_rx_storage1, &mut tcp_tx_storage1),
            SocketState::<S>::new(&mut sockets, &mut tcp_rx_storage2, &mut tcp_tx_storage2),
            SocketState::<S>::new(&mut sockets, &mut tcp_rx_storage3, &mut tcp_tx_storage3),
        ];

        let mut server = Server {
            states,
            sockets,
            net,
        };
        f(&mut server);
    }

    /// Poll the interface and the sockets
    pub fn poll(&mut self, now: Instant) -> Result<(), smoltcp::Error> {
        // Poll smoltcp EthernetInterface,
        // pass only unexpected smoltcp errors to the caller
        match self.net.poll(&mut self.sockets, now) {
            Ok(_) => Ok(()),
            Err(smoltcp::Error::Malformed) => Ok(()),
            Err(smoltcp::Error::Unrecognized) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Iterate over all sockets managed by this server
    pub fn for_each<F: FnMut(SocketRef<TcpSocket>, &mut S)>(&mut self, mut callback: F) {
        for state in &mut self.states {
            let socket = self.sockets.get::<TcpSocket>(state.handle);
            callback(socket, &mut state.state);
        }
    }

    fn set_ipv4_address(&mut self, ipv4_address: Ipv4Cidr) {
        self.net.update_ip_addrs(|addrs| {
            for addr in addrs.iter_mut() {
                if let IpCidr::Ipv4(_) = addr {
                    *addr = IpCidr::Ipv4(ipv4_address);
                    // done
                    break;
                }
            }
        });
    }

    fn set_gateway(&mut self, gateway: Option<Ipv4Address>) {
        let routes = self.net.routes_mut();
        match gateway {
            None => routes.update(|routes_storage| {
                routes_storage.remove(&IpCidr::new(IpAddress::v4(0, 0, 0, 0), 0));
            }),
            Some(gateway) => {
                routes.add_default_ipv4_route(gateway).unwrap();
            }
        }
    }

    pub fn set_ipv4_config(&mut self, config: Ipv4Config) {
        let (address, gateway) = split_ipv4_config(config);
        self.set_ipv4_address(address);
        self.set_gateway(gateway);
    }
}
