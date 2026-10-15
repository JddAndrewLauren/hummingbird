//! The one base64 codec this crate owns, lifted out of `fcm.rs` (#837) when
//! the Google sign-in verifier became its second consumer: FCM's assertion
//! *encodes* base64url and decodes a PEM body's standard base64; the
//! sign-in verifier *decodes* a JWT's three base64url segments and a JWK's
//! `n`/`e`. Hand-rolled rather than pulled in: it is a few dozen lines, and
//! `authority`'s dependency list is deliberately short enough to audit at a
//! glance (`lib.rs`'s guard test) — and the wasm32 worker build may gain no
//! parser crate (CLAUDE.md, "The wasm32 worker build stays thin").

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url, unpadded — JWT's encoding (RFC 7515 §2).
pub fn encode_url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = |i: usize| *chunk.get(i).unwrap_or(&0) as u32;
        let n = (b(0) << 16) | (b(1) << 8) | b(2);
        let digits = [(n >> 18) & 63, (n >> 12) & 63, (n >> 6) & 63, n & 63];
        // 3 input bytes -> 4 digits, 2 -> 3, 1 -> 2; the rest would be
        // padding, which base64url omits.
        for digit in digits.iter().take(chunk.len() + 1) {
            out.push(URL_SAFE[*digit as usize] as char);
        }
    }
    out
}

/// Standard base64 decode, tolerant of the whitespace and line breaks a PEM
/// block carries and of padding being present or absent.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let filtered = text
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=');
    decode_digits(filtered, STANDARD)
}

/// base64url decode, strict: the URL-safe alphabet only, no padding, no
/// whitespace — exactly what a JWT segment or a JWK member is. Anything
/// else is not a well-formed token and must not be read leniently.
pub fn decode_url(text: &str) -> Option<Vec<u8>> {
    // A single leftover digit carries six bits, which can never complete a
    // byte: no encoder produces a length of 4k + 1.
    if text.len() % 4 == 1 {
        return None;
    }
    decode_digits(text.bytes(), URL_SAFE)
}

fn decode_digits(digits: impl Iterator<Item = u8>, alphabet: &[u8; 64]) -> Option<Vec<u8>> {
    let mut acc: u32 = 0;
    let mut bits = 0;
    let mut out = Vec::new();
    for byte in digits {
        let value = alphabet.iter().position(|c| *c == byte)? as u32;
        acc = ((acc << 6) | value) & 0xffff;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    // Leftover bits are the encoder's zero padding; anything set in them
    // means the input was not valid base64.
    if acc & ((1 << bits) - 1) != 0 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_url_encodes_without_padding() {
        assert_eq!(encode_url(b""), "");
        assert_eq!(encode_url(b"f"), "Zg");
        assert_eq!(encode_url(b"fo"), "Zm8");
        assert_eq!(encode_url(b"foo"), "Zm9v");
        assert_eq!(encode_url(b"foob"), "Zm9vYg");
        assert_eq!(encode_url(b"fooba"), "Zm9vYmE");
        assert_eq!(encode_url(b"foobar"), "Zm9vYmFy");
    }

    /// The two alphabets differ only in the last two digits, which is
    /// exactly where a mix-up would hide: `>>?` encodes to `Pj4/` in
    /// standard base64 and must encode to `Pj4_` here.
    #[test]
    fn encode_url_uses_the_url_safe_alphabet() {
        assert_eq!(encode_url(&[0x3e, 0x3e, 0x3f]), "Pj4_");
        assert_eq!(encode_url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn decode_round_trips_and_tolerates_pem_whitespace() {
        assert_eq!(decode("Zm9vYmFy").as_deref(), Some(&b"foobar"[..]));
        assert_eq!(decode("Zm9v\nYmFy\n").as_deref(), Some(&b"foobar"[..]));
        assert_eq!(decode("Zg==").as_deref(), Some(&b"f"[..]));
        assert_eq!(decode("Zg").as_deref(), Some(&b"f"[..]));
        assert_eq!(decode("not base64!"), None);
    }

    #[test]
    fn decode_url_round_trips_its_own_encoder() {
        for len in 0..=40u8 {
            let bytes: Vec<u8> = (0..len)
                .map(|i| i.wrapping_mul(37).wrapping_add(0xf0))
                .collect();
            assert_eq!(decode_url(&encode_url(&bytes)), Some(bytes), "length {len}");
        }
    }

    /// Strict where `decode` is lenient: a JWT segment with padding, a
    /// standard-alphabet digit, whitespace or an impossible length is not a
    /// JWT segment at all.
    #[test]
    fn decode_url_refuses_what_a_jwt_segment_cannot_contain() {
        assert_eq!(decode_url("Pj4_").as_deref(), Some(&[0x3e, 0x3e, 0x3f][..]));
        assert_eq!(decode_url("Pj4/"), None, "standard alphabet");
        assert_eq!(decode_url("Zg=="), None, "padding");
        assert_eq!(decode_url("Zm9v YmFy"), None, "whitespace");
        assert_eq!(decode_url("Zm9vY"), None, "4k + 1 digits");
        assert_eq!(decode_url("Zh"), None, "non-zero trailing bits");
    }
}
