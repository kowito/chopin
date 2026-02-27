//! SCRAM-SHA-256 authentication for PostgreSQL.
//!
//! Implements RFC 5802 / RFC 7677 SCRAM mechanism without external dependencies.
//! Uses raw SHA-256 and HMAC implementations to stay zero-dependency.

/// SCRAM-SHA-256 client state machine.
pub struct ScramClient {
    username: String,
    password: String,
    nonce: String,
    client_first_bare: String,
    server_nonce: String,
    salt: Vec<u8>,
    iterations: u32,
    auth_message: String,
    salted_password: [u8; 32],
}

impl ScramClient {
    pub fn new(username: &str, password: &str) -> Self {
        let nonce = generate_nonce();
        Self {
            username: username.to_string(),
            password: password.to_string(),
            nonce,
            client_first_bare: String::new(),
            server_nonce: String::new(),
            salt: Vec::new(),
            iterations: 0,
            auth_message: String::new(),
            salted_password: [0u8; 32],
        }
    }

    /// Build the client-first-message to send in SASLInitialResponse.
    pub fn client_first_message(&mut self) -> Vec<u8> {
        self.client_first_bare = format!("n={},r={}", self.username, self.nonce);
        let msg = format!("n,,{}", self.client_first_bare);
        msg.into_bytes()
    }

    /// Process the server-first-message and produce the client-final-message.
    pub fn process_server_first(&mut self, server_first: &[u8]) -> Result<Vec<u8>, String> {
        let server_first_str = std::str::from_utf8(server_first)
            .map_err(|_| "Invalid UTF-8 in server-first-message".to_string())?;

        // Parse r=<nonce>, s=<salt>, i=<iterations>
        let mut server_nonce = "";
        let mut salt_b64 = "";
        let mut iterations = 0u32;

        for part in server_first_str.split(',') {
            if let Some(val) = part.strip_prefix("r=") {
                server_nonce = val;
            } else if let Some(val) = part.strip_prefix("s=") {
                salt_b64 = val;
            } else if let Some(val) = part.strip_prefix("i=") {
                iterations = val.parse().map_err(|_| "Invalid iteration count".to_string())?;
            }
        }

        if !server_nonce.starts_with(&self.nonce) {
            return Err("Server nonce doesn't start with client nonce".to_string());
        }

        self.server_nonce = server_nonce.to_string();
        self.salt = base64_decode(salt_b64)?;
        self.iterations = iterations;

        // Derive salted password
        self.salted_password = hi(self.password.as_bytes(), &self.salt, self.iterations);

        // Build client-final-message-without-proof
        let client_final_without_proof = format!("c=biws,r={}", self.server_nonce);

        // Auth message
        self.auth_message = format!(
            "{},{},{}",
            self.client_first_bare, server_first_str, client_final_without_proof
        );

        // Client key
        let client_key = hmac_sha256(&self.salted_password, b"Client Key");
        let stored_key = sha256(&client_key);
        let client_signature = hmac_sha256(&stored_key, self.auth_message.as_bytes());

        // Client proof = client_key XOR client_signature
        let mut client_proof = [0u8; 32];
        for i in 0..32 {
            client_proof[i] = client_key[i] ^ client_signature[i];
        }

        let proof_b64 = base64_encode(&client_proof);
        let client_final = format!("{},p={}", client_final_without_proof, proof_b64);

        Ok(client_final.into_bytes())
    }

    /// Verify the server-final-message (optional but recommended).
    pub fn verify_server_final(&self, server_final: &[u8]) -> Result<(), String> {
        let server_final_str = std::str::from_utf8(server_final)
            .map_err(|_| "Invalid UTF-8 in server-final".to_string())?;

        let verifier_b64 = server_final_str
            .strip_prefix("v=")
            .ok_or_else(|| "Missing v= in server-final".to_string())?;

        let server_signature_received = base64_decode(verifier_b64)?;

        // Compute expected server signature
        let server_key = hmac_sha256(&self.salted_password, b"Server Key");
        let expected = hmac_sha256(&server_key, self.auth_message.as_bytes());

        if server_signature_received == expected {
            Ok(())
        } else {
            Err("Server signature mismatch".to_string())
        }
    }
}

// ─── Cryptographic Primitives (no external deps) ──────────────

/// SHA-256 implementation (FIPS 180-4).
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    // Pre-processing: pad message
    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 512-bit (64-byte) chunk
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = h[0]; let mut b = h[1]; let mut c = h[2]; let mut d = h[3];
        let mut e = h[4]; let mut f = h[5]; let mut g = h[6]; let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g; g = f; f = e;
            e = d.wrapping_add(temp1);
            d = c; c = b; b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e); h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g); h[7] = h[7].wrapping_add(hh);
    }

    let mut result = [0u8; 32];
    for i in 0..8 {
        result[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    result
}

/// HMAC-SHA-256 (RFC 2104).
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let block_size = 64;
    let mut k = [0u8; 64];

    if key.len() > block_size {
        let hash = sha256(key);
        k[..32].copy_from_slice(&hash);
    } else {
        k[..key.len()].copy_from_slice(key);
    }

    // Inner padding
    let mut ipad = [0x36u8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
    }

    // Outer padding
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        opad[i] ^= k[i];
    }

    // inner hash = SHA256(ipad || message)
    let mut inner = Vec::with_capacity(64 + message.len());
    inner.extend_from_slice(&ipad);
    inner.extend_from_slice(message);
    let inner_hash = sha256(&inner);

    // outer hash = SHA256(opad || inner_hash)
    let mut outer = Vec::with_capacity(64 + 32);
    outer.extend_from_slice(&opad);
    outer.extend_from_slice(&inner_hash);
    sha256(&outer)
}

/// PBKDF2-HMAC-SHA256 (Hi function per RFC 5802).
fn hi(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    // U1 = HMAC(password, salt || INT(1))
    let mut salt_1 = salt.to_vec();
    salt_1.extend_from_slice(&1u32.to_be_bytes());

    let mut u = hmac_sha256(password, &salt_1);
    let mut result = u;

    for _ in 1..iterations {
        u = hmac_sha256(password, &u);
        for j in 0..32 {
            result[j] ^= u[j];
        }
    }
    result
}

// ─── Base64 (minimal implementation) ──────────────────────────

const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;

        result.push(B64_CHARS[((n >> 18) & 0x3F) as usize] as char);
        result.push(B64_CHARS[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(B64_CHARS[((n >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(B64_CHARS[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

pub fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let input = input.trim_end_matches('=');
    let mut result = Vec::with_capacity(input.len() * 3 / 4);

    let chars: Vec<u8> = input
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => 255,
        })
        .collect();

    for chunk in chars.chunks(4) {
        let n = match chunk.len() {
            4 => ((chunk[0] as u32) << 18) | ((chunk[1] as u32) << 12) | ((chunk[2] as u32) << 6) | (chunk[3] as u32),
            3 => ((chunk[0] as u32) << 18) | ((chunk[1] as u32) << 12) | ((chunk[2] as u32) << 6),
            2 => ((chunk[0] as u32) << 18) | ((chunk[1] as u32) << 12),
            _ => return Err("Invalid base64 chunk".to_string()),
        };

        result.push(((n >> 16) & 0xFF) as u8);
        if chunk.len() > 2 {
            result.push(((n >> 8) & 0xFF) as u8);
        }
        if chunk.len() > 3 {
            result.push((n & 0xFF) as u8);
        }
    }
    Ok(result)
}

/// Generate a random nonce string.
fn generate_nonce() -> String {
    // Use /dev/urandom for cryptographic randomness
    let mut buf = [0u8; 18];
    #[cfg(unix)]
    {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            let _ = f.read_exact(&mut buf);
        }
    }
    base64_encode(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256() {
        let hash = sha256(b"");
        let expected = [
            0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14,
            0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9, 0x24,
            0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c,
            0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55,
        ];
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_base64_roundtrip() {
        let data = b"hello world";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_hmac_sha256() {
        // RFC 4231 Test Case 2
        let key = b"Jefe";
        let data = b"what do ya want for nothing?";
        let result = hmac_sha256(key, data);
        let expected: [u8; 32] = [
            0x5b, 0xdc, 0xc1, 0x46, 0xbf, 0x60, 0x75, 0x4e,
            0x6a, 0x04, 0x24, 0x26, 0x08, 0x95, 0x75, 0xc7,
            0x5a, 0x00, 0x3f, 0x08, 0x9d, 0x27, 0x39, 0x83,
            0x9d, 0xec, 0x58, 0xb9, 0x64, 0xec, 0x38, 0x43,
        ];
        assert_eq!(result, expected);
    }
}
