//! Row abstraction for query results with inline small-value optimisation.
//!
//! Column values ≤ 24 bytes (which covers all binary-format scalars: bool,
//! i16, i32, i64, f32, f64, UUID, date, time, timestamp) are stored
//! **inline** — zero heap allocations per column.  Only values larger than
//! 24 bytes (e.g. long text, bytea, jsonb) spill to the heap.

use std::rc::Rc;

use crate::codec::ColumnDesc;
use crate::error::{PgError, PgResult};
use crate::protocol::FormatCode;
use crate::types::{FromSql, PgValue};

/// Maximum number of bytes stored inline (no heap allocation).
/// 24 bytes covers: bool(1), i16(2), i32(4), i64(8), f32(4), f64(8),
/// UUID(16), Date(4), Time(8), Timestamp(8), Interval(16), MacAddr(6),
/// Point(16).
const INLINE_CAP: usize = 24;

/// Compact byte storage: small values inline, large values on the heap.
#[derive(Clone)]
enum CompactBytes {
    /// Value stored inline — no heap allocation.
    Inline { data: [u8; INLINE_CAP], len: u8 },
    /// Value too large for inline — heap allocated.
    Heap(Vec<u8>),
}

impl std::fmt::Debug for CompactBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CompactBytes({}b)", self.as_slice().len())
    }
}

impl CompactBytes {
    /// Create from a byte slice.  Inline if ≤ INLINE_CAP, heap otherwise.
    #[inline]
    fn from_slice(data: &[u8]) -> Self {
        if data.len() <= INLINE_CAP {
            let mut buf = [0u8; INLINE_CAP];
            buf[..data.len()].copy_from_slice(data);
            CompactBytes::Inline {
                data: buf,
                len: data.len() as u8,
            }
        } else {
            CompactBytes::Heap(data.to_vec())
        }
    }

    /// Borrow the stored bytes.
    #[inline]
    fn as_slice(&self) -> &[u8] {
        match self {
            CompactBytes::Inline { data, len } => &data[..*len as usize],
            CompactBytes::Heap(v) => v,
        }
    }
}

/// A row returned from a query. Contains column descriptions and raw data.
///
/// Column descriptors are shared via `Rc` across all rows in a result set,
/// avoiding expensive deep clones on every `DataRow` message.
///
/// Column values use [`CompactBytes`] — values ≤ 24 bytes are stored inline
/// (no heap allocation), which covers all binary-format scalar types.
#[derive(Debug)]
pub struct Row {
    columns: Rc<Vec<ColumnDesc>>,
    values: Vec<Option<CompactBytes>>,
}

impl Row {
    /// Create a new row from shared column descriptions and raw column data.
    ///
    /// Small values (≤ 24 bytes) are stored inline with zero heap allocation.
    pub fn new(columns: Rc<Vec<ColumnDesc>>, raw_values: Vec<Option<&[u8]>>) -> Self {
        let values = raw_values
            .into_iter()
            .map(|v| v.map(CompactBytes::from_slice))
            .collect();
        Self { columns, values }
    }

    /// Safely create a mock row for testing, bypassing wire protocols.
    pub fn mock(names: &[&str], values: &[PgValue]) -> Self {
        let mut cols = Vec::new();
        let mut raw_values: Vec<Option<CompactBytes>> = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let (type_oid, data) = match &values[i] {
                PgValue::Null => (25, None), // default to text
                PgValue::Int4(v) => (23, Some(v.to_string().into_bytes())),
                PgValue::Int8(v) => (20, Some(v.to_string().into_bytes())),
                PgValue::Text(s) => (25, Some(s.clone().into_bytes())),
                PgValue::Bool(b) => (16, Some(if *b { b"t".to_vec() } else { b"f".to_vec() })),
                // A complete map isn't necessary for a lightweight mock just matching basic fields
                _ => (25, values[i].to_text_bytes()),
            };
            cols.push(ColumnDesc {
                name: name.to_string(),
                table_oid: 0,
                col_attr: 0,
                type_oid,
                type_size: -1,
                type_modifier: -1,
                format_code: FormatCode::Text,
            });
            raw_values.push(data.map(|d| CompactBytes::from_slice(&d)));
        }
        Self {
            columns: Rc::new(cols),
            values: raw_values,
        }
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
                FormatCode::Text => PgValue::from_text(col.type_oid, data.as_slice()),
                FormatCode::Binary => PgValue::from_binary(col.type_oid, data.as_slice()),
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

    /// Get a typed value by column index using the `FromSql` trait.
    ///
    /// # Example
    /// ```ignore
    /// let name: String = row.get_typed(0)?;
    /// let age: Option<i32> = row.get_typed(1)?;
    /// ```
    pub fn get_typed<T: FromSql>(&self, index: usize) -> PgResult<T> {
        let value = self.get(index)?;
        T::from_sql(&value)
    }

    /// Get a typed value by column name using the `FromSql` trait.
    pub fn get_typed_by_name<T: FromSql>(&self, name: &str) -> PgResult<T> {
        let value = self.get_by_name(name)?;
        T::from_sql(&value)
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
            Some(data) => std::str::from_utf8(data.as_slice())
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

    /// Get a column as a UUID string (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx).
    pub fn get_uuid(&self, index: usize) -> PgResult<Option<[u8; 16]>> {
        match self.get(index)? {
            PgValue::Null => Ok(None),
            PgValue::Uuid(bytes) => Ok(Some(bytes)),
            _ => Err(PgError::TypeConversion(
                "Cannot convert to UUID".to_string(),
            )),
        }
    }

    /// Get column descriptions.
    pub fn columns(&self) -> &[ColumnDesc] {
        &self.columns
    }

    /// Get a column name by index.
    pub fn column_name(&self, index: usize) -> Option<&str> {
        self.columns.get(index).map(|c| c.name.as_str())
    }

    /// Find a column index by name.
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::ColumnDesc;
    use crate::error::{PgError, PgResult};
    use crate::protocol::FormatCode;
    use crate::types::PgValue;
    use std::rc::Rc;

    // OID constants (text format values are parsed the same as DB text protocol)
    const OID_TEXT: u32 = 25;
    const OID_INT4: u32 = 23;
    const OID_INT8: u32 = 20;
    const OID_BOOL: u32 = 16;
    const OID_FLOAT8: u32 = 701;

    fn col(name: &str, type_oid: u32) -> ColumnDesc {
        ColumnDesc {
            name: name.to_string(),
            table_oid: 0,
            col_attr: 0,
            type_oid,
            type_size: -1,
            type_modifier: -1,
            format_code: FormatCode::Text,
        }
    }

    fn make_row(cols: &[(&str, u32)], vals: &[Option<&[u8]>]) -> Row {
        let descs = Rc::new(cols.iter().map(|(n, o)| col(n, *o)).collect::<Vec<_>>());
        Row::new(descs, vals.to_vec())
    }

    // ─── Structural ───────────────────────────────────────────────────────────

    #[test]
    fn test_row_len_two_columns() {
        let row = make_row(
            &[("name", OID_TEXT), ("age", OID_INT4)],
            &[Some(b"alice"), Some(b"30")],
        );
        assert_eq!(row.len(), 2);
        assert!(!row.is_empty());
    }

    #[test]
    fn test_row_empty_columns() {
        let row = Row::new(Rc::new(vec![]), vec![]);
        assert_eq!(row.len(), 0);
        assert!(row.is_empty());
    }

    #[test]
    fn test_row_len_matches_column_count() {
        let row = make_row(
            &[("a", OID_TEXT), ("b", OID_INT4), ("c", OID_BOOL)],
            &[Some(b"x"), Some(b"1"), Some(b"t")],
        );
        assert_eq!(row.len(), row.columns().len());
    }

    // ─── get() by index ───────────────────────────────────────────────────────

    #[test]
    fn test_get_text_value() {
        let row = make_row(&[("name", OID_TEXT)], &[Some(b"hello")]);
        match row.get(0).unwrap() {
            PgValue::Text(s) => assert_eq!(s, "hello"),
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[test]
    fn test_get_int4_value() {
        let row = make_row(&[("n", OID_INT4)], &[Some(b"42")]);
        match row.get(0).unwrap() {
            PgValue::Int4(v) => assert_eq!(v, 42),
            other => panic!("Expected Int4, got {:?}", other),
        }
    }

    #[test]
    fn test_get_bool_true() {
        let row = make_row(&[("flag", OID_BOOL)], &[Some(b"t")]);
        match row.get(0).unwrap() {
            PgValue::Bool(v) => assert!(v),
            other => panic!("Expected Bool, got {:?}", other),
        }
    }

    #[test]
    fn test_get_bool_false() {
        let row = make_row(&[("flag", OID_BOOL)], &[Some(b"f")]);
        match row.get(0).unwrap() {
            PgValue::Bool(v) => assert!(!v),
            other => panic!("Expected Bool, got {:?}", other),
        }
    }

    #[test]
    fn test_get_null_returns_pgvalue_null() {
        let row = make_row(&[("name", OID_TEXT)], &[None]);
        assert!(matches!(row.get(0).unwrap(), PgValue::Null));
    }

    #[test]
    fn test_get_index_out_of_range_returns_error() {
        let row = make_row(&[("name", OID_TEXT)], &[Some(b"alice")]);
        let err = row.get(99);
        assert!(err.is_err());
        if let Err(PgError::TypeConversion(msg)) = err {
            assert!(
                msg.contains("out of range") || msg.contains("index"),
                "Error should mention out-of-range: {}",
                msg
            );
        } else {
            panic!("Expected TypeConversion error");
        }
    }

    // ─── get_by_name() ────────────────────────────────────────────────────────

    #[test]
    fn test_get_by_name_found() {
        let row = make_row(
            &[("id", OID_INT4), ("name", OID_TEXT)],
            &[Some(b"1"), Some(b"charlie")],
        );
        match row.get_by_name("name").unwrap() {
            PgValue::Text(s) => assert_eq!(s, "charlie"),
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[test]
    fn test_get_by_name_not_found_returns_error() {
        let row = make_row(&[("id", OID_INT4)], &[Some(b"5")]);
        let err = row.get_by_name("nonexistent");
        assert!(err.is_err());
        if let Err(PgError::TypeConversion(msg)) = err {
            assert!(
                msg.contains("not found") || msg.contains("nonexistent"),
                "Error should mention missing column: {}",
                msg
            );
        } else {
            panic!("Expected TypeConversion error");
        }
    }

    #[test]
    fn test_get_by_name_null_column() {
        let row = make_row(&[("data", OID_TEXT)], &[None]);
        assert!(matches!(row.get_by_name("data").unwrap(), PgValue::Null));
    }

    // ─── Convenience accessors ────────────────────────────────────────────────

    #[test]
    fn test_get_str_valid_utf8() {
        let row = make_row(&[("msg", OID_TEXT)], &[Some(b"hello world")]);
        assert_eq!(row.get_str(0).unwrap(), Some("hello world"));
    }

    #[test]
    fn test_get_str_null() {
        let row = make_row(&[("msg", OID_TEXT)], &[None]);
        assert_eq!(row.get_str(0).unwrap(), None);
    }

    #[test]
    fn test_get_i32_value() {
        let row = make_row(&[("n", OID_INT4)], &[Some(b"777")]);
        assert_eq!(row.get_i32(0).unwrap(), Some(777));
    }

    #[test]
    fn test_get_i32_null() {
        let row = make_row(&[("n", OID_INT4)], &[None]);
        assert_eq!(row.get_i32(0).unwrap(), None);
    }

    #[test]
    fn test_get_i64_large_value() {
        let row = make_row(&[("n", OID_INT8)], &[Some(b"9876543210")]);
        assert_eq!(row.get_i64(0).unwrap(), Some(9_876_543_210_i64));
    }

    #[test]
    fn test_get_i64_null() {
        let row = make_row(&[("n", OID_INT8)], &[None]);
        assert_eq!(row.get_i64(0).unwrap(), None);
    }

    #[test]
    fn test_get_bool_null() {
        let row = make_row(&[("flag", OID_BOOL)], &[None]);
        assert_eq!(row.get_bool(0).unwrap(), None);
    }

    #[test]
    fn test_get_f64_value() {
        let pi = std::f64::consts::PI;
        let text = pi.to_string();
        let row = make_row(&[("score", OID_FLOAT8)], &[Some(text.as_bytes())]);
        let v = row.get_f64(0).unwrap().unwrap();
        assert!((v - pi).abs() < f64::EPSILON);
    }

    #[test]
    fn test_get_f64_null() {
        let row = make_row(&[("score", OID_FLOAT8)], &[None]);
        assert_eq!(row.get_f64(0).unwrap(), None);
    }

    // ─── column_name / column_index ───────────────────────────────────────────

    #[test]
    fn test_column_name_by_index() {
        let row = make_row(
            &[("user_id", OID_INT4), ("email", OID_TEXT)],
            &[Some(b"1"), Some(b"a@b.com")],
        );
        assert_eq!(row.column_name(0), Some("user_id"));
        assert_eq!(row.column_name(1), Some("email"));
        assert_eq!(row.column_name(99), None);
    }

    #[test]
    fn test_column_index_by_name() {
        let row = make_row(
            &[("id", OID_INT4), ("name", OID_TEXT)],
            &[Some(b"1"), Some(b"bob")],
        );
        assert_eq!(row.column_index("id"), Some(0));
        assert_eq!(row.column_index("name"), Some(1));
        assert_eq!(row.column_index("missing"), None);
    }

    #[test]
    fn test_columns_slice() {
        let row = make_row(
            &[("a", OID_TEXT), ("b", OID_INT4)],
            &[Some(b"x"), Some(b"1")],
        );
        let cols = row.columns();
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].name, "a");
        assert_eq!(cols[1].name, "b");
    }

    // ─── get_typed / get_typed_by_name ────────────────────────────────────────

    #[test]
    fn test_get_typed_i32() {
        let row = make_row(&[("n", OID_INT4)], &[Some(b"77")]);
        let v: i32 = row.get_typed(0).unwrap();
        assert_eq!(v, 77);
    }

    #[test]
    fn test_get_typed_option_null_is_none() {
        let row = make_row(&[("n", OID_INT4)], &[None]);
        let v: Option<i32> = row.get_typed(0).unwrap();
        assert_eq!(v, None);
    }

    #[test]
    fn test_get_typed_option_value_is_some() {
        let row = make_row(&[("n", OID_INT4)], &[Some(b"55")]);
        let v: Option<i32> = row.get_typed(0).unwrap();
        assert_eq!(v, Some(55));
    }

    #[test]
    fn test_get_typed_by_name_string() {
        let row = make_row(&[("label", OID_TEXT)], &[Some(b"production")]);
        let v: String = row.get_typed_by_name("label").unwrap();
        assert_eq!(v, "production");
    }

    #[test]
    fn test_get_typed_by_name_not_found() {
        let row = make_row(&[("n", OID_INT4)], &[Some(b"1")]);
        let r: PgResult<i32> = row.get_typed_by_name("missing");
        assert!(r.is_err());
    }

    // ─── Rc sharing — performance characteristic ─────────────────────────────
    // Verifies that column descriptors are NOT copied per row (O(1) sharing).

    #[test]
    fn test_rc_columns_shared_across_rows() {
        let cols = Rc::new(vec![col("v", OID_INT4)]);
        let initial_count = Rc::strong_count(&cols);

        let row1 = Row::new(Rc::clone(&cols), vec![Some(b"1")]);
        let row2 = Row::new(Rc::clone(&cols), vec![Some(b"2")]);
        let row3 = Row::new(Rc::clone(&cols), vec![Some(b"3")]);

        // cols + row1 + row2 + row3
        assert_eq!(Rc::strong_count(&cols), initial_count + 3);

        drop(row1);
        assert_eq!(Rc::strong_count(&cols), initial_count + 2);

        drop(row2);
        drop(row3);
        assert_eq!(Rc::strong_count(&cols), initial_count);
    }

    #[test]
    fn test_rc_1000_rows_share_same_column_descriptor() {
        // Scalability: 1000 rows share one column vec — no deep copies
        let cols = Rc::new(vec![col("id", OID_INT4), col("name", OID_TEXT)]);
        let rows: Vec<Row> = (0..1000)
            .map(|i| {
                let val = i.to_string();
                // NOTE: raw bytes live in their own allocation per row, which is expected
                Row::new(Rc::clone(&cols), vec![Some(val.as_bytes()), Some(b"x")])
            })
            .collect();

        // Only 1 + 1000 strong refs, NOT 1001 deep copies of the column vec
        assert_eq!(Rc::strong_count(&cols), 1001);

        drop(rows);
        assert_eq!(Rc::strong_count(&cols), 1);
    }

    // ─── Multiple columns edge cases ─────────────────────────────────────────

    #[test]
    fn test_get_mixed_null_and_non_null() {
        let row = make_row(
            &[("a", OID_TEXT), ("b", OID_INT4), ("c", OID_TEXT)],
            &[Some(b"hello"), None, Some(b"world")],
        );
        assert!(matches!(row.get(0).unwrap(), PgValue::Text(_)));
        assert!(matches!(row.get(1).unwrap(), PgValue::Null));
        assert!(matches!(row.get(2).unwrap(), PgValue::Text(_)));
    }

    #[test]
    fn test_get_first_and_last_column() {
        let row = make_row(
            &[
                ("first", OID_INT4),
                ("middle", OID_TEXT),
                ("last", OID_BOOL),
            ],
            &[Some(b"1"), Some(b"mid"), Some(b"t")],
        );
        assert!(matches!(row.get(0).unwrap(), PgValue::Int4(1)));
        assert!(matches!(row.get(2).unwrap(), PgValue::Bool(true)));
    }
}
