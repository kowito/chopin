//! Zero-overhead JWT authentication, RBAC middleware, and password hashing
//! for the Chopin web framework.
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use chopin_auth::{
//!     HasJti, JwtManager, PasswordHasher, TokenBlacklist, init_jwt_manager, Auth,
//! };
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! struct Claims {
//!     sub: String,
//!     jti: String,
//!     exp: u64,
//! }
//!
//! impl HasJti for Claims {
//!     fn jti(&self) -> Option<&str> { Some(&self.jti) }
//! }
//!
//! // Once at server startup:
//! let blacklist = TokenBlacklist::new();
//! let manager = JwtManager::new(b"my-secret").with_blacklist(blacklist.clone());
//! init_jwt_manager(manager);
//!
//! // In a route handler, use `Auth<Claims>` as an extractor:
//! // async fn my_handler(auth: Auth<Claims>) -> Response { ... }
//!
//! // Hash a password:
//! let hash = PasswordHasher::interactive().hash(b"p4ssw0rd")?;
//!
//! // Revoke a token (e.g. on logout):
//! // blacklist.revoke(claims.jti.clone(), Some(claims.exp));
//! ```
pub mod crypto;
pub mod extractor;
pub mod jwks;
pub mod jwt;
pub mod middleware;
pub mod oauth;
pub mod revocation;

pub use crypto::{PasswordHasher, hash_password, verify_password};
pub use extractor::{Auth, ErrorHandler, init_jwt_manager, set_error_handler};
pub use jwks::JwksProvider;
pub use jwt::{AuthError, HasJti, JwtConfig, JwtManager};
pub use middleware::{Role, RoleCheck, ScopeCheck};
pub use oauth::{AuthorizationUrl, TokenPair, code_challenge_s256, code_verifier, token_pair};
pub use revocation::TokenBlacklist;
