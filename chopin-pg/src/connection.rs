//! Blocking PgConnection — connects, authenticates, and queries PostgreSQL.
//!
//! This is a synchronous (blocking) implementation suitable for use within
//! worker threads that have their own event loops. It uses standard TCP sockets
//! and can be made non-blocking by integrating with the worker's kqueue/epoll.

use std::io::{Read, Write};
use std::net::TcpStream;

use crate::codec;
use crate::protocol::*;
use crate::auth::ScramClient;
use crate::error::{PgError, PgResult};
use crate::row::Row;
use crate::statement::StatementCache;

/// Connection configuration.
#[derive(Debug, Clone)]
pub struct PgConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

impl PgConfig {
    pub fn new(host: &str, port: u16, user: &str, password: &str, database: &str) -> Self {
        Self {
            host: host.to_string(),
            port,
            user: user.to_string(),
            password: password.to_string(),
            database: database.to_string(),
        }
    }

    /// Parse from a connection string: `postgres://user:pass@host:port/db`
    pub fn from_url(url: &str) -> PgResult<Self> {
        let url = url.strip_prefix("postgres://").or_else(|| url.strip_prefix("postgresql://"))
            .ok_or_else(|| PgError::Protocol("Invalid URL scheme".to_string()))?;

        // user:pass@host:port/db
        let (userpass, hostdb) = url.split_once('@')
            .ok_or_else(|| PgError::Protocol("Missing @ in URL".to_string()))?;
        let (user, password) = userpass.split_once(':').unwrap_or((userpass, ""));
        let (hostport, database) = hostdb.split_once('/')
            .ok_or_else(|| PgError::Protocol("Missing database in URL".to_string()))?;
        let (host, port_str) = hostport.split_once(':').unwrap_or((hostport, "5432"));
        let port: u16 = port_str.parse().map_err(|_| PgError::Protocol("Invalid port".to_string()))?;

        Ok(Self {
            host: host.to_string(),
            port,
            user: user.to_string(),
            password: password.to_string(),
            database: database.to_string(),
        })
    }
}

/// A synchronous PostgreSQL connection with implicit statement caching.
pub struct PgConnection {
    stream: TcpStream,
    read_buf: Vec<u8>,
    write_buf: Vec<u8>,
    read_pos: usize,
    tx_status: TransactionStatus,
    stmt_cache: StatementCache,
    process_id: i32,
    secret_key: i32,
    server_params: Vec<(String, String)>,
}

impl PgConnection {
    /// Connect to PostgreSQL and complete authentication.
    pub fn connect(config: &PgConfig) -> PgResult<Self> {
        let addr = format!("{}:{}", config.host, config.port);
        let stream = TcpStream::connect(&addr).map_err(PgError::Io)?;

        let mut conn = Self {
            stream,
            read_buf: vec![0u8; 64 * 1024], // 64 KB read buffer
            write_buf: vec![0u8; 64 * 1024], // 64 KB write buffer
            read_pos: 0,
            tx_status: TransactionStatus::Idle,
            stmt_cache: StatementCache::new(),
            process_id: 0,
            secret_key: 0,
            server_params: Vec::new(),
        };

        conn.startup(config)?;
        Ok(conn)
    }

    /// Perform the startup and authentication handshake.
    fn startup(&mut self, config: &PgConfig) -> PgResult<()> {
        // Send StartupMessage
        let n = codec::encode_startup(&mut self.write_buf, &config.user, &config.database, &[]);
        self.stream.write_all(&self.write_buf[..n]).map_err(PgError::Io)?;

        // Read server response
        loop {
            if self.read_pos == 0 || codec::message_complete(&self.read_buf[..self.read_pos]).is_none() {
                self.fill_read_buf()?;
            }

            while let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos]) {
                let header = codec::decode_header(&self.read_buf).unwrap();
                let body = &self.read_buf[5..msg_len];

                match header.tag {
                    BackendTag::AuthenticationRequest => {
                        let auth_type = codec::read_i32(&self.read_buf, 5);
                        match AuthType::from_i32(auth_type) {
                            Some(AuthType::Ok) => {
                                // Handled! Keep going to ReadyForQuery
                            }
                            Some(AuthType::CleartextPassword) => {
                                let n = codec::encode_password(&mut self.write_buf, &config.password);
                                self.stream.write_all(&self.write_buf[..n]).map_err(PgError::Io)?;
                            }
                            Some(AuthType::SASLInit) => {
                                let mut scram = ScramClient::new(&config.user, &config.password);
                                let client_first = scram.client_first_message();
                                let n = codec::encode_sasl_initial(
                                    &mut self.write_buf,
                                    "SCRAM-SHA-256",
                                    &client_first,
                                );
                                self.stream.write_all(&self.write_buf[..n]).map_err(PgError::Io)?;

                                self.consume_read(msg_len);
                                self.wait_for_sasl_continue(&mut scram, config)?;
                                // After SASL, we might still have messages in the buffer
                                // so we don't return, we continue the outer loop.
                                continue;
                            }
                            Some(AuthType::MD5Password) => {
                                return Err(PgError::Auth("MD5 auth not fully implemented, use SCRAM-SHA-256".to_string()));
                            }
                            _ => {
                                return Err(PgError::Auth(format!("Unsupported auth type: {}", auth_type)));
                            }
                        }
                    }
                    BackendTag::ParameterStatus => {
                        let (name, consumed) = codec::read_cstring(body, 0);
                        let (value, _) = codec::read_cstring(body, consumed);
                        self.server_params.push((name.to_string(), value.to_string()));
                    }
                    BackendTag::BackendKeyData => {
                        self.process_id = codec::read_i32(body, 0);
                        self.secret_key = codec::read_i32(body, 4);
                    }
                    BackendTag::ReadyForQuery => {
                        self.tx_status = TransactionStatus::from(body[0]);
                        self.consume_read(msg_len);
                        return Ok(()); // Connection is ready!
                    }
                    BackendTag::ErrorResponse => {
                        let fields = codec::parse_error_fields(body);
                        let mut severity = String::new();
                        let mut code = String::new();
                        let mut message = String::new();
                        for (field_type, value) in &fields {
                            match field_type {
                                b'S' => severity = value.clone(),
                                b'C' => code = value.clone(),
                                b'M' => message = value.clone(),
                                _ => {}
                            }
                        }
                        return Err(PgError::Server { severity, code, message });
                    }
                    _ => {
                        // Skip unknown messages
                    }
                }
                self.consume_read(msg_len);
            }
        }
    }

    /// Handle SASL Continue/Final exchange.
    fn wait_for_sasl_continue(&mut self, scram: &mut ScramClient, _config: &PgConfig) -> PgResult<()> {
        loop {
            if self.read_pos == 0 || codec::message_complete(&self.read_buf[..self.read_pos]).is_none() {
                self.fill_read_buf()?;
            }

            while let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos]) {
                let header = codec::decode_header(&self.read_buf).unwrap();
                let body = &self.read_buf[5..msg_len].to_vec();

                match header.tag {
                    BackendTag::AuthenticationRequest => {
                        let auth_type = codec::read_i32(&self.read_buf, 5);
                        match AuthType::from_i32(auth_type) {
                            Some(AuthType::SASLContinue) => {
                                let server_first = &body[4..];
                                let client_final = scram.process_server_first(server_first)
                                    .map_err(|e| PgError::Auth(e))?;
                                
                                let n = codec::encode_sasl_response(&mut self.write_buf, &client_final);
                                self.stream.write_all(&self.write_buf[..n]).map_err(PgError::Io)?;
                            }
                            Some(AuthType::SASLFinal) => {
                                let server_final = &body[4..];
                                scram.verify_server_final(server_final)
                                    .map_err(|e| PgError::Auth(e))?;
                            }
                            Some(AuthType::Ok) => {
                                self.consume_read(msg_len);
                                return Ok(());
                            }
                            _ => {
                                return Err(PgError::Auth("Unexpected auth message during SASL".to_string()));
                            }
                        }
                    }
                    _ => {
                        // Skip
                    }
                }
                self.consume_read(msg_len);
            }
        }
    }

    // ─── Query Methods ────────────────────────────────────────

    /// Execute a simple query (no parameters). Returns all result rows.
    pub fn query_simple(&mut self, sql: &str) -> PgResult<Vec<Row>> {
        let n = codec::encode_query(&mut self.write_buf, sql);
        self.stream.write_all(&self.write_buf[..n]).map_err(PgError::Io)?;
        self.read_query_results()
    }

    /// Execute a parameterized query using the Extended Query Protocol.
    /// Uses implicit statement caching for performance.
    pub fn query(&mut self, sql: &str, params: &[&dyn crate::types::ToParam]) -> PgResult<Vec<Row>> {
        let stmt = self.stmt_cache.get_or_create(sql);
        let mut pos = 0;

        if stmt.is_new {
            // Parse
            let n = codec::encode_parse(&mut self.write_buf[pos..], &stmt.name, sql, &[]);
            pos += n;

            // Describe (to get column info)
            let n = codec::encode_describe(&mut self.write_buf[pos..], DescribeTarget::Statement, &stmt.name);
            pos += n;
        }

        // Bind
        let param_values: Vec<Option<Vec<u8>>> = params.iter()
            .map(|p| p.to_param().to_text_bytes())
            .collect();
        let param_refs: Vec<Option<&[u8]>> = param_values.iter()
            .map(|p| p.as_deref())
            .collect();
        let n = codec::encode_bind(
            &mut self.write_buf[pos..],
            "",               // unnamed portal
            &stmt.name,
            &[],              // all text format
            &param_refs,
            &[],              // all text format results
        );
        pos += n;

        // Execute
        let n = codec::encode_execute(&mut self.write_buf[pos..], "", 0);
        pos += n;

        // Sync
        let n = codec::encode_sync(&mut self.write_buf[pos..]);
        pos += n;

        self.stream.write_all(&self.write_buf[..pos]).map_err(PgError::Io)?;

        // Read results
        let rows = self.read_extended_results(sql, &stmt.name, stmt.is_new, stmt.columns)?;
        Ok(rows)
    }

    /// Execute a query expecting exactly one row.
    pub fn query_one(&mut self, sql: &str, params: &[&dyn crate::types::ToParam]) -> PgResult<Row> {
        let rows = self.query(sql, params)?;
        rows.into_iter().next().ok_or(PgError::NoRows)
    }

    /// Execute a statement that returns no rows (INSERT, UPDATE, DELETE).
    pub fn execute(&mut self, sql: &str, params: &[&dyn crate::types::ToParam]) -> PgResult<u64> {
        let rows = self.query(sql, params)?;
        // The number of affected rows is typically in CommandComplete, but
        // for simplicity we return the vec length. The real implementation
        // would parse the CommandComplete tag.
        Ok(rows.len() as u64)
    }

    // ─── Transaction Support ──────────────────────────────────

    /// Begin a transaction.
    pub fn begin(&mut self) -> PgResult<()> {
        self.query_simple("BEGIN")?;
        Ok(())
    }

    /// Commit the current transaction.
    pub fn commit(&mut self) -> PgResult<()> {
        self.query_simple("COMMIT")?;
        Ok(())
    }

    /// Rollback the current transaction.
    pub fn rollback(&mut self) -> PgResult<()> {
        self.query_simple("ROLLBACK")?;
        Ok(())
    }

    /// Create a savepoint.
    pub fn savepoint(&mut self, name: &str) -> PgResult<()> {
        self.query_simple(&format!("SAVEPOINT {}", name))?;
        Ok(())
    }

    /// Rollback to a savepoint.
    pub fn rollback_to(&mut self, name: &str) -> PgResult<()> {
        self.query_simple(&format!("ROLLBACK TO SAVEPOINT {}", name))?;
        Ok(())
    }

    /// Release a savepoint.
    pub fn release_savepoint(&mut self, name: &str) -> PgResult<()> {
        self.query_simple(&format!("RELEASE SAVEPOINT {}", name))?;
        Ok(())
    }

    // ─── COPY Protocol ────────────────────────────────────────

    /// Start a COPY FROM STDIN operation.
    pub fn copy_in(&mut self, sql: &str) -> PgResult<CopyWriter<'_>> {
        let n = codec::encode_query(&mut self.write_buf, sql);
        self.stream.write_all(&self.write_buf[..n]).map_err(PgError::Io)?;

        // Read until CopyInResponse
        loop {
            self.fill_read_buf()?;
            let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos]) else {
                continue;
            };
            let header = codec::decode_header(&self.read_buf).unwrap();
            match header.tag {
                BackendTag::CopyInResponse => {
                    self.consume_read(msg_len);
                    return Ok(CopyWriter { conn: self });
                }
                BackendTag::ErrorResponse => {
                    let body = &self.read_buf[5..msg_len];
                    return Err(self.parse_error(body));
                }
                _ => {
                    self.consume_read(msg_len);
                }
            }
        }
    }

    // ─── LISTEN / NOTIFY ──────────────────────────────────────

    /// Subscribe to a notification channel.
    pub fn listen(&mut self, channel: &str) -> PgResult<()> {
        self.query_simple(&format!("LISTEN {}", channel))?;
        Ok(())
    }

    /// Send a notification.
    pub fn notify(&mut self, channel: &str, payload: &str) -> PgResult<()> {
        self.query_simple(&format!("NOTIFY {}, '{}'", channel, payload))?;
        Ok(())
    }

    /// Get the current transaction status.
    pub fn transaction_status(&self) -> TransactionStatus {
        self.tx_status
    }

    /// Get the number of cached statements.
    pub fn cached_statements(&self) -> usize {
        self.stmt_cache.len()
    }

    // ─── Internal Methods ─────────────────────────────────────

    fn fill_read_buf(&mut self) -> PgResult<()> {
        let n = self.stream.read(&mut self.read_buf[self.read_pos..]).map_err(PgError::Io)?;
        if n == 0 {
            return Err(PgError::ConnectionClosed);
        }
        self.read_pos += n;
        Ok(())
    }

    fn consume_read(&mut self, n: usize) {
        self.read_buf.copy_within(n..self.read_pos, 0);
        self.read_pos -= n;
    }

    fn read_query_results(&mut self) -> PgResult<Vec<Row>> {
        let mut rows = Vec::new();
        let mut columns: Vec<codec::ColumnDesc> = Vec::new();

        loop {
            self.fill_read_buf()?;

            while let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos]) {
                let header = codec::decode_header(&self.read_buf).unwrap();
                let body = &self.read_buf[5..msg_len];

                match header.tag {
                    BackendTag::RowDescription => {
                        columns = codec::parse_row_description(body);
                    }
                    BackendTag::DataRow => {
                        let raw_values = codec::parse_data_row(body);
                        rows.push(Row::new(columns.clone(), raw_values));
                    }
                    BackendTag::CommandComplete => {
                        // Query finished
                    }
                    BackendTag::ReadyForQuery => {
                        self.tx_status = TransactionStatus::from(body[0]);
                        self.consume_read(msg_len);
                        return Ok(rows);
                    }
                    BackendTag::ErrorResponse => {
                        let err = self.parse_error(body);
                        self.consume_read(msg_len);
                        // Drain to ReadyForQuery
                        self.drain_to_ready()?;
                        return Err(err);
                    }
                    BackendTag::EmptyQueryResponse | BackendTag::NoticeResponse => {
                        // Skip
                    }
                    _ => {}
                }
                self.consume_read(msg_len);
            }
        }
    }

    fn read_extended_results(
        &mut self,
        sql: &str,
        stmt_name: &str,
        is_new: bool,
        cached_columns: Option<Vec<codec::ColumnDesc>>,
    ) -> PgResult<Vec<Row>> {
        let mut rows = Vec::new();
        let mut columns = cached_columns.unwrap_or_default();

        loop {
            self.fill_read_buf()?;

            while let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos]) {
                let header = codec::decode_header(&self.read_buf).unwrap();
                let body = &self.read_buf[5..msg_len];

                match header.tag {
                    BackendTag::ParseComplete => {}
                    BackendTag::ParameterDescription => {}
                    BackendTag::RowDescription => {
                        columns = codec::parse_row_description(body);
                        if is_new {
                            self.stmt_cache.insert(sql, stmt_name.to_string(), 0, Some(columns.clone()));
                        }
                    }
                    BackendTag::NoData => {
                        if is_new {
                            self.stmt_cache.insert(sql, stmt_name.to_string(), 0, None);
                        }
                    }
                    BackendTag::BindComplete => {}
                    BackendTag::DataRow => {
                        let raw_values = codec::parse_data_row(body);
                        rows.push(Row::new(columns.clone(), raw_values));
                    }
                    BackendTag::CommandComplete => {}
                    BackendTag::ReadyForQuery => {
                        self.tx_status = TransactionStatus::from(body[0]);
                        self.consume_read(msg_len);
                        return Ok(rows);
                    }
                    BackendTag::ErrorResponse => {
                        let err = self.parse_error(body);
                        self.consume_read(msg_len);
                        self.drain_to_ready()?;
                        return Err(err);
                    }
                    _ => {}
                }
                self.consume_read(msg_len);
            }
        }
    }

    fn drain_to_ready(&mut self) -> PgResult<()> {
        loop {
            self.fill_read_buf()?;
            while let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos]) {
                let header = codec::decode_header(&self.read_buf).unwrap();
                if header.tag == BackendTag::ReadyForQuery {
                    let body = &self.read_buf[5..msg_len];
                    self.tx_status = TransactionStatus::from(body[0]);
                    self.consume_read(msg_len);
                    return Ok(());
                }
                self.consume_read(msg_len);
            }
        }
    }

    fn parse_error(&self, body: &[u8]) -> PgError {
        let fields = codec::parse_error_fields(body);
        let mut severity = String::new();
        let mut code = String::new();
        let mut message = String::new();
        for (field_type, value) in &fields {
            match field_type {
                b'S' => severity = value.clone(),
                b'C' => code = value.clone(),
                b'M' => message = value.clone(),
                _ => {}
            }
        }
        PgError::Server { severity, code, message }
    }
}

impl Drop for PgConnection {
    fn drop(&mut self) {
        // Send Terminate message
        let n = codec::encode_terminate(&mut self.write_buf);
        let _ = self.stream.write_all(&self.write_buf[..n]);
    }
}

/// COPY writer for streaming data into PostgreSQL.
pub struct CopyWriter<'a> {
    conn: &'a mut PgConnection,
}

impl<'a> CopyWriter<'a> {
    /// Write a chunk of COPY data.
    pub fn write_data(&mut self, data: &[u8]) -> PgResult<()> {
        let n = codec::encode_copy_data(&mut self.conn.write_buf, data);
        self.conn.stream.write_all(&self.conn.write_buf[..n]).map_err(PgError::Io)
    }

    /// Finish the COPY operation.
    pub fn finish(self) -> PgResult<()> {
        let n = codec::encode_copy_done(&mut self.conn.write_buf);
        self.conn.stream.write_all(&self.conn.write_buf[..n]).map_err(PgError::Io)?;

        // Drain to ReadyForQuery
        loop {
            self.conn.fill_read_buf()?;
            while let Some(msg_len) = codec::message_complete(&self.conn.read_buf[..self.conn.read_pos]) {
                let header = codec::decode_header(&self.conn.read_buf).unwrap();
                if header.tag == BackendTag::ReadyForQuery {
                    let body = &self.conn.read_buf[5..msg_len];
                    self.conn.tx_status = TransactionStatus::from(body[0]);
                    self.conn.consume_read(msg_len);
                    return Ok(());
                }
                if header.tag == BackendTag::ErrorResponse {
                    let body = &self.conn.read_buf[5..msg_len];
                    let err = self.conn.parse_error(body);
                    self.conn.consume_read(msg_len);
                    return Err(err);
                }
                self.conn.consume_read(msg_len);
            }
        }
    }
}

