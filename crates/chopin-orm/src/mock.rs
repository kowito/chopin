use crate::{Executor, OrmResult};
use chopin_pg::Row;

/// An in-memory testing stub satisfying the `Executor` trait without PostgreSQL connections.
///
/// Intercepts inbound queries while releasing user-defined `mock_row!` sequences dynamically natively into client code.
pub struct MockExecutor {
    pub executed_queries: Vec<(String, usize)>,
    pub mocked_results: Vec<Vec<Row>>,
}

impl MockExecutor {
    pub fn new() -> Self {
        Self {
            executed_queries: Vec::new(),
            mocked_results: Vec::new(),
        }
    }

    /// Enqueues an ordered sequence of Mock configurations to be drained consecutively by the test suite upon query intercepts.
    pub fn push_result(&mut self, rows: Vec<Row>) {
        self.mocked_results.push(rows);
    }
}

impl Default for MockExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl Executor for MockExecutor {
    fn execute(&mut self, query: &str, params: &[&dyn chopin_pg::types::ToSql]) -> OrmResult<u64> {
        self.executed_queries
            .push((query.to_string(), params.len()));
        Ok(1)
    }

    fn query(
        &mut self,
        query: &str,
        params: &[&dyn chopin_pg::types::ToSql],
    ) -> OrmResult<Vec<Row>> {
        self.executed_queries
            .push((query.to_string(), params.len()));
        if !self.mocked_results.is_empty() {
            Ok(self.mocked_results.remove(0))
        } else {
            Ok(vec![])
        }
    }
}

/// Constructs a sequence-free structurally valid `chopin_pg::Row` from literal tuple representations.
///
/// Useful directly within `MockExecutor::push_result`.
#[macro_export]
macro_rules! mock_row {
    ( $( $name:expr => $val:expr ),* $(,)? ) => {
        {
            use chopin_pg::types::ToParam;
            let mut names = Vec::new();
            let mut vals: Vec<chopin_pg::PgValue> = Vec::new();
            $(
                names.push($name);
                vals.push($val.to_param());
            )*
            chopin_pg::Row::mock(&names, &vals)
        }
    };
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use super::*;
    use crate as chopin_orm;
    use crate::{Model, builder::ColumnTrait};

    #[derive(Model, Debug, Clone, PartialEq)]
    #[model(table_name = "tester")]
    pub struct Tester {
        #[model(primary_key)]
        pub id: i32,
        pub name: String,
    }
    impl crate::Validate for Tester {}

    #[test]
    fn test_mock_executor() {
        let mut mock = MockExecutor::new();
        mock.push_result(vec![
            mock_row!("id" => 1, "name" => "Alice"),
            mock_row!("id" => 2, "name" => "Bob"),
        ]);

        let results = Tester::find()
            .filter(TesterColumn::id.gt(0))
            .all(&mut mock)
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "Alice");
        assert_eq!(results[1].name, "Bob");
        assert_eq!(mock.executed_queries.len(), 1);
        assert!(
            mock.executed_queries[0]
                .0
                .contains("SELECT id, name FROM tester WHERE id > $1")
        );
    }
}
