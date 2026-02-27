//! Zero-copy row abstraction for query results.

use crate::codec::ColumnDesc;
use crate::error::{PgError, PgResult};
use crate::protocol::FormatCode;
use crate::types::PgValue;

/// A row returned from a query. Contains column descriptions and raw data.
#[derive(Debug)]
pub struct Row {
    columns: Vec<ColumnDesc>,
    values: Vec<Option<Vec<u8>>>,
}

impl Row {
    /// Create a new row from column descriptions and raw column data.
    pub fn new(columns: Vec<ColumnDesc>, raw_values: Vec<Option<&[u8]>>) -> Self {
        let values = raw_values
            .into_iter()
            .map(|v| v.map(|d| d.to_vec()))
            .collect();
        Self { columns, values }
    }

    /// Get the number of columns.
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// Check if the row is empty.
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// Get a column value by index as a PgValue.
    pub fn get(&self, index: usize) -> PgResult<PgValue> {
        if index >= self.columns.len() {
            return Err(PgError::TypeConversion(format!(
                "Column index {} out of range",
                index
            )));
        }

        let col = &self.columns[index];
        match &self.values[index] {
            None => Ok(PgValue::Null),
            Some(data) => match col.format_code {
                FormatCode::Text => PgValue::from_text(col.type_oid, data),
                FormatCode::Binary => PgValue::from_binary(col.type_oid, data),
            },
        }
    }

    /// Get a column value by name as a PgValue.
    pub fn get_by_name(&self, name: &str) -> PgResult<PgValue> {
        let index = self
            .columns
            .iter()
            .position(|c| c.name == name)
            .ok_or_else(|| PgError::TypeConversion(format!("Column '{}' not found", name)))?;
        self.get(index)
    }

    /// Get a column as a string (text representation).
    pub fn get_str(&self, index: usize) -> PgResult<Option<&str>> {
        if index >= self.columns.len() {
            return Err(PgError::TypeConversion(format!(
                "Column index {} out of range",
                index
            )));
        }
        match &self.values[index] {
            None => Ok(None),
            Some(data) => std::str::from_utf8(data)
                .map(Some)
                .map_err(|_| PgError::TypeConversion("Invalid UTF-8".to_string())),
        }
    }

    /// Get a column as i32.
    pub fn get_i32(&self, index: usize) -> PgResult<Option<i32>> {
        match self.get(index)? {
            PgValue::Null => Ok(None),
            PgValue::Int4(v) => Ok(Some(v)),
            PgValue::Int2(v) => Ok(Some(v as i32)),
            PgValue::Text(s) => s
                .parse()
                .map(Some)
                .map_err(|_| PgError::TypeConversion("Not an i32".to_string())),
            _ => Err(PgError::TypeConversion("Cannot convert to i32".to_string())),
        }
    }

    /// Get a column as i64.
    pub fn get_i64(&self, index: usize) -> PgResult<Option<i64>> {
        match self.get(index)? {
            PgValue::Null => Ok(None),
            PgValue::Int8(v) => Ok(Some(v)),
            PgValue::Int4(v) => Ok(Some(v as i64)),
            PgValue::Int2(v) => Ok(Some(v as i64)),
            PgValue::Text(s) => s
                .parse()
                .map(Some)
                .map_err(|_| PgError::TypeConversion("Not an i64".to_string())),
            _ => Err(PgError::TypeConversion("Cannot convert to i64".to_string())),
        }
    }

    /// Get a column as bool.
    pub fn get_bool(&self, index: usize) -> PgResult<Option<bool>> {
        match self.get(index)? {
            PgValue::Null => Ok(None),
            PgValue::Bool(v) => Ok(Some(v)),
            _ => Err(PgError::TypeConversion(
                "Cannot convert to bool".to_string(),
            )),
        }
    }

    /// Get a column as f64.
    pub fn get_f64(&self, index: usize) -> PgResult<Option<f64>> {
        match self.get(index)? {
            PgValue::Null => Ok(None),
            PgValue::Float8(v) => Ok(Some(v)),
            PgValue::Float4(v) => Ok(Some(v as f64)),
            PgValue::Int4(v) => Ok(Some(v as f64)),
            PgValue::Text(s) => s
                .parse()
                .map(Some)
                .map_err(|_| PgError::TypeConversion("Not an f64".to_string())),
            _ => Err(PgError::TypeConversion("Cannot convert to f64".to_string())),
        }
    }

    /// Get column descriptions.
    pub fn columns(&self) -> &[ColumnDesc] {
        &self.columns
    }
}
