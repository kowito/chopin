use chopin_core::response::ApiResponse;

// ═══ ApiResponse success ═══

#[test]
fn test_api_response_success_with_string() {
    let resp = ApiResponse::success("hello".to_string());
    assert!(resp.success);
    assert!(resp.data.is_some());
    assert_eq!(resp.data.unwrap(), "hello");
    assert!(resp.error.is_none());
}

#[test]
fn test_api_response_success_with_struct() {
    #[derive(Debug, Clone, serde::Serialize)]
    struct Item {
        id: i32,
        name: String,
    }

    let item = Item {
        id: 1,
        name: "test".to_string(),
    };
    let resp = ApiResponse::success(item);
    assert!(resp.success);
    assert!(resp.data.is_some());
    assert!(resp.error.is_none());
}

#[test]
fn test_api_response_success_with_vec() {
    let items = vec![1, 2, 3];
    let resp = ApiResponse::success(items);
    assert!(resp.success);
    assert!(resp.data.is_some());
    assert_eq!(resp.data.unwrap().len(), 3);
}

// ═══ ApiResponse serialization ═══

#[test]
fn test_api_response_success_json() {
    let resp = ApiResponse::success("ok".to_string());
    let json = serde_json::to_string(&resp).expect("serialize");
    assert!(json.contains("\"success\":true"));
    assert!(json.contains("\"data\":\"ok\""));
}

#[test]
fn test_api_response_success_no_error_in_json() {
    let resp = ApiResponse::success(42);
    let json = serde_json::to_string(&resp).expect("serialize");
    // error field should either be null or absent
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
    let error = parsed.get("error");
    assert!(error.is_none() || error == Some(&serde_json::Value::Null));
}

// ═══ ApiResponse with Option<T> ═══

#[test]
fn test_api_response_with_none_data() {
    let resp: ApiResponse<String> = ApiResponse {
        success: true,
        data: None,
        error: None,
    };
    assert!(resp.success);
    assert!(resp.data.is_none());
}

// ═══ ApiResponse error format ═══

#[test]
fn test_api_response_error_format() {
    use chopin_core::error::ErrorDetail;

    let resp: ApiResponse<()> = ApiResponse {
        success: false,
        data: None,
        error: Some(ErrorDetail {
            code: "NOT_FOUND".to_string(),
            message: "User not found".to_string(),
            fields: None,
        }),
    };

    assert!(!resp.success);
    assert!(resp.data.is_none());
    assert!(resp.error.is_some());
    let error = resp.error.unwrap();
    assert_eq!(error.code, "NOT_FOUND");
    assert_eq!(error.message, "User not found");
}
