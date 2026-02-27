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
        if v == 1 { FormatCode::Binary } else { FormatCode::Text }
    }
}
