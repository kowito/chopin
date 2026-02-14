use axum::http::StatusCode;
use chopin::error::{ChopinError, ErrorDetail, FieldError};

// ═══ Status codes for all variants ═══

#[test]
fn test_not_found_status_code() {
    let err = ChopinError::NotFound("thing".into());
    assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
}

#[test]
fn test_bad_request_status_code() {
    let err = ChopinError::BadRequest("bad".into());
    assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_unauthorized_status_code() {
    let err = ChopinError::Unauthorized("no auth".into());
    assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_forbidden_status_code() {
    let err = ChopinError::Forbidden("denied".into());
    assert_eq!(err.status_code(), StatusCode::FORBIDDEN);
}

#[test]
fn test_conflict_status_code() {
    let err = ChopinError::Conflict("duplicate".into());
    assert_eq!(err.status_code(), StatusCode::CONFLICT);
}

#[test]
fn test_validation_status_code() {
    let err = ChopinError::Validation("invalid".into());
    assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn test_validation_errors_status_code() {
    let err = ChopinError::ValidationErrors(vec![]);
    assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn test_internal_status_code() {
    let err = ChopinError::Internal("oops".into());
    assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_database_status_code() {
    let db_err = sea_orm::DbErr::Custom("test".into());
    let err = ChopinError::Database(db_err);
    assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ═══ Error codes for all variants ═══

#[test]
fn test_not_found_error_code() {
    let err = ChopinError::NotFound("x".into());
    assert_eq!(err.error_code(), "NOT_FOUND");
}

#[test]
fn test_bad_request_error_code() {
    let err = ChopinError::BadRequest("x".into());
    assert_eq!(err.error_code(), "BAD_REQUEST");
}

#[test]
fn test_unauthorized_error_code() {
    let err = ChopinError::Unauthorized("x".into());
    assert_eq!(err.error_code(), "UNAUTHORIZED");
}

#[test]
fn test_forbidden_error_code() {
    let err = ChopinError::Forbidden("x".into());
    assert_eq!(err.error_code(), "FORBIDDEN");
}

#[test]
fn test_conflict_error_code() {
    let err = ChopinError::Conflict("x".into());
    assert_eq!(err.error_code(), "CONFLICT");
}

#[test]
fn test_validation_error_code() {
    let err = ChopinError::Validation("x".into());
    assert_eq!(err.error_code(), "VALIDATION_ERROR");
}

#[test]
fn test_validation_errors_error_code() {
    let err = ChopinError::ValidationErrors(vec![]);
    assert_eq!(err.error_code(), "VALIDATION_ERROR");
}

#[test]
fn test_internal_error_code() {
    let err = ChopinError::Internal("x".into());
    assert_eq!(err.error_code(), "INTERNAL_ERROR");
}

#[test]
fn test_database_error_code() {
    let db_err = sea_orm::DbErr::Custom("test".into());
    let err = ChopinError::Database(db_err);
    assert_eq!(err.error_code(), "DATABASE_ERROR");
}

// ═══ Display / to_string ═══

#[test]
fn test_not_found_display() {
    let err = ChopinError::NotFound("User 42".into());
    assert_eq!(err.to_string(), "Not found: User 42");
}

#[test]
fn test_bad_request_display() {
    let err = ChopinError::BadRequest("missing field".into());
    assert_eq!(err.to_string(), "Bad request: missing field");
}

#[test]
fn test_unauthorized_display() {
    let err = ChopinError::Unauthorized("expired token".into());
    assert_eq!(err.to_string(), "Unauthorized: expired token");
}

#[test]
fn test_internal_display() {
    let err = ChopinError::Internal("panic".into());
    assert_eq!(err.to_string(), "Internal server error: panic");
}

#[test]
fn test_validation_errors_display() {
    let err = ChopinError::ValidationErrors(vec![FieldError::new("email", "invalid")]);
    assert_eq!(err.to_string(), "Validation errors");
}

// ═══ FieldError ═══

#[test]
fn test_field_error_new() {
    let fe = FieldError::new("email", "must be valid");
    assert_eq!(fe.field, "email");
    assert_eq!(fe.message, "must be valid");
    assert!(fe.code.is_none());
}

#[test]
fn test_field_error_with_code() {
    let fe = FieldError::with_code("password", "too short", "min_length");
    assert_eq!(fe.field, "password");
    assert_eq!(fe.message, "too short");
    assert_eq!(fe.code, Some("min_length".to_string()));
}

#[test]
fn test_field_error_new_with_string_args() {
    let field = String::from("username");
    let message = String::from("already taken");
    let fe = FieldError::new(field, message);
    assert_eq!(fe.field, "username");
    assert_eq!(fe.message, "already taken");
}

#[test]
fn test_field_error_clone() {
    let fe = FieldError::with_code("name", "required", "required_field");
    let cloned = fe.clone();
    assert_eq!(fe.field, cloned.field);
    assert_eq!(fe.message, cloned.message);
    assert_eq!(fe.code, cloned.code);
}

#[test]
fn test_field_error_serialization() {
    let fe = FieldError::new("email", "invalid format");
    let json = serde_json::to_string(&fe).expect("serialize");
    assert!(json.contains("\"field\":\"email\""));
    assert!(json.contains("\"message\":\"invalid format\""));
    // code is None → should be skipped
    assert!(!json.contains("\"code\""));
}

#[test]
fn test_field_error_with_code_serialization() {
    let fe = FieldError::with_code("age", "must be positive", "positive");
    let json = serde_json::to_string(&fe).expect("serialize");
    assert!(json.contains("\"code\":\"positive\""));
}

// ═══ validation_fields constructor ═══

#[test]
fn test_validation_fields_constructor() {
    let errors = vec![
        FieldError::new("email", "required"),
        FieldError::new("password", "too short"),
    ];
    let err = ChopinError::validation_fields(errors);
    assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err.error_code(), "VALIDATION_ERROR");
}

// ═══ ErrorDetail serialization ═══

#[test]
fn test_error_detail_without_fields() {
    let detail = ErrorDetail {
        code: "NOT_FOUND".to_string(),
        message: "User not found".to_string(),
        fields: None,
    };
    let json = serde_json::to_string(&detail).expect("serialize");
    assert!(json.contains("\"code\":\"NOT_FOUND\""));
    assert!(json.contains("\"message\":\"User not found\""));
    // fields is None → should be skipped
    assert!(!json.contains("\"fields\""));
}

#[test]
fn test_error_detail_with_fields() {
    let detail = ErrorDetail {
        code: "VALIDATION_ERROR".to_string(),
        message: "Validation failed".to_string(),
        fields: Some(vec![FieldError::new("email", "required")]),
    };
    let json = serde_json::to_string(&detail).expect("serialize");
    assert!(json.contains("\"fields\""));
    assert!(json.contains("\"email\""));
}

// ═══ IntoResponse ═══

#[test]
fn test_into_response_not_found() {
    use axum::response::IntoResponse;
    let err = ChopinError::NotFound("User 99".into());
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_into_response_bad_request() {
    use axum::response::IntoResponse;
    let err = ChopinError::BadRequest("missing param".into());
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_into_response_validation_errors() {
    use axum::response::IntoResponse;
    let err = ChopinError::ValidationErrors(vec![
        FieldError::new("email", "invalid"),
        FieldError::with_code("name", "required", "required"),
    ]);
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn test_into_response_database_error() {
    use axum::response::IntoResponse;
    let db_err = sea_orm::DbErr::Custom("connection lost".into());
    let err = ChopinError::Database(db_err);
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ═══ From<DbErr> conversion ═══

#[test]
fn test_from_db_err() {
    let db_err = sea_orm::DbErr::Custom("test error".into());
    let err: ChopinError = db_err.into();
    assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(err.error_code(), "DATABASE_ERROR");
}
