//! Hash of text and of files, plus a guess of what type a given hash is.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct HashSet {
    pub crc32: String,
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
    pub sha512: String,
    pub bytes: u64,
}

/// CRC-32 (IEEE), the same as zip and PNG. Table built on the spot — 256
/// entries are not worth a dependency.
pub fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, e) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *e = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for b in data {
        crc = table[((crc ^ *b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

pub fn hash_all(data: &[u8]) -> HashSet {
    // md5, sha1 and sha2 share the same RustCrypto `Digest` trait.
    use md5::Digest as _;
    HashSet {
        crc32: format!("{:08x}", crc32(data)),
        md5: hex::encode(md5::Md5::digest(data)),
        sha1: hex::encode(sha1::Sha1::digest(data)),
        sha256: hex::encode(sha2::Sha256::digest(data)),
        sha512: hex::encode(sha2::Sha512::digest(data)),
        bytes: data.len() as u64,
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct HmacOptions {
    pub data: String,
    pub key: String,
    /// "sha256" | "sha512" | "sha1" | "md5"
    #[serde(default = "default_alg")]
    pub algorithm: String,
}

fn default_alg() -> String {
    "sha256".into()
}

pub fn hmac(data: &[u8], key: &[u8], algorithm: &str) -> String {
    use hmac::Mac;
    match algorithm {
        "sha1" => {
            let mut m = <hmac::Hmac<sha1::Sha1>>::new_from_slice(key).unwrap();
            m.update(data);
            hex::encode(m.finalize().into_bytes())
        }
        "md5" => {
            let mut m = <hmac::Hmac<md5::Md5>>::new_from_slice(key).unwrap();
            m.update(data);
            hex::encode(m.finalize().into_bytes())
        }
        "sha512" => {
            let mut m = <hmac::Hmac<sha2::Sha512>>::new_from_slice(key).unwrap();
            m.update(data);
            hex::encode(m.finalize().into_bytes())
        }
        _ => {
            let mut m = <hmac::Hmac<sha2::Sha256>>::new_from_slice(key).unwrap();
            m.update(data);
            hex::encode(m.finalize().into_bytes())
        }
    }
}

// ── What hash is this? ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct HashGuess {
    pub name: String,
    /// "alta" | "média" | "baixa"
    pub confidence: String,
    pub note: String,
}

/// Guesses by prefix, size and alphabet. A raw 32-hex hash is MD5 or
/// NTLM and there is no way to tell them apart — so the list comes out ordered, not unique.
pub fn identify(input: &str) -> Vec<HashGuess> {
    let s = input.trim();
    let mut out = Vec::new();
    let guess = |name: &str, confidence: &str, note: &str| HashGuess {
        name: name.into(),
        confidence: confidence.into(),
        note: note.into(),
    };

    // Formats with a prefix are unambiguous.
    for (prefix, name, note) in [
        ("$2a$", "bcrypt", "password, with embedded cost"),
        ("$2b$", "bcrypt", "password, with embedded cost"),
        ("$2y$", "bcrypt", "password, with embedded cost"),
        ("$argon2i$", "Argon2i", "password"),
        ("$argon2id$", "Argon2id", "password"),
        ("$1$", "MD5-crypt", "/etc/shadow (old)"),
        ("$5$", "SHA-256 crypt", "/etc/shadow"),
        ("$6$", "SHA-512 crypt", "/etc/shadow"),
        ("$y$", "yescrypt", "/etc/shadow (modern)"),
        ("$pbkdf2", "PBKDF2", "password"),
        ("{SSHA}", "SSHA", "LDAP"),
        ("pbkdf2_sha256$", "PBKDF2-SHA256", "Django"),
    ] {
        if s.starts_with(prefix) {
            out.push(guess(name, "alta", note));
            return out;
        }
    }

    let hex = !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit());
    if hex {
        match s.len() {
            8 => out.push(guess("CRC-32", "medium", "8 hex could also be Adler-32")),
            16 => out.push(guess("CRC-64 / MySQL antigo", "baixa", "16 hex")),
            32 => {
                out.push(guess("MD5", "medium", "32 hex"));
                out.push(guess("NTLM", "medium", "same size as MD5"));
                out.push(guess("MD4", "baixa", "same size as MD5"));
            }
            40 => {
                out.push(guess("SHA-1", "medium", "40 hex"));
                out.push(guess("RIPEMD-160", "baixa", "same size as SHA-1"));
            }
            56 => out.push(guess("SHA-224", "medium", "56 hex")),
            64 => {
                out.push(guess("SHA-256", "medium", "64 hex"));
                out.push(guess("SHA3-256 / BLAKE2s", "baixa", "same size"));
            }
            96 => out.push(guess("SHA-384", "medium", "96 hex")),
            128 => {
                out.push(guess("SHA-512", "medium", "128 hex"));
                out.push(guess("BLAKE2b / Whirlpool", "baixa", "same size"));
            }
            _ => {}
        }
    }
    if out.is_empty() {
        let b64ish = s.len() >= 16
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=');
        if b64ish {
            out.push(guess(
                "base64",
                "baixa",
                "looks like an encoding, not a hash — decode it first",
            ));
        } else {
            out.push(guess(
                "desconhecido",
                "baixa",
                "size and alphabet do not match any known format",
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors_match() {
        let h = hash_all(b"abc");
        assert_eq!(h.md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(h.sha1, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            h.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(h.crc32, "352441c2");
        assert_eq!(h.bytes, 3);
    }

    #[test]
    fn crc32_matches_the_classic_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn hmac_matches_rfc_4231() {
        // RFC 4231, case 2.
        let mac = hmac(b"what do ya want for nothing?", b"Jefe", "sha256");
        assert_eq!(
            mac,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn prefixed_formats_are_certain() {
        let g = identify("$2b$12$abcdefghijklmnopqrstuv");
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].name, "bcrypt");
        assert_eq!(g[0].confidence, "alta");
        assert_eq!(identify("$6$salt$hash")[0].name, "SHA-512 crypt");
    }

    #[test]
    fn ambiguous_lengths_return_every_candidate() {
        let g = identify("900150983cd24fb0d6963f7d28e17f72");
        let names: Vec<&str> = g.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"MD5") && names.contains(&"NTLM"));
        let g = identify("a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(g[0].name, "SHA-1");
    }

    #[test]
    fn base64_is_called_out_as_not_a_hash() {
        let g = identify("SGVsbG8gbXVuZG8gZGUgdGVzdGU=");
        assert_eq!(g[0].name, "base64");
    }

    #[test]
    fn garbage_is_unknown() {
        assert_eq!(identify("nada disso aqui!!")[0].name, "desconhecido");
    }
}
