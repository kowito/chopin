// src/oauth.rs
//
//! OAuth 2.0 / PKCE helpers for the authorization-code flow (RFC 7636).
//!
//! Provides:
//! - PKCE code-verifier generation (`code_verifier()`)
//! - S256 code-challenge computation (`code_challenge_s256()`)
//! - Authorization URL builder (`AuthorizationUrl`)
//! - Token-pair issuance helper (`token_pair()`)

use crate::jwt::{AuthError, JwtManager};
use serde::Serialize;

// ─── PKCE ────────────────────────────────────────────────────────────────────

/// Generate a cryptographically random code-verifier string (43–128 chars, RFC 7636 §4.1).
///
/// Uses the OS CSPRNG (`/dev/urandom` on Unix).
pub fn code_verifier() -> String {
    let mut buf = [0u8; 32];
    getrandom(&mut buf);
    base64url_encode(&buf)
}

/// Compute the S256 code-challenge for a given verifier (RFC 7636 §4.2).
///
/// `challenge = BASE64URL(SHA-256(verifier))`
pub fn code_challenge_s256(verifier: &str) -> String {
    let hash = sha256(verifier.as_bytes());
    base64url_encode(&hash)
}

/// Minimal SHA-256 (FIPS 180-4) — zero external dependencies.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Pre-processing: pad message
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 512-bit block
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&val.to_be_bytes());
    }
    out
}

/// Fill buffer from OS CSPRNG.
fn getrandom(buf: &mut [u8]) {
    use std::io::Read;
    let mut f = std::fs::File::open("/dev/urandom").expect("cannot open /dev/urandom");
    f.read_exact(buf).expect("cannot read /dev/urandom");
}

/// Base64url-encode (no padding) per RFC 4648 §5.
fn base64url_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((data.len() * 4 + 2) / 3);
    let mut i = 0;
    while i + 2 < data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
        out.push(CHARS[((n >> 6) & 63) as usize] as char);
        out.push(CHARS[(n & 63) as usize] as char);
        i += 3;
    }
    let rem = data.len() - i;
    if rem == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
        out.push(CHARS[((n >> 6) & 63) as usize] as char);
    } else if rem == 1 {
        let n = (data[i] as u32) << 16;
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
    }
    out
}

// ─── Authorization URL Builder ───────────────────────────────────────────────

/// Builder for constructing an OAuth 2.0 authorization URL with PKCE.
pub struct AuthorizationUrl {
    authorize_endpoint: String,
    client_id: String,
    redirect_uri: String,
    scopes: Vec<String>,
    state: Option<String>,
    code_challenge: Option<String>,
}

impl AuthorizationUrl {
    pub fn new(
        authorize_endpoint: impl Into<String>,
        client_id: impl Into<String>,
        redirect_uri: impl Into<String>,
    ) -> Self {
        Self {
            authorize_endpoint: authorize_endpoint.into(),
            client_id: client_id.into(),
            redirect_uri: redirect_uri.into(),
            scopes: Vec::new(),
            state: None,
            code_challenge: None,
        }
    }

    pub fn scope(mut self, scope: impl Into<String>) -> Self {
        self.scopes.push(scope.into());
        self
    }

    pub fn state(mut self, state: impl Into<String>) -> Self {
        self.state = Some(state.into());
        self
    }

    /// Attach a PKCE code-challenge (S256).
    pub fn code_challenge(mut self, challenge: impl Into<String>) -> Self {
        self.code_challenge = Some(challenge.into());
        self
    }

    /// Build the full authorization URL with query parameters.
    pub fn build(self) -> String {
        let mut url = format!(
            "{}?response_type=code&client_id={}&redirect_uri={}",
            self.authorize_endpoint,
            percent_encode(&self.client_id),
            percent_encode(&self.redirect_uri),
        );

        if !self.scopes.is_empty() {
            url.push_str("&scope=");
            url.push_str(&percent_encode(&self.scopes.join(" ")));
        }

        if let Some(state) = &self.state {
            url.push_str("&state=");
            url.push_str(&percent_encode(state));
        }

        if let Some(challenge) = &self.code_challenge {
            url.push_str("&code_challenge=");
            url.push_str(&percent_encode(challenge));
            url.push_str("&code_challenge_method=S256");
        }

        url
    }
}

/// Minimal percent-encoding for query parameter values.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
                out.push(char::from(b"0123456789ABCDEF"[(b & 0x0f) as usize]));
            }
        }
    }
    out
}

// ─── Token Pair ──────────────────────────────────────────────────────────────

/// A pair of access + refresh tokens.
#[derive(Debug, Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    pub expires_in: u64,
}

/// Issue an access/refresh token pair from the same `JwtManager`.
///
/// `access_claims` and `refresh_claims` should differ in `exp` (and optionally
/// scope/audience). The `expires_in` value in the returned [`TokenPair`] is
/// the access-token TTL in seconds.
pub fn token_pair<A: Serialize, R: Serialize>(
    manager: &JwtManager,
    access_claims: &A,
    refresh_claims: &R,
    access_ttl_secs: u64,
) -> Result<TokenPair, AuthError> {
    let access_token = manager.encode(access_claims)?;
    let refresh_token = manager.encode(refresh_claims)?;
    Ok(TokenPair {
        access_token,
        refresh_token,
        token_type: "Bearer",
        expires_in: access_ttl_secs,
    })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_empty() {
        let hash = sha256(b"");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_abc() {
        let hash = sha256(b"abc");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_base64url_encode() {
        // RFC 4648 test vectors (without padding)
        assert_eq!(base64url_encode(b""), "");
        assert_eq!(base64url_encode(b"f"), "Zg");
        assert_eq!(base64url_encode(b"fo"), "Zm8");
        assert_eq!(base64url_encode(b"foo"), "Zm9v");
        assert_eq!(base64url_encode(b"foob"), "Zm9vYg");
        assert_eq!(base64url_encode(b"fooba"), "Zm9vYmE");
        assert_eq!(base64url_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_code_verifier_length() {
        let v = code_verifier();
        // 32 random bytes → 43 base64url chars
        assert_eq!(v.len(), 43);
        // All characters are valid base64url (no padding)
        assert!(v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn test_code_verifier_unique() {
        let v1 = code_verifier();
        let v2 = code_verifier();
        assert_ne!(v1, v2, "two verifiers should be distinct");
    }

    #[test]
    fn test_code_challenge_s256() {
        // Cross-check: known verifier should produce known challenge.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = code_challenge_s256(verifier);
        // Verified against RFC 7636 Appendix B
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn test_authorization_url_minimal() {
        let url = AuthorizationUrl::new(
            "https://auth.example.com/authorize",
            "my-client",
            "http://localhost:3000/callback",
        )
        .build();

        assert!(url.starts_with("https://auth.example.com/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=my-client"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A3000%2Fcallback"));
    }

    #[test]
    fn test_authorization_url_with_pkce() {
        let verifier = code_verifier();
        let challenge = code_challenge_s256(&verifier);

        let url = AuthorizationUrl::new(
            "https://auth.example.com/authorize",
            "client",
            "http://localhost/cb",
        )
        .scope("openid")
        .scope("profile")
        .state("random-state")
        .code_challenge(&challenge)
        .build();

        assert!(url.contains("scope=openid%20profile"));
        assert!(url.contains("state=random-state"));
        assert!(url.contains(&format!("code_challenge={}", challenge)));
        assert!(url.contains("code_challenge_method=S256"));
    }

    #[test]
    fn test_token_pair() {
        let mgr = JwtManager::new(b"test-secret");

        #[derive(Serialize)]
        struct Access {
            sub: String,
            exp: u64,
            scope: String,
        }
        #[derive(Serialize)]
        struct Refresh {
            sub: String,
            exp: u64,
            jti: String,
        }

        let pair = token_pair(
            &mgr,
            &Access {
                sub: "user-1".into(),
                exp: 253_370_764_800,
                scope: "read write".into(),
            },
            &Refresh {
                sub: "user-1".into(),
                exp: 253_370_764_800,
                jti: "refresh-jti-1".into(),
            },
            3600,
        )
        .unwrap();

        assert!(!pair.access_token.is_empty());
        assert!(!pair.refresh_token.is_empty());
        assert_ne!(pair.access_token, pair.refresh_token);
        assert_eq!(pair.token_type, "Bearer");
        assert_eq!(pair.expires_in, 3600);
    }

    #[test]
    fn test_percent_encode() {
        assert_eq!(percent_encode("hello"), "hello");
        assert_eq!(percent_encode("hello world"), "hello%20world");
        assert_eq!(
            percent_encode("http://localhost:3000/cb"),
            "http%3A%2F%2Flocalhost%3A3000%2Fcb"
        );
    }
}
