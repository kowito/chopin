use crate::{Executor, Model, OrmResult};

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveValue<T> {
    Set(T),
    Unchanged(T),
    NotSet,
}

impl<T> ActiveValue<T> {
    pub fn is_set(&self) -> bool {
        matches!(self, ActiveValue::Set(_))
    }

    pub fn is_unchanged(&self) -> bool {
        matches!(self, ActiveValue::Unchanged(_))
    }

    pub fn is_not_set(&self) -> bool {
        matches!(self, ActiveValue::NotSet)
    }

    pub fn take(self) -> Option<T> {
        match self {
            ActiveValue::Set(v) => Some(v),
            ActiveValue::Unchanged(v) => Some(v),
            ActiveValue::NotSet => None,
        }
    }

    pub fn as_ref(&self) -> Option<&T> {
        match self {
            ActiveValue::Set(v) => Some(v),
            ActiveValue::Unchanged(v) => Some(v),
            ActiveValue::NotSet => None,
        }
    }
}

pub trait ActiveModelTrait: Sized + Send + Sync {
    type Entity: Model;

    /// Update the model in the database matching its primary key.
    /// Only updates fields that are `ActiveValue::Set`.
    fn update(&self, executor: &mut impl Executor) -> OrmResult<Self::Entity>;
}
