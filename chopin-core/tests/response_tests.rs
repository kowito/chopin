use chopin::response::ApiResponse;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestData {
    id: u32,
    name: String,
}

#[test]
fn test_success_response() {
    let data = TestData {
        id: 1,
        name: "Test".to_string(),
    };

    let response = ApiResponse::success(data.clone());

    assert!(response.success);
    assert!(response.data.is_some());
    assert_eq!(response.data.unwrap(), data);
    assert!(response.error.is_none());
}

#[test]
fn test_ok_response() {
    let data = TestData {
        id: 2,
        name: "OK Test".to_string(),
    };

    let response = ApiResponse::success(data.clone());

    assert!(response.success);
    assert!(response.data.is_some());
    assert_eq!(response.data.unwrap(), data);
    assert!(response.error.is_none());
}

#[test]
fn test_created_response() {
    let data = TestData {
        id: 3,
        name: "Created".to_string(),
    };

    let response = ApiResponse::success(data.clone());

    assert!(response.success);
    assert!(response.data.is_some());
    assert_eq!(response.data.unwrap(), data);
    assert!(response.error.is_none());
}

#[test]
fn test_error_response() {
    let response: ApiResponse<TestData> = ApiResponse::error("ERR_TEST", "Test error message");

    assert!(!response.success);
    assert!(response.data.is_none());
    assert!(response.error.is_some());

    let error = response.error.unwrap();
    assert_eq!(error.code, "ERR_TEST");
    assert_eq!(error.message, "Test error message");
    assert!(error.fields.is_none());
}

#[test]
fn test_error_with_string_conversion() {
    let error_code = String::from("AUTH_ERROR");
    let error_msg = String::from("Authentication failed");
    let response: ApiResponse<TestData> = ApiResponse::error(error_code, error_msg);

    assert!(!response.success);
    let error = response.error.unwrap();
    assert_eq!(error.code, "AUTH_ERROR");
    assert_eq!(error.message, "Authentication failed");
}

#[test]
fn test_response_serialization() {
    let data = TestData {
        id: 42,
        name: "Serialize Test".to_string(),
    };

    let response = ApiResponse::success(data);
    let json = serde_json::to_string(&response).expect("Failed to serialize");

    assert!(json.contains("\"success\":true"));
    assert!(json.contains("\"id\":42"));
    assert!(json.contains("\"name\":\"Serialize Test\""));
    assert!(!json.contains("\"error\""));
}

#[test]
fn test_error_response_serialization() {
    let response: ApiResponse<()> = ApiResponse::error("NOT_FOUND", "Resource not found");
    let json = serde_json::to_string(&response).expect("Failed to serialize");

    assert!(json.contains("\"success\":false"));
    assert!(json.contains("\"code\":\"NOT_FOUND\""));
    assert!(json.contains("\"message\":\"Resource not found\""));
    assert!(!json.contains("\"data\""));
}

#[test]
fn test_empty_data_response() {
    let response = ApiResponse::success(());
    assert!(response.success);
    assert!(response.data.is_some());
}

#[test]
fn test_vec_data_response() {
    let data = vec![
        TestData {
            id: 1,
            name: "First".to_string(),
        },
        TestData {
            id: 2,
            name: "Second".to_string(),
        },
    ];

    let response = ApiResponse::success(data.clone());
    assert!(response.success);
    assert!(response.data.is_some());
    assert_eq!(response.data.unwrap().len(), 2);
}

#[test]
fn test_option_data_response() {
    let data: Option<TestData> = Some(TestData {
        id: 1,
        name: "Optional".to_string(),
    });

    let response = ApiResponse::success(data);
    assert!(response.success);
    assert!(response.data.is_some());
}

#[test]
fn test_response_with_none_option() {
    let data: Option<TestData> = None;
    let response = ApiResponse::success(data);
    assert!(response.success);
    assert!(response.data.is_some());
    assert!(response.data.unwrap().is_none());
}
