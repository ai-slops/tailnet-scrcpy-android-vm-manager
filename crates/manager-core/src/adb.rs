use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use thiserror::Error;

const MAX_KEY_LINE_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbPublicKey {
    encoded: String,
    comment: Option<String>,
    fingerprint: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AdbKeyError {
    #[error("ADB public key is empty")]
    Empty,
    #[error("private key material is not accepted")]
    PrivateKey,
    #[error("ADB public key exceeds {MAX_KEY_LINE_BYTES} bytes")]
    TooLarge,
    #[error("ADB public key must contain exactly one line")]
    MultipleLines,
    #[error("ADB public key is not valid base64: {0}")]
    InvalidBase64(String),
    #[error("decoded ADB public key has an unexpected size: {0} bytes")]
    InvalidSize(usize),
}

impl AdbPublicKey {
    pub fn parse(input: &str) -> Result<Self, AdbKeyError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AdbKeyError::Empty);
        }
        if input.len() > MAX_KEY_LINE_BYTES {
            return Err(AdbKeyError::TooLarge);
        }
        if input.contains('\n') || input.contains('\r') {
            return Err(AdbKeyError::MultipleLines);
        }
        if input.contains("PRIVATE KEY") {
            return Err(AdbKeyError::PrivateKey);
        }

        let mut fields = input.splitn(2, char::is_whitespace);
        let encoded = fields.next().ok_or(AdbKeyError::Empty)?;
        let decoded = STANDARD
            .decode(encoded)
            .map_err(|error| AdbKeyError::InvalidBase64(error.to_string()))?;
        // Android's RSAPublicKey structure is 524 bytes for the supported
        // 2048-bit RSA ADB key format.
        if decoded.len() != 524 {
            return Err(AdbKeyError::InvalidSize(decoded.len()));
        }
        let comment = fields
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let fingerprint = format!("SHA256:{}", STANDARD.encode(Sha256::digest(&decoded)));

        Ok(Self {
            encoded: encoded.to_owned(),
            comment,
            fingerprint,
        })
    }

    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    #[must_use]
    pub fn authorized_line(&self) -> String {
        match &self.comment {
            Some(comment) => format!("{} {comment}", self.encoded),
            None => self.encoded.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_key(comment: &str) -> String {
        format!("{} {comment}", STANDARD.encode([7_u8; 524]))
    }

    #[test]
    fn parses_and_fingerprints_public_key() {
        let key = AdbPublicKey::parse(&valid_key("ios@test")).unwrap();
        assert!(key.fingerprint().starts_with("SHA256:"));
        assert!(key.authorized_line().ends_with("ios@test"));
    }

    #[test]
    fn fingerprint_ignores_comment() {
        let first = AdbPublicKey::parse(&valid_key("first")).unwrap();
        let second = AdbPublicKey::parse(&valid_key("second")).unwrap();
        assert_eq!(first.fingerprint(), second.fingerprint());
    }

    #[test]
    fn rejects_private_and_multiline_material() {
        assert_eq!(
            AdbPublicKey::parse("-----BEGIN PRIVATE KEY-----").unwrap_err(),
            AdbKeyError::PrivateKey
        );
        assert_eq!(
            AdbPublicKey::parse("abc\ndef").unwrap_err(),
            AdbKeyError::MultipleLines
        );
    }

    #[test]
    fn rejects_wrong_decoded_size() {
        let error = AdbPublicKey::parse(&STANDARD.encode([0_u8; 12])).unwrap_err();
        assert_eq!(error, AdbKeyError::InvalidSize(12));
    }
}
