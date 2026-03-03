//! PostgreSQL v3 Wire Protocol message definitions.
//!
//! Reference: https://www.postgresql.org/docs/current/protocol-message-formats.html

/// Frontend (client → server) message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendMessage {
    /// StartupMessage (no tag byte, identified by length + version).
    Startup,
    /// Password response ('p').
    PasswordMessage,
    /// SASL initial response ('p').
    SASLInitialResponse,
    /// SASL response ('p').
    SASLResponse,
    /// Parse ('P') — extended query protocol.
    Parse,
    /// Bind ('B').
    Bind,
    /// Describe ('D').
    Describe,
    /// Execute ('E').
    Execute,
    /// Sync ('S').
    Sync,
    /// Close ('C').
    Close,
    /// Query ('Q') — simple query protocol.
    Query,
    /// Terminate ('X').
    Terminate,
    /// Flush ('H').
    Flush,
    /// CopyData ('d').
    CopyData,
    /// CopyDone ('c').
    CopyDone,
    /// CopyFail ('f').
    CopyFail,
}

/// Backend (server → client) message tag bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BackendTag {
    AuthenticationRequest = b'R',
    ParameterStatus = b'S',
    BackendKeyData = b'K',
    ReadyForQuery = b'Z',
    RowDescription = b'T',
    DataRow = b'D',
    CommandComplete = b'C',
    ErrorResponse = b'E',
    NoticeResponse = b'N',
    ParseComplete = b'1',
    BindComplete = b'2',
    CloseComplete = b'3',
    NoData = b'n',
    ParameterDescription = b't',
    EmptyQueryResponse = b'I',
    NotificationResponse = b'A',
    CopyInResponse = b'G',
    CopyOutResponse = b'H',
    CopyDone = b'c',
    CopyData = b'd',
    NegotiateProtocolVersion = b'v',
    Unknown = 0,
}

impl From<u8> for BackendTag {
    fn from(b: u8) -> Self {
        match b {
            b'R' => BackendTag::AuthenticationRequest,
            b'S' => BackendTag::ParameterStatus,
            b'K' => BackendTag::BackendKeyData,
            b'Z' => BackendTag::ReadyForQuery,
            b'T' => BackendTag::RowDescription,
            b'D' => BackendTag::DataRow,
            b'C' => BackendTag::CommandComplete,
            b'E' => BackendTag::ErrorResponse,
            b'N' => BackendTag::NoticeResponse,
            b'1' => BackendTag::ParseComplete,
            b'2' => BackendTag::BindComplete,
            b'3' => BackendTag::CloseComplete,
            b'n' => BackendTag::NoData,
            b't' => BackendTag::ParameterDescription,
            b'I' => BackendTag::EmptyQueryResponse,
            b'A' => BackendTag::NotificationResponse,
            b'G' => BackendTag::CopyInResponse,
            b'H' => BackendTag::CopyOutResponse,
            b'c' => BackendTag::CopyDone,
            b'd' => BackendTag::CopyData,
            b'v' => BackendTag::NegotiateProtocolVersion,
            _ => BackendTag::Unknown,
        }
    }
}

/// Authentication sub-types from AuthenticationRequest messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthType {
    Ok = 0,
    CleartextPassword = 3,
    MD5Password = 5,
    SASLInit = 10,
    SASLContinue = 11,
    SASLFinal = 12,
}

impl AuthType {
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            0 => Some(AuthType::Ok),
            3 => Some(AuthType::CleartextPassword),
            5 => Some(AuthType::MD5Password),
            10 => Some(AuthType::SASLInit),
            11 => Some(AuthType::SASLContinue),
            12 => Some(AuthType::SASLFinal),
            _ => None,
        }
    }
}

/// Transaction status indicator from ReadyForQuery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    /// 'I' — Idle, not in a transaction.
    Idle,
    /// 'T' — In a transaction block.
    InTransaction,
    /// 'E' — In a failed transaction block.
    Failed,
}

impl From<u8> for TransactionStatus {
    fn from(b: u8) -> Self {
        match b {
            b'T' => TransactionStatus::InTransaction,
            b'E' => TransactionStatus::Failed,
            _ => TransactionStatus::Idle,
        }
    }
}

/// Describe target: Statement or Portal.
#[derive(Debug, Clone, Copy)]
pub enum DescribeTarget {
    Statement,
    Portal,
}

/// Close target: Statement or Portal.
#[derive(Debug, Clone, Copy)]
pub enum CloseTarget {
    Statement,
    Portal,
}

/// Column format codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatCode {
    Text = 0,
    Binary = 1,
}

impl From<i16> for FormatCode {
    fn from(v: i16) -> Self {
        if v == 1 {
            FormatCode::Binary
        } else {
            FormatCode::Text
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── BackendTag::from(u8) ──────────────────────────────────────────────────

    #[test]
    fn test_backend_tag_auth() {
        assert_eq!(BackendTag::from(b'R'), BackendTag::AuthenticationRequest);
    }

    #[test]
    fn test_backend_tag_parameter_status() {
        assert_eq!(BackendTag::from(b'S'), BackendTag::ParameterStatus);
    }

    #[test]
    fn test_backend_tag_backend_key_data() {
        assert_eq!(BackendTag::from(b'K'), BackendTag::BackendKeyData);
    }

    #[test]
    fn test_backend_tag_ready_for_query() {
        assert_eq!(BackendTag::from(b'Z'), BackendTag::ReadyForQuery);
    }

    #[test]
    fn test_backend_tag_row_description() {
        assert_eq!(BackendTag::from(b'T'), BackendTag::RowDescription);
    }

    #[test]
    fn test_backend_tag_data_row() {
        assert_eq!(BackendTag::from(b'D'), BackendTag::DataRow);
    }

    #[test]
    fn test_backend_tag_command_complete() {
        assert_eq!(BackendTag::from(b'C'), BackendTag::CommandComplete);
    }

    #[test]
    fn test_backend_tag_error_response() {
        assert_eq!(BackendTag::from(b'E'), BackendTag::ErrorResponse);
    }

    #[test]
    fn test_backend_tag_notice_response() {
        assert_eq!(BackendTag::from(b'N'), BackendTag::NoticeResponse);
    }

    #[test]
    fn test_backend_tag_parse_complete() {
        assert_eq!(BackendTag::from(b'1'), BackendTag::ParseComplete);
    }

    #[test]
    fn test_backend_tag_bind_complete() {
        assert_eq!(BackendTag::from(b'2'), BackendTag::BindComplete);
    }

    #[test]
    fn test_backend_tag_close_complete() {
        assert_eq!(BackendTag::from(b'3'), BackendTag::CloseComplete);
    }

    #[test]
    fn test_backend_tag_no_data() {
        assert_eq!(BackendTag::from(b'n'), BackendTag::NoData);
    }

    #[test]
    fn test_backend_tag_parameter_description() {
        assert_eq!(BackendTag::from(b't'), BackendTag::ParameterDescription);
    }

    #[test]
    fn test_backend_tag_empty_query() {
        assert_eq!(BackendTag::from(b'I'), BackendTag::EmptyQueryResponse);
    }

    #[test]
    fn test_backend_tag_notification() {
        assert_eq!(BackendTag::from(b'A'), BackendTag::NotificationResponse);
    }

    #[test]
    fn test_backend_tag_copy_in() {
        assert_eq!(BackendTag::from(b'G'), BackendTag::CopyInResponse);
    }

    #[test]
    fn test_backend_tag_copy_out() {
        assert_eq!(BackendTag::from(b'H'), BackendTag::CopyOutResponse);
    }

    #[test]
    fn test_backend_tag_copy_done() {
        assert_eq!(BackendTag::from(b'c'), BackendTag::CopyDone);
    }

    #[test]
    fn test_backend_tag_copy_data_lowercase() {
        assert_eq!(BackendTag::from(b'd'), BackendTag::CopyData);
    }

    #[test]
    fn test_backend_tag_negotiate_protocol() {
        assert_eq!(BackendTag::from(b'v'), BackendTag::NegotiateProtocolVersion);
    }

    #[test]
    fn test_backend_tag_unknown_byte() {
        assert_eq!(BackendTag::from(0xFF), BackendTag::Unknown);
    }

    #[test]
    fn test_backend_tag_unknown_zero() {
        assert_eq!(BackendTag::from(0x00), BackendTag::Unknown);
    }

    #[test]
    fn test_backend_tag_debug_format() {
        let tag = BackendTag::ReadyForQuery;
        let s = format!("{:?}", tag);
        assert_eq!(s, "ReadyForQuery");
    }

    #[test]
    fn test_backend_tag_equality() {
        assert_eq!(BackendTag::from(b'Z'), BackendTag::ReadyForQuery);
        assert_ne!(BackendTag::from(b'Z'), BackendTag::ErrorResponse);
    }

    #[test]
    fn test_backend_tag_repr_values() {
        // Verify the repr(u8) byte values match the protocol spec
        assert_eq!(BackendTag::AuthenticationRequest as u8, b'R');
        assert_eq!(BackendTag::ReadyForQuery as u8, b'Z');
        assert_eq!(BackendTag::ErrorResponse as u8, b'E');
        assert_eq!(BackendTag::DataRow as u8, b'D');
        assert_eq!(BackendTag::CommandComplete as u8, b'C');
    }

    // ─── TransactionStatus::from(u8) ─────────────────────────────────────────

    #[test]
    fn test_tx_status_idle_from_i() {
        assert_eq!(TransactionStatus::from(b'I'), TransactionStatus::Idle);
    }

    #[test]
    fn test_tx_status_in_transaction() {
        assert_eq!(
            TransactionStatus::from(b'T'),
            TransactionStatus::InTransaction
        );
    }

    #[test]
    fn test_tx_status_failed() {
        assert_eq!(TransactionStatus::from(b'E'), TransactionStatus::Failed);
    }

    #[test]
    fn test_tx_status_unknown_defaults_idle() {
        // Any unrecognized byte → Idle (safe default)
        assert_eq!(TransactionStatus::from(0xFF), TransactionStatus::Idle);
        assert_eq!(TransactionStatus::from(b'X'), TransactionStatus::Idle);
    }

    #[test]
    fn test_tx_status_debug() {
        assert_eq!(format!("{:?}", TransactionStatus::Idle), "Idle");
        assert_eq!(
            format!("{:?}", TransactionStatus::InTransaction),
            "InTransaction"
        );
        assert_eq!(format!("{:?}", TransactionStatus::Failed), "Failed");
    }

    #[test]
    fn test_tx_status_eq() {
        assert_eq!(TransactionStatus::Idle, TransactionStatus::Idle);
        assert_ne!(TransactionStatus::Idle, TransactionStatus::InTransaction);
    }

    // ─── AuthType::from_i32 ──────────────────────────────────────────────────

    #[test]
    fn test_auth_type_ok() {
        assert_eq!(AuthType::from_i32(0), Some(AuthType::Ok));
    }

    #[test]
    fn test_auth_type_cleartext() {
        assert_eq!(AuthType::from_i32(3), Some(AuthType::CleartextPassword));
    }

    #[test]
    fn test_auth_type_md5() {
        assert_eq!(AuthType::from_i32(5), Some(AuthType::MD5Password));
    }

    #[test]
    fn test_auth_type_sasl_init() {
        assert_eq!(AuthType::from_i32(10), Some(AuthType::SASLInit));
    }

    #[test]
    fn test_auth_type_sasl_continue() {
        assert_eq!(AuthType::from_i32(11), Some(AuthType::SASLContinue));
    }

    #[test]
    fn test_auth_type_sasl_final() {
        assert_eq!(AuthType::from_i32(12), Some(AuthType::SASLFinal));
    }

    #[test]
    fn test_auth_type_unknown_returns_none() {
        assert_eq!(AuthType::from_i32(1), None);
        assert_eq!(AuthType::from_i32(99), None);
        assert_eq!(AuthType::from_i32(-1), None);
    }

    // ─── FormatCode::from(i16) ───────────────────────────────────────────────

    #[test]
    fn test_format_code_text_from_zero() {
        assert_eq!(FormatCode::from(0i16), FormatCode::Text);
    }

    #[test]
    fn test_format_code_binary_from_one() {
        assert_eq!(FormatCode::from(1i16), FormatCode::Binary);
    }

    #[test]
    fn test_format_code_unknown_defaults_text() {
        // Anything other than 1 → Text (safe default)
        assert_eq!(FormatCode::from(2i16), FormatCode::Text);
        assert_eq!(FormatCode::from(-1i16), FormatCode::Text);
        assert_eq!(FormatCode::from(99i16), FormatCode::Text);
    }

    #[test]
    fn test_format_code_values() {
        assert_eq!(FormatCode::Text as i16, 0);
        assert_eq!(FormatCode::Binary as i16, 1);
    }

    #[test]
    fn test_format_code_eq() {
        assert_eq!(FormatCode::Text, FormatCode::Text);
        assert_ne!(FormatCode::Text, FormatCode::Binary);
    }

    // ─── Roundtrip: from → tag byte → from ───────────────────────────────────

    #[test]
    fn test_all_known_backend_tags_roundtrip() {
        let pairs: &[(u8, BackendTag)] = &[
            (b'R', BackendTag::AuthenticationRequest),
            (b'S', BackendTag::ParameterStatus),
            (b'K', BackendTag::BackendKeyData),
            (b'Z', BackendTag::ReadyForQuery),
            (b'T', BackendTag::RowDescription),
            (b'D', BackendTag::DataRow),
            (b'C', BackendTag::CommandComplete),
            (b'E', BackendTag::ErrorResponse),
            (b'N', BackendTag::NoticeResponse),
            (b'1', BackendTag::ParseComplete),
            (b'2', BackendTag::BindComplete),
            (b'3', BackendTag::CloseComplete),
            (b'n', BackendTag::NoData),
            (b't', BackendTag::ParameterDescription),
            (b'I', BackendTag::EmptyQueryResponse),
            (b'A', BackendTag::NotificationResponse),
            (b'G', BackendTag::CopyInResponse),
            (b'H', BackendTag::CopyOutResponse),
            (b'c', BackendTag::CopyDone),
            (b'd', BackendTag::CopyData),
            (b'v', BackendTag::NegotiateProtocolVersion),
        ];
        for &(byte, ref expected) in pairs {
            let got = BackendTag::from(byte);
            assert_eq!(
                &got, expected,
                "byte {:#04x} should map to {:?}",
                byte, expected
            );
        }
    }
}
