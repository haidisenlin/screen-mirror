use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::Result;

use crate::security::cipher::Cipher;
use crate::security::replay::TcpCounterCheck;

/// Framed, encrypted TCP transport for the control channel.
/// Wire format: [4B big-endian length | encrypted payload]
/// where encrypted payload = [8B counter | ciphertext | 16B tag]
pub struct SecureChannel {
    stream: TcpStream,
    send_cipher: Cipher,
    recv_cipher: Cipher,
    counter_check: TcpCounterCheck,
}

impl SecureChannel {
    pub fn new(stream: TcpStream, control_key: &[u8; 32], is_initiator: bool) -> Self {
        let (send_prefix, recv_prefix) = if is_initiator {
            ([0u8, 0, 0, 0], [0u8, 0, 0, 1])
        } else {
            ([0u8, 0, 0, 1], [0u8, 0, 0, 0])
        };
        Self {
            stream,
            send_cipher: Cipher::new(control_key, send_prefix),
            recv_cipher: Cipher::new(control_key, recv_prefix),
            counter_check: TcpCounterCheck::new(),
        }
    }

    pub fn send(&mut self, plaintext: &[u8]) -> Result<()> {
        let sealed = self.send_cipher.seal(plaintext);
        let len = sealed.len() as u32;
        self.stream.write_all(&len.to_be_bytes())?;
        self.stream.write_all(&sealed)?;
        Ok(())
    }

    pub fn recv(&mut self) -> Result<Option<Vec<u8>>> {
        let mut len_buf = [0u8; 4];
        match self.stream.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > 1024 * 1024 {
            anyhow::bail!("control frame too large: {len} bytes");
        }

        let mut buf = vec![0u8; len];
        self.stream.read_exact(&mut buf)?;

        let (counter, plaintext) = self
            .recv_cipher
            .open(&buf)
            .map_err(|e| anyhow::anyhow!("decrypt failed: {e}"))?;

        if !self.counter_check.check(counter) {
            anyhow::bail!("TCP counter replay/reorder detected: {counter}");
        }

        Ok(Some(plaintext))
    }

    pub fn set_read_timeout(&self, duration: Option<Duration>) -> Result<()> {
        self.stream.set_read_timeout(duration)?;
        Ok(())
    }

    pub fn shutdown(&self) -> Result<()> {
        self.stream.shutdown(std::net::Shutdown::Both)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn create_pair(key: &[u8; 32]) -> (SecureChannel, SecureChannel) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(addr).unwrap();
        let (server_stream, _) = listener.accept().unwrap();

        let client = SecureChannel::new(client_stream, key, true);
        let server = SecureChannel::new(server_stream, key, false);
        (client, server)
    }

    #[test]
    fn send_recv_roundtrip() {
        let key = [0x42u8; 32];
        let (mut client, mut server) = create_pair(&key);

        client.send(b"hello server").unwrap();
        let msg = server.recv().unwrap().unwrap();
        assert_eq!(msg, b"hello server");

        server.send(b"hello client").unwrap();
        let msg = client.recv().unwrap().unwrap();
        assert_eq!(msg, b"hello client");
    }

    #[test]
    fn multiple_messages() {
        let key = [0xAAu8; 32];
        let (mut client, mut server) = create_pair(&key);

        for i in 0..10 {
            let data = format!("message {i}");
            client.send(data.as_bytes()).unwrap();
        }

        for i in 0..10 {
            let msg = server.recv().unwrap().unwrap();
            assert_eq!(msg, format!("message {i}").as_bytes());
        }
    }

    #[test]
    fn closed_connection_returns_none() {
        let key = [0xBBu8; 32];
        let (client, mut server) = create_pair(&key);
        drop(client);
        let result = server.recv().unwrap();
        assert!(result.is_none());
    }
}
