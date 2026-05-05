use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit},
};
use std::fmt;

pub const TAG_LEN: usize = 16;
pub const NONCE_COUNTER_LEN: usize = 8;

#[derive(Debug, PartialEq)]
pub enum CipherError {
    TooShort,
    AuthFailed,
}

impl fmt::Display for CipherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CipherError::TooShort => write!(f, "packet too short"),
            CipherError::AuthFailed => write!(f, "authentication failed"),
        }
    }
}

impl std::error::Error for CipherError {}

pub struct Cipher {
    chacha: ChaCha20Poly1305,
    prefix: [u8; 4],
    send_counter: u64,
}

impl Cipher {
    pub fn new(key: &[u8; 32], prefix: [u8; 4]) -> Self {
        Self {
            chacha: ChaCha20Poly1305::new(key.into()),
            prefix,
            send_counter: 0,
        }
    }

    /// Encrypts plaintext. Returns `[8B counter | ciphertext | 16B tag]`.
    pub fn seal(&mut self, plaintext: &[u8]) -> Vec<u8> {
        let counter = self.send_counter;
        self.send_counter += 1;

        let nonce = self.build_nonce(counter);
        let ciphertext_with_tag = self
            .chacha
            .encrypt(&nonce, plaintext)
            .expect("encryption should not fail");

        let mut packet = Vec::with_capacity(NONCE_COUNTER_LEN + ciphertext_with_tag.len());
        packet.extend_from_slice(&counter.to_be_bytes());
        packet.extend_from_slice(&ciphertext_with_tag);
        packet
    }

    /// Decrypts a packet. Returns `(counter, plaintext)`.
    pub fn open(&self, packet: &[u8]) -> Result<(u64, Vec<u8>), CipherError> {
        // Minimum: 8B counter + 0B plaintext + 16B tag
        if packet.len() < NONCE_COUNTER_LEN + TAG_LEN {
            return Err(CipherError::TooShort);
        }

        let counter = u64::from_be_bytes(packet[..NONCE_COUNTER_LEN].try_into().unwrap());
        let ciphertext_with_tag = &packet[NONCE_COUNTER_LEN..];

        let nonce = self.build_nonce(counter);
        let plaintext = self
            .chacha
            .decrypt(&nonce, ciphertext_with_tag)
            .map_err(|_| CipherError::AuthFailed)?;

        Ok((counter, plaintext))
    }

    fn build_nonce(&self, counter: u64) -> Nonce {
        let mut nonce = [0u8; 12];
        nonce[..4].copy_from_slice(&self.prefix);
        nonce[4..].copy_from_slice(&counter.to_be_bytes());
        *Nonce::from_slice(&nonce)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let key = [0x42u8; 32];
        let mut sender = Cipher::new(&key, [0, 0, 0, 0]);
        let receiver = Cipher::new(&key, [0, 0, 0, 0]);
        let plaintext = b"hello world";
        let sealed = sender.seal(plaintext);
        assert_eq!(sealed.len(), 8 + plaintext.len() + 16);
        let (counter, decrypted) = receiver.open(&sealed).unwrap();
        assert_eq!(counter, 0);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn counter_increments() {
        let key = [0xABu8; 32];
        let mut sender = Cipher::new(&key, [0, 0, 0, 1]);
        let s1 = sender.seal(b"first");
        let s2 = sender.seal(b"second");
        let c1 = u64::from_be_bytes(s1[..8].try_into().unwrap());
        let c2 = u64::from_be_bytes(s2[..8].try_into().unwrap());
        assert_eq!(c1, 0);
        assert_eq!(c2, 1);
    }

    #[test]
    fn tampered_packet_fails() {
        let key = [0x99u8; 32];
        let mut sender = Cipher::new(&key, [0, 0, 0, 0]);
        let receiver = Cipher::new(&key, [0, 0, 0, 0]);
        let mut sealed = sender.seal(b"secret");
        sealed[10] ^= 0xFF;
        assert_eq!(receiver.open(&sealed), Err(CipherError::AuthFailed));
    }

    #[test]
    fn wrong_key_fails() {
        let mut sender = Cipher::new(&[0x11u8; 32], [0, 0, 0, 0]);
        let receiver = Cipher::new(&[0x22u8; 32], [0, 0, 0, 0]);
        let sealed = sender.seal(b"data");
        assert_eq!(receiver.open(&sealed), Err(CipherError::AuthFailed));
    }

    #[test]
    fn too_short_fails() {
        let receiver = Cipher::new(&[0u8; 32], [0, 0, 0, 0]);
        assert_eq!(receiver.open(&[0u8; 20]), Err(CipherError::TooShort));
    }
}
