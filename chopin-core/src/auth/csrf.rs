use crate::auth::totp::generate_secure_token;

/// Generate a CSRF token that should be stored in the session / cookie
/// and verified on state-changing requests.
pub fn generate_csrf_token() -> String {
    generate_secure_token()
}

/// Verify a CSRF token from the request header matches the session token.
pub fn verify_csrf_token(session_token: &str, request_token: &str) -> bool {
    // Constant-time comparison to prevent timing attacks
    if session_token.len() != request_token.len() {
        return false;
    }
    session_token
        .bytes()
        .zip(request_token.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}
