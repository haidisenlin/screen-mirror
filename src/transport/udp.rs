use anyhow::Result;
use std::net::{SocketAddr, UdpSocket};

pub struct UdpSender {
    socket: UdpSocket,
    target: SocketAddr,
}

impl UdpSender {
    pub fn new(target: SocketAddr) -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        Ok(Self { socket, target })
    }

    pub fn send(&self, data: &[u8]) -> Result<()> {
        self.socket.send_to(data, self.target)?;
        Ok(())
    }
}

pub struct UdpReceiver {
    socket: UdpSocket,
}

impl UdpReceiver {
    pub fn new(bind_addr: SocketAddr) -> Result<Self> {
        let socket = UdpSocket::bind(bind_addr)?;
        Ok(Self { socket })
    }

    pub fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        let (n, _addr) = self.socket.recv_from(buf)?;
        Ok(n)
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> Result<()> {
        self.socket.set_nonblocking(nonblocking)?;
        Ok(())
    }
}
