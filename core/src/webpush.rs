//! Web Push without a push server of our own: the PC encrypts a message for one browser
//! subscription (RFC 8291, aes128gcm) and signs a VAPID token (RFC 8292). Sending the bytes is
//! the app's job; this module only builds them.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes128Gcm, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use hkdf::Hkdf;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::{PublicKey, SecretKey};
use rand_core::{OsRng, RngCore};
use sha2::Sha256;

/// The browser's half of a push subscription (`PushSubscription.toJSON()`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Subscription {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

/// Apple and RFC 8030 accept 4096 bytes of body: 86 of header, 16 of tag and 1 of padding.
pub const MAX_PLAINTEXT: usize = 4096 - 86 - 16 - 1;
const RECORD_SIZE: u32 = 4096;

pub fn b64(bytes: &[u8]) -> String {
    B64.encode(bytes)
}

pub fn unb64(s: &str) -> Result<Vec<u8>, String> {
    B64.decode(s.trim_end_matches('=')).map_err(|e| e.to_string())
}

/// A new VAPID private key, base64url (the config keeps it).
pub fn new_key() -> String {
    b64(&SecretKey::random(&mut OsRng).to_bytes())
}

fn secret(key_b64: &str) -> Result<SecretKey, String> {
    SecretKey::from_slice(&unb64(key_b64)?).map_err(|e| e.to_string())
}

/// The public half of a VAPID key: `applicationServerKey` for `pushManager.subscribe`.
pub fn public_key(key_b64: &str) -> Result<String, String> {
    Ok(b64(secret(key_b64)?.public_key().to_encoded_point(false).as_bytes()))
}

/// The encrypted body for one subscription.
pub fn encrypt(sub: &Subscription, plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    encrypt_with(&SecretKey::random(&mut OsRng), &salt, sub, plaintext)
}

fn hkdf(salt: &[u8], ikm: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    Hkdf::<Sha256>::new(Some(salt), ikm).expand(info, &mut out).expect("hkdf length");
    out
}

fn encrypt_with(as_key: &SecretKey, salt: &[u8; 16], sub: &Subscription, plaintext: &[u8]) -> Result<Vec<u8>, String> {
    if plaintext.len() > MAX_PLAINTEXT {
        return Err(format!("push message too long: {} bytes", plaintext.len()));
    }
    let ua_public = unb64(&sub.p256dh)?;
    let auth = unb64(&sub.auth)?;
    let ua_key = PublicKey::from_sec1_bytes(&ua_public).map_err(|e| e.to_string())?;
    let as_public = as_key.public_key().to_encoded_point(false);
    let shared = p256::ecdh::diffie_hellman(as_key.to_nonzero_scalar(), ua_key.as_affine());

    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(&ua_public);
    key_info.extend_from_slice(as_public.as_bytes());
    let ikm = hkdf(&auth, shared.raw_secret_bytes(), &key_info, 32);
    let cek = hkdf(salt, &ikm, b"Content-Encoding: aes128gcm\0", 16);
    let nonce = hkdf(salt, &ikm, b"Content-Encoding: nonce\0", 12);

    let mut padded = plaintext.to_vec();
    padded.push(2);
    let sealed = Aes128Gcm::new_from_slice(&cek)
        .map_err(|e| e.to_string())?
        .encrypt(Nonce::from_slice(&nonce), padded.as_slice())
        .map_err(|e| e.to_string())?;

    let mut body = salt.to_vec();
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(as_public.as_bytes().len() as u8);
    body.extend_from_slice(as_public.as_bytes());
    body.extend_from_slice(&sealed);
    Ok(body)
}

/// `Authorization` header value for a push to `endpoint`, valid 12 hours from `now_s`.
pub fn vapid_auth(key_b64: &str, endpoint: &str, subject: &str, now_s: i64) -> Result<String, String> {
    let origin: String = endpoint.splitn(4, '/').take(3).collect::<Vec<_>>().join("/");
    let header = b64(br#"{"typ":"JWT","alg":"ES256"}"#);
    let claims = b64(serde_json::json!({ "aud": origin, "exp": now_s + 12 * 3600, "sub": subject }).to_string().as_bytes());
    let signing_input = format!("{header}.{claims}");
    let key = SigningKey::from(secret(key_b64)?);
    let signature: Signature = key.sign(signing_input.as_bytes());
    Ok(format!("vapid t={signing_input}.{}, k={}", b64(&signature.to_bytes()), public_key(key_b64)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Verifier;
    use p256::ecdsa::VerifyingKey;

    /// RFC 8291, Section 5 and Appendix A.
    #[test]
    fn rfc8291_example() {
        let as_key = SecretKey::from_slice(&unb64("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw").unwrap()).unwrap();
        let salt: [u8; 16] = unb64("DGv6ra1nlYgDCS1FRnbzlw").unwrap().try_into().unwrap();
        let sub = Subscription {
            endpoint: "https://push.example.net/push/JzLQ3raZJfFBR0aqvOMsLrt54w4rJUsV".into(),
            p256dh: "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4".into(),
            auth: "BTBZMqHH6r4Tts7J_aSIgg".into(),
        };
        let body = encrypt_with(&as_key, &salt, &sub, b"When I grow up, I want to be a watermelon").unwrap();
        let header = unb64("DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8").unwrap();
        let ciphertext = unb64("8pfeW0KbunFT06SuDKoJH9Ql87S1QUrdirN6GcG7sFz1y1sqLgVi1VhjVkHsUoEsbI_0LpXMuGvnzQ").unwrap();
        assert_eq!(body, [header, ciphertext].concat());
    }

    #[test]
    fn too_long_is_refused() {
        let sub = Subscription { endpoint: String::new(), p256dh: String::new(), auth: String::new() };
        assert!(encrypt(&sub, &vec![b'x'; MAX_PLAINTEXT + 1]).is_err());
    }

    #[test]
    fn vapid_token_verifies() {
        let key = new_key();
        let auth = vapid_auth(&key, "https://web.push.apple.com/QGuQyavXutnMH", "https://github.com/NikGob/ClockManage", 1_700_000_000).unwrap();
        let (token, k) = auth.strip_prefix("vapid t=").unwrap().split_once(", k=").unwrap();
        assert_eq!(k, public_key(&key).unwrap());
        let (input, sig) = token.rsplit_once('.').unwrap();
        let claims: serde_json::Value = serde_json::from_slice(&unb64(input.split('.').nth(1).unwrap()).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://web.push.apple.com");
        let vk = VerifyingKey::from_sec1_bytes(&unb64(k).unwrap()).unwrap();
        let sig = Signature::from_slice(&unb64(sig).unwrap()).unwrap();
        assert!(vk.verify(input.as_bytes(), &sig).is_ok());
    }
}
