use spake2::{Ed25519Group, Identity, Password, Spake2};

use super::keys::SessionKeys;

pub fn sender_start(pin: &str) -> (Vec<u8>, Spake2<Ed25519Group>) {
    let (state, outbound_msg) = Spake2::<Ed25519Group>::start_a(
        &Password::new(pin.as_bytes()),
        &Identity::new(b"sender"),
        &Identity::new(b"receiver"),
    );
    (outbound_msg, state)
}

pub fn sender_finish(
    state: Spake2<Ed25519Group>,
    inbound_msg: &[u8],
) -> Result<SessionKeys, PairingError> {
    let shared_secret = state
        .finish(inbound_msg)
        .map_err(|_| PairingError::KeyAgreementFailed)?;
    Ok(SessionKeys::derive(&shared_secret))
}

pub fn receiver_start(pin: &str) -> (Vec<u8>, Spake2<Ed25519Group>) {
    let (state, outbound_msg) = Spake2::<Ed25519Group>::start_b(
        &Password::new(pin.as_bytes()),
        &Identity::new(b"sender"),
        &Identity::new(b"receiver"),
    );
    (outbound_msg, state)
}

pub fn receiver_finish(
    state: Spake2<Ed25519Group>,
    inbound_msg: &[u8],
) -> Result<SessionKeys, PairingError> {
    let shared_secret = state
        .finish(inbound_msg)
        .map_err(|_| PairingError::KeyAgreementFailed)?;
    Ok(SessionKeys::derive(&shared_secret))
}

pub fn generate_pin() -> String {
    use rand::Rng;
    let n: u32 = rand::thread_rng().gen_range(0..1_000_000);
    format!("{n:06}")
}

#[derive(Debug, Clone, PartialEq)]
pub enum PairingError {
    KeyAgreementFailed,
}

impl std::fmt::Display for PairingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KeyAgreementFailed => write!(f, "SPAKE2 key agreement failed (wrong PIN?)"),
        }
    }
}

impl std::error::Error for PairingError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_pin_succeeds() {
        let pin = "123456";
        let (msg_a, state_a) = sender_start(pin);
        let (msg_b, state_b) = receiver_start(pin);

        let keys_a = sender_finish(state_a, &msg_b).unwrap();
        let keys_b = receiver_finish(state_b, &msg_a).unwrap();

        assert_eq!(keys_a.control_key, keys_b.control_key);
        assert_eq!(keys_a.media_key, keys_b.media_key);
    }

    #[test]
    fn mismatched_pin_produces_different_keys() {
        let (msg_a, state_a) = sender_start("111111");
        let (msg_b, state_b) = receiver_start("222222");

        let keys_a = sender_finish(state_a, &msg_b).unwrap();
        let keys_b = receiver_finish(state_b, &msg_a).unwrap();

        assert_ne!(keys_a.control_key, keys_b.control_key);
    }

    #[test]
    fn generate_pin_is_6_digits() {
        for _ in 0..100 {
            let pin = generate_pin();
            assert_eq!(pin.len(), 6);
            assert!(pin.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn end_to_end_with_cipher() {
        use crate::security::cipher::Cipher;

        let pin = "847291";
        let (msg_a, state_a) = sender_start(pin);
        let (msg_b, state_b) = receiver_start(pin);

        let keys_a = sender_finish(state_a, &msg_b).unwrap();
        let keys_b = receiver_finish(state_b, &msg_a).unwrap();

        let mut sender_cipher = Cipher::new(&keys_a.media_key, [0, 0, 0, 1]);
        let receiver_cipher = Cipher::new(&keys_b.media_key, [0, 0, 0, 1]);

        let sealed = sender_cipher.seal(b"rtp-packet-data");
        let (counter, plaintext) = receiver_cipher.open(&sealed).unwrap();
        assert_eq!(counter, 0);
        assert_eq!(plaintext, b"rtp-packet-data");
    }
}
