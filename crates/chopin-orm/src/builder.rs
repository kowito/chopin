use crate::{Model, OrmError, OrmResult, PgValue};
use std::marker::PhantomData;

/// A type alias for `Condition<M>`, representing a SQL expression for a specific Model.
pub type Expr<M> = Condition<M>;

/// A tree representing a SQL condition (WHERE or HAVING clause) used to filter queries.
/// Supports generic `And` and `Or` nesting and intelligently binds indexed parameters.
pub enum Condition<M> {
    Raw(String, Vec<PgValue>, PhantomData<M>),
    And(Vec<Condition<M>>),
    Or(Vec<Condition<M>>),
}

impl<M> Clone for Condition<M> {
    fn clone(&self) -> Self {
        match self {
            Condition::Raw(c, p, _) => Condition::Raw(c.clone(), p.clone(), PhantomData),
            Condition::And(c) => Condition::And(c.clone()),
            Condition::Or(c) => Condition::Or(c.clone()),
        }
    }
}

impl<M> Condition<M> {
    /// Create a raw SQL condition segment.
    /// Use `{}` as a placeholder for parameterized values.
    /// ```ignore
    /// Condition::new("age > {}", vec![25.to_param()])
    /// ```
    pub fn new(clause: impl Into<String>, params: Vec<PgValue>) -> Self {
        Condition::Raw(clause.into(), params, PhantomData)
    }

    /// Combine this condition with another using `AND`.
    pub fn and(self, other: Self) -> Self {
        match self {
            Condition::And(mut conds) => {
                conds.push(other);
                Condition::And(conds)
            }
            _ => Condition::And(vec![self, other]),
        }
    }

    /// Combine this condition with another using `OR`.
    pub fn or(self, other: Self) -> Self {
        match self {
            Condition::Or(mut conds) => {
                conds.push(other);
                Condition::Or(conds)
            }
            _ => Condition::Or(vec![self, other]),
        }
    }

    /// Resolves the condition tree into a parameterized SQL string.
    ///
    /// Collects references to the `PgValue` parameters owned by this condition tree.
    /// The returned references are valid as long as `self` is alive.
    ///
    /// # Safety / Security
    /// This method performs placeholder mapping from `{}` to `$n`. While standard
    /// DSL methods in `ColumnTrait` are safe, using `Condition::Raw` with unsanitized
    /// user input in the clause string can lead to SQL injection. Always use `{}`
    /// for values and pass them via the `params` vector.
    fn resolve<'a>(
        &'a self,
        param_idx: &mut usize,
        params_out: &mut Vec<&'a PgValue>,
    ) -> String {
        match self {
            Condition::Raw(clause, params, _) => {
                let mut resolved = String::with_capacity(clause.len());
                let mut chars = clause.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '{' && chars.peek() == Some(&'}') {
                        chars.next();
                        resolved.push('$');
                        resolved.push_str(&param_idx.to_string());
                        *param_idx += 1;
                    } else {
                        resolved.push(c);
                    }
                }
                params_out.extend(params.iter());
                resolved
            }
            Condition::And(conds) => {
                let resolved: Vec<_> = conds
                    .iter()
                    .map(|c| c.resolve(param_idx, params_out))
                    .collect();
                format!("({})", resolved.join(" AND "))
            }
            Condition::Or(conds) => {
                let resolved: Vec<_> = conds
                    .iter()
                    .map(|c| c.resolve(param_idx, params_out))
                    .collect();
                format!("({})", resolved.join(" OR "))
            }
        }
    }
}

/// Trait for defining operations natively on database columns.
pub trait ColumnTrait<M: Model> {
    fn column_name(&self) -> &'static str;

    fn eq(self, val: impl crate::ToSql) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("{} = {{}}", self.column_name()), vec![val.to_sql()])
    }
    fn neq(self, val: impl crate::ToSql) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(
            format!("{} != {{}}", self.column_name()),
            vec![val.to_sql()],
        )
    }
    fn gt(self, val: impl crate::ToSql) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("{} > {{}}", self.column_name()), vec![val.to_sql()])
    }
    fn gte(self, val: impl crate::ToSql) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(
            format!("{} >= {{}}", self.column_name()),
            vec![val.to_sql()],
        )
    }
    fn lt(self, val: impl crate::ToSql) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("{} < {{}}", self.column_name()), vec![val.to_sql()])
    }
    fn lte(self, val: impl crate::ToSql) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(
            format!("{} <= {{}}", self.column_name()),
            vec![val.to_sql()],
        )
    }
    #[allow(clippy::wrong_self_convention)]
    fn is_null(self) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("{} IS NULL", self.column_name()), vec![])
    }
    #[allow(clippy::wrong_self_convention)]
    fn is_not_null(self) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("{} IS NOT NULL", self.column_name()), vec![])
    }
    fn count(self) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("COUNT({})", self.column_name()), vec![])
    }
    fn sum(self) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("SUM({})", self.column_name()), vec![])
    }
    fn max(self) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("MAX({})", self.column_name()), vec![])
    }
    fn min(self) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("MIN({})", self.column_name()), vec![])
    }
    fn like(self, val: impl crate::ToSql) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(
            format!("{} LIKE {{}}", self.column_name()),
            vec![val.to_sql()],
        )
    }
    fn ilike(self, val: impl crate::ToSql) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(
            format!("{} ILIKE {{}}", self.column_name()),
            vec![val.to_sql()],
        )
    }
    #[allow(clippy::wrong_self_convention)]
    fn is_in<T: crate::ToSql>(self, vals: Vec<T>) -> Expr<M>
    where
        Self: Sized,
    {
        let placeholders: Vec<String> = (0..vals.len()).map(|_| "{}".to_string()).collect();
        let params: Vec<PgValue> = vals.into_iter().map(|v| v.to_sql()).collect();
        Expr::new(
            format!("{} IN ({})", self.column_name(), placeholders.join(", ")),
            params,
        )
    }
}

/// A type-safe SQL query builder.
///
/// Constructed primarily via `<Model>::find()` and `<Model>::select(...)`.
/// Chain methods like `.filter()`, `.order_by()`, and `.limit()` before executing.
#[must_use = "QueryBuilder does nothing until executed with .all(), .one(), .count(), etc."]
pub struct QueryBuilder<M> {
    _marker: PhantomData<M>,
    select_override: Option<Vec<Expr<M>>>,
    joins: Vec<String>,
    filters: Vec<Expr<M>>,
    group_by: Option<String>,
    having: Vec<Expr<M>>,
    order_by: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

impl<M> Clone for QueryBuilder<M> {
    fn clone(&self) -> Self {
        Self {
            _marker: PhantomData,
            select_override: self.select_override.clone(),
            joins: self.joins.clone(),
            filters: self.filters.clone(),
            group_by: self.group_by.clone(),
            having: self.having.clone(),
            order_by: self.order_by.clone(),
            limit: self.limit,
            offset: self.offset,
        }
    }
}

impl<M: Model + Send + Sync> Default for QueryBuilder<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: Model + Send + Sync> QueryBuilder<M> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
            select_override: None,
            joins: Vec::new(),
            filters: Vec::new(),
            group_by: None,
            having: Vec::new(),
            order_by: None,
            limit: None,
            offset: None,
        }
    }

    /// Add a WHERE filter clause, e.g., filter(Expr::new("age >= {}", vec![18.to_param()]))
    /// Or using DSL: filter(UserColumn::Age.gte(18.to_param()))
    /// Backwards compatibility raw signature with $1 etc is also supported if string literal passes without `{}`
    pub fn filter<E>(mut self, expr: E) -> Self
    where
        E: IntoExpr<M>,
    {
        self.filters.push(expr.into_expr());
        self
    }

    pub fn select_only<E: IntoExpr<M>>(mut self, exprs: Vec<E>) -> Self {
        self.select_override = Some(exprs.into_iter().map(|e| e.into_expr()).collect());
        self
    }

    pub fn join(mut self, clause: &str) -> Self {
        self.joins.push(clause.into());
        self
    }

    /// Adds an `INNER JOIN` automatically resolving foreign keys using `HasForeignKey`.
    pub fn join_child<R: Model + crate::HasForeignKey<M>>(mut self) -> Self {
        let (other_table, mappings) = R::foreign_key_info();
        let my_table = M::table_name();

        let join_on = mappings
            .iter()
            .map(|(child_col, parent_col)| {
                format!("{}.{} = {}.{}", other_table, child_col, my_table, parent_col)
            })
            .collect::<Vec<_>>()
            .join(" AND ");

        self.joins.push(format!("JOIN {} ON {}", other_table, join_on));
        self
    }

    /// Adds an `INNER JOIN` automatically resolving the parent entity foreign keys.
    pub fn join_parent<R: Model>(mut self) -> Self
    where
        M: crate::HasForeignKey<R>,
    {
        let (my_table, mappings) = M::foreign_key_info();
        let other_table = R::table_name();

        let join_on = mappings
            .iter()
            .map(|(local_col, parent_col)| {
                format!("{}.{} = {}.{}", other_table, parent_col, my_table, local_col)
            })
            .collect::<Vec<_>>()
            .join(" AND ");

        self.joins.push(format!("JOIN {} ON {}", other_table, join_on));
        self
    }

    pub fn group_by(mut self, clause: &str) -> Self {
        self.group_by = Some(clause.into());
        self
    }

    pub fn having<E: IntoExpr<M>>(mut self, expr: E) -> Self {
        self.having.push(expr.into_expr());
        self
    }

    pub fn order_by(mut self, clause: &str) -> Self {
        self.order_by = Some(clause.to_string());
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    fn build_query(&self) -> (String, Vec<&PgValue>) {
        let mut all_params: Vec<&PgValue> = Vec::new();
        let mut param_idx = 1;

        let select_clause = if let Some(exprs) = &self.select_override {
            let mapped: Vec<_> = exprs
                .iter()
                .map(|e| e.resolve(&mut param_idx, &mut all_params))
                .collect();
            mapped.join(", ")
        } else {
            M::columns().join(", ")
        };

        let mut query = format!("SELECT {} FROM {}", select_clause, M::table_name());

        if !self.joins.is_empty() {
            query.push(' ');
            query.push_str(&self.joins.join(" "));
        }

        if !self.filters.is_empty() {
            query.push_str(" WHERE ");
            let filter_strings: Vec<_> = self
                .filters
                .iter()
                .map(|e| e.resolve(&mut param_idx, &mut all_params))
                .collect();
            query.push_str(&filter_strings.join(" AND "));
        }

        if let Some(gb) = &self.group_by {
            query.push_str(" GROUP BY ");
            query.push_str(gb);
        }

        if !self.having.is_empty() {
            query.push_str(" HAVING ");
            let having_strings: Vec<_> = self
                .having
                .iter()
                .map(|e| e.resolve(&mut param_idx, &mut all_params))
                .collect();
            query.push_str(&having_strings.join(" AND "));
        }

        if let Some(order) = &self.order_by {
            query.push_str(" ORDER BY ");
            query.push_str(order);
        }

        if let Some(limit) = self.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = self.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        (query, all_params)
    }

    /// Executes the query and returns raw `Row` results without model mapping.
    pub fn into_raw(self, executor: &mut impl crate::Executor) -> OrmResult<Vec<crate::Row>> {
        let (query, all_params) = self.build_query();
        #[cfg(feature = "log")]
        log::debug!("into_raw: {} | params: {}", query, all_params.len());
        let params_ref: Vec<&dyn chopin_pg::types::ToSql> =
            all_params.iter().map(|p| *p as _).collect();
        executor.query(&query, &params_ref)
    }

    /// Executes the query and returns a list of models.
    pub fn all(self, executor: &mut impl crate::Executor) -> OrmResult<Vec<M>> {
        let (query, all_params) = self.build_query();

        let params_ref: Vec<&dyn chopin_pg::types::ToSql> =
            all_params.iter().map(|p| *p as _).collect();

        let rows = executor.query(&query, &params_ref)?;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(M::from_row(&row)?);
        }
        Ok(result)
    }

    /// Converts this query builder into a `Paginator` using the specified page size.
    pub fn paginate(self, page_size: usize) -> Paginator<M> {
        Paginator::new(self, page_size)
    }

    /// Executes the query, returning the first matching model, or `None` if not found.
    pub fn one(mut self, executor: &mut impl crate::Executor) -> OrmResult<Option<M>> {
        self.limit = Some(1);
        let mut all = self.all(executor)?;
        Ok(all.pop())
    }

    /// Executes a `COUNT(*)` query for the current filters.
    pub fn count(mut self, executor: &mut impl crate::Executor) -> OrmResult<i64> {
        self.select_override = Some(vec![Expr::new("COUNT(*)", vec![])]);
        let (query, all_params) = self.build_query();

        let params_ref: Vec<&dyn chopin_pg::types::ToSql> =
            all_params.iter().map(|p| *p as _).collect();

        let rows = executor.query(&query, &params_ref)?;
        if let Some(row) = rows.first() {
            let val: PgValue = row.get(0).map_err(OrmError::from)?;
            return Ok(match val {
                PgValue::Int8(v) => v,
                PgValue::Int4(v) => v as i64,
                PgValue::Text(s) => s.parse().unwrap_or(0),
                _ => 0,
            });
        }
        Ok(0)
    }
}

/// A paginated result set holding data and metadata (counts).
#[derive(Debug)]
pub struct Page<M> {
    pub items: Vec<M>,
    pub total: i64,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

impl<M> Page<M> {
    /// Returns `true` if there are more pages after this one.
    pub fn has_next(&self) -> bool {
        self.page < self.total_pages
    }

    /// Returns `true` if there are pages before this one.
    pub fn has_prev(&self) -> bool {
        self.page > 1
    }
}

/// An iterator-like coordinator wrapping a `QueryBuilder` for pagination slicing.
#[must_use = "Paginator does nothing until .fetch() is called"]
pub struct Paginator<M> {
    builder: QueryBuilder<M>,
    page_size: usize,
    page: usize,
}

impl<M: Model + Send + Sync> Paginator<M> {
    pub fn new(builder: QueryBuilder<M>, page_size: usize) -> Self {
        Self {
            builder,
            page_size,
            page: 1,
        }
    }

    /// Advances the paginator to the specified page (1-indexed).
    pub fn page(mut self, page: usize) -> Self {
        self.page = page;
        self
    }

    /// Executes the underlying count and slice queries, returning a populated `Page`.
    pub fn fetch(self, executor: &mut impl crate::Executor) -> OrmResult<Page<M>> {
        let total = self.builder.clone().count(executor)?;
        let offset = self.page_size * self.page.saturating_sub(1);

        let items = self
            .builder
            .limit(self.page_size)
            .offset(offset)
            .all(executor)?;

        let total_pages = (total as usize).div_ceil(self.page_size);

        Ok(Page {
            items,
            total,
            page: self.page,
            page_size: self.page_size,
            total_pages,
        })
    }
}

pub trait IntoExpr<M> {
    fn into_expr(self) -> Expr<M>;
}

impl<M> IntoExpr<M> for Expr<M> {
    fn into_expr(self) -> Expr<M> {
        self
    }
}

// For backwards compatibility filter("age = $1", vec![...])
impl<M, S: Into<String>> IntoExpr<M> for (S, Vec<PgValue>) {
    fn into_expr(self) -> Expr<M> {
        Expr::new(self.0.into(), self.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FromRow, Model};
    use chopin_pg::Row;

    struct MockModel {
        pub id: i32,
    }

    impl crate::Validate for MockModel {}

    impl FromRow for MockModel {
        fn from_row(_row: &Row) -> OrmResult<Self> {
            Ok(Self { id: 0 })
        }
    }

    impl Model for MockModel {
        fn table_name() -> &'static str {
            "mocks"
        }
        fn primary_key_columns() -> &'static [&'static str] {
            &["id"]
        }
        fn generated_columns() -> &'static [&'static str] {
            &["id"]
        }
        fn columns() -> &'static [&'static str] {
            &["id", "name"]
        }
        fn primary_key_values(&self) -> Vec<PgValue> {
            vec![PgValue::Int4(self.id)]
        }
        fn set_generated_values(&mut self, mut vals: Vec<PgValue>) -> OrmResult<()> {
            if vals.is_empty() {
                return Ok(());
            }
            if let PgValue::Int4(v) = vals.remove(0) {
                self.id = v;
            }
            Ok(())
        }
        fn get_values(&self) -> Vec<PgValue> {
            vec![]
        }
        fn create_table_stmt() -> String {
            "".into()
        }
        fn column_definitions() -> Vec<(&'static str, &'static str)> {
            vec![]
        }
    }

    enum MockColumn {
        Id,
        Name,
    }

    impl ColumnTrait<MockModel> for MockColumn {
        fn column_name(&self) -> &'static str {
            match self {
                Self::Id => "id",
                Self::Name => "name",
            }
        }
    }

    #[test]
    fn test_query_builder_sql_generation() {
        let qb: QueryBuilder<MockModel> = QueryBuilder::new();
        assert_eq!(qb.build_query().0, "SELECT id, name FROM mocks");

        let qb = QueryBuilder::<MockModel>::new()
            .filter(("name = $1", vec![]))
            .filter(("id > $2", vec![]));
        assert_eq!(
            qb.build_query().0,
            "SELECT id, name FROM mocks WHERE name = $1 AND id > $2"
        );

        let qb = QueryBuilder::<MockModel>::new()
            .order_by("name DESC")
            .limit(10)
            .offset(5);
        assert_eq!(
            qb.build_query().0,
            "SELECT id, name FROM mocks ORDER BY name DESC LIMIT 10 OFFSET 5"
        );
    }

    #[test]
    fn test_order_by_without_where() {
        let qb = QueryBuilder::<MockModel>::new().order_by("id ASC");
        assert_eq!(
            qb.build_query().0,
            "SELECT id, name FROM mocks ORDER BY id ASC"
        );
    }

    #[test]
    fn test_limit_only() {
        let qb = QueryBuilder::<MockModel>::new().limit(20);
        assert_eq!(qb.build_query().0, "SELECT id, name FROM mocks LIMIT 20");
    }

    #[test]
    fn test_offset_only() {
        let qb = QueryBuilder::<MockModel>::new().offset(15);
        assert_eq!(qb.build_query().0, "SELECT id, name FROM mocks OFFSET 15");
    }

    #[test]
    fn test_limit_and_offset_without_where() {
        let qb = QueryBuilder::<MockModel>::new().limit(5).offset(10);
        assert_eq!(
            qb.build_query().0,
            "SELECT id, name FROM mocks LIMIT 5 OFFSET 10"
        );
    }

    #[test]
    fn test_multiple_filters() {
        let qb = QueryBuilder::<MockModel>::new()
            .filter(("id > $1", vec![]))
            .filter(("name = $2", vec![]))
            .filter(("active = $3", vec![]));
        assert_eq!(
            qb.build_query().0,
            "SELECT id, name FROM mocks WHERE id > $1 AND name = $2 AND active = $3"
        );
    }

    #[test]
    fn test_full_query_with_all_clauses() {
        let qb = QueryBuilder::<MockModel>::new()
            .filter(("status = $1", vec![]))
            .order_by("created_at DESC")
            .limit(25)
            .offset(50);
        assert_eq!(
            qb.build_query().0,
            "SELECT id, name FROM mocks WHERE status = $1 ORDER BY created_at DESC LIMIT 25 OFFSET 50"
        );
    }

    #[test]
    fn test_default_equals_new() {
        let qb_default: QueryBuilder<MockModel> = Default::default();
        let qb_new: QueryBuilder<MockModel> = QueryBuilder::new();
        assert_eq!(qb_default.build_query().0, qb_new.build_query().0);
    }

    #[test]
    fn test_no_clauses_is_plain_select() {
        let qb: QueryBuilder<MockModel> = QueryBuilder::new();
        assert_eq!(qb.build_query().0, "SELECT id, name FROM mocks");
    }

    #[test]
    fn test_dsl_generation() {
        let qb = QueryBuilder::<MockModel>::new()
            .filter(MockColumn::Id.gt(10))
            .filter(MockColumn::Name.eq("test"));
        let (sql, _) = qb.build_query();
        assert_eq!(
            sql,
            "SELECT id, name FROM mocks WHERE id > $1 AND name = $2"
        );
    }
}
