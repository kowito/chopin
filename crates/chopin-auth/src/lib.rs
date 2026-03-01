// src/lib.rs
pub mod crypto;
pub mod extractor;
pub mod jwt;
pub mod middleware;
pub mod revocation;

pub use extractor::Auth;
pub use jwt::{JwtConfig, JwtManager};
pub use middleware::Role;
