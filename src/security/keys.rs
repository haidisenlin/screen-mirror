use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SessionKeys {
    pub control_key: [u8; 32],
    pub media_key: [u8; 32],
}

impl SessionKeys {
    /// Derive control and media keys from a SPAKE2+ shared secret.
    pub fn derive(shared_secret: &[u8]) -> Self {
        let hk = Hkdf::<Sha256>::new(None, shared_secret);

        let mut control_key = [0u8; 32];
        hk.expand(b"screenmirror-control-v1", &mut control_key)
            .expect("32 bytes is a valid HKDF-SHA256 output length");

        let mut media_key = [0u8; 32];
        hk.expand(b"screenmirror-media-v1", &mut media_key)
            .expect("32 bytes is a valid HKDF-SHA256 output length");

        Self {
            control_key,
            media_key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_produces_different_keys() {
        let secret = b"test-shared-secret-from-spake2-plus";
        let keys = SessionKeys::derive(secret);
        assert_ne!(keys.control_key, keys.media_key);
    }

    #[test]
    fn derive_is_deterministic() {
        let secret = b"reproducible";
        let k1 = SessionKeys::derive(secret);
        let k2 = SessionKeys::derive(secret);
        assert_eq!(k1.control_key, k2.control_key);
        assert_eq!(k1.media_key, k2.media_key);
    }

    #[test]
    fn different_secrets_produce_different_keys() {
        let k1 = SessionKeys::derive(b"secret-a");
        let k2 = SessionKeys::derive(b"secret-b");
        assert_ne!(k1.control_key, k2.control_key);
        assert_ne!(k1.media_key, k2.media_key);
    }
}
