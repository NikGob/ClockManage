//! The iPhone web app and the PC talk through public ntfy.sh topics (no account, no server of
//! our own): every message is AES-256-GCM sealed with a key only the two of them know.
//!
//! Pairing: the PC shows a short code (and a QR with it). Both sides derive a topic and a key
//! from the code; the PC posts the long-term link there, sealed with the code key.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::webpush::{b64, unb64};

pub const NTFY: &str = "https://ntfy.sh";
/// ntfy.sh caps a message at 4096 bytes; a part with its prefix stays well below.
const PART: usize = 3000;
const CODE_ABC: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

/// A paired web app: two topics and the key that seals what goes through them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Link {
    pub key: String,
    /// Phone → PC: its push subscription, diet edits.
    pub up: String,
    /// PC → phone: the current diet.
    pub down: String,
}

fn random(n: usize) -> Vec<u8> {
    let mut v = vec![0u8; n];
    OsRng.fill_bytes(&mut v);
    v
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn new_link() -> Link {
    Link { key: b64(&random(32)), up: format!("cm{}", hex(&random(16))), down: format!("cm{}", hex(&random(16))) }
}

/// Eight characters without look-alikes (0/O, 1/I), shown as ABCD-EFGH.
pub fn pair_code() -> String {
    let code: String = random(8).iter().map(|b| CODE_ABC[(*b as usize) % CODE_ABC.len()] as char).collect();
    format!("{}-{}", &code[..4], &code[4..])
}

fn normalize(code: &str) -> String {
    code.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_uppercase()).collect()
}

pub fn pair_topic(code: &str) -> String {
    let h = Sha256::digest(format!("clockmanage-pair:{}", normalize(code)));
    format!("cm{}", hex(&h[..16]))
}

pub fn pair_key(code: &str) -> Vec<u8> {
    Sha256::digest(format!("clockmanage-key:{}", normalize(code))).to_vec()
}

/// base64url(nonce ‖ ciphertext).
pub fn seal(key: &[u8], plain: &[u8]) -> String {
    let nonce = random(12);
    let ct = Aes256Gcm::new_from_slice(key).expect("32-byte key").encrypt(Nonce::from_slice(&nonce), plain).expect("aes-gcm");
    b64(&[nonce, ct].concat())
}

pub fn open(key: &[u8], sealed: &str) -> Result<Vec<u8>, String> {
    let raw = unb64(sealed)?;
    if raw.len() < 28 || key.len() != 32 {
        return Err("bad message".into());
    }
    Aes256Gcm::new_from_slice(key)
        .map_err(|e| e.to_string())?
        .decrypt(Nonce::from_slice(&raw[..12]), &raw[12..])
        .map_err(|_| "bad message".to_string())
}

/// One sealed message as ntfy bodies: `id:i/n:part`.
pub fn parts(sealed: &str) -> Vec<String> {
    let id = hex(&random(4));
    let chunks: Vec<&str> = sealed.as_bytes().chunks(PART).map(|c| std::str::from_utf8(c).unwrap_or_default()).collect();
    let n = chunks.len();
    chunks.iter().enumerate().map(|(i, c)| format!("{id}:{i}/{n}:{c}")).collect()
}

/// Whole sealed messages out of ntfy bodies (in the order they arrived), with the time of
/// their last part; incomplete ones are skipped.
pub fn assemble(bodies: &[(i64, String)]) -> Vec<(i64, String)> {
    let mut groups: Vec<(String, usize, Vec<Option<String>>, i64)> = Vec::new();
    for (time, body) in bodies {
        let mut it = body.splitn(3, ':');
        let (Some(id), Some(pos), Some(part)) = (it.next(), it.next(), it.next()) else { continue };
        let Some((i, n)) = pos.split_once('/').and_then(|(i, n)| Some((i.parse::<usize>().ok()?, n.parse::<usize>().ok()?))) else { continue };
        if n == 0 || i >= n || n > 64 {
            continue;
        }
        let g = match groups.iter().position(|g| g.0 == id && g.1 == n) {
            Some(g) => g,
            None => {
                groups.push((id.to_string(), n, vec![None; n], 0));
                groups.len() - 1
            }
        };
        groups[g].2[i] = Some(part.to_string());
        groups[g].3 = groups[g].3.max(*time);
    }
    let mut done: Vec<(i64, String)> = groups
        .into_iter()
        .filter(|g| g.2.iter().all(Option::is_some))
        .map(|g| (g.3, g.2.into_iter().flatten().collect()))
        .collect();
    done.sort_by_key(|d| d.0);
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_round_trip() {
        let key = unb64(&new_link().key).unwrap();
        let s = seal(&key, "диета: без сахара".as_bytes());
        assert_eq!(open(&key, &s).unwrap(), "диета: без сахара".as_bytes());
        assert!(open(&random(32), &s).is_err());
    }

    #[test]
    fn code_topic_ignores_case_and_dash() {
        let c = pair_code();
        assert_eq!(c.len(), 9);
        assert_eq!(pair_topic(&c), pair_topic(&c.to_lowercase().replace('-', "")));
        assert_eq!(pair_key(&c).len(), 32);
        assert!(pair_topic(&c).starts_with("cm") && pair_topic(&c).len() == 34);
    }

    #[test]
    fn long_message_splits_and_joins() {
        let key = unb64(&new_link().key).unwrap();
        let s = seal(&key, "x".repeat(7000).as_bytes());
        let p = parts(&s);
        assert_eq!(p.len(), 4);
        let mut bodies: Vec<(i64, String)> = p.iter().enumerate().map(|(i, b)| (10 + i as i64, b.clone())).collect();
        bodies.insert(1, (5, "junk".into()));
        bodies.swap(2, 3);
        let whole = assemble(&bodies);
        assert_eq!(whole, vec![(13, s.clone())]);
        assert!(assemble(&bodies[..2]).is_empty());
    }
}
