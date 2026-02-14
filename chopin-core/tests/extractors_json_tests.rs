use axum::{
    body::Body,
    extract::FromRequest,
    http::{Request, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use chopin::extractors::json::Json;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestPayload {
    name: String,
    age: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct EmptyPayload {}

#[tokio::test]
async fn test_valid_json_extraction() {
    let payload = TestPayload {
        name: "Alice".to_string(),
        age: 30,
    };

    let json_str = serde_json::to_string(&payload).unwrap();
    let req = Request::builder()
        .header("content-type", "application/json")
        .body(Body::from(json_str))
        .unwrap();

    let result = Json::<TestPayload>::from_request(req, &()).await;

    assert!(result.is_ok());
    let Json(extracted) = result.unwrap();
    assert_eq!(extracted, payload);
}

#[tokio::test]
async fn test_invalid_json_fails() {
    let req = Request::builder()
        .header("content-type", "application/json")
        .body(Body::from("{invalid json}"))
        .unwrap();

    let result = Json::<TestPayload>::from_request(req, &()).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_missing_required_fields_fails() {
    let req = Request::builder()
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name": "Bob"}"#))
        .unwrap();

    let result = Json::<TestPayload>::from_request(req, &()).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_extra_fields_are_ignored() {
    let req = Request::builder()
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name": "Charlie", "age": 25, "extra": "ignored"}"#,
        ))
        .unwrap();

    let result = Json::<TestPayload>::from_request(req, &()).await;

    assert!(result.is_ok());
    let Json(extracted) = result.unwrap();
    assert_eq!(extracted.name, "Charlie");
    assert_eq!(extracted.age, 25);
}

#[tokio::test]
async fn test_empty_body_fails() {
    let req = Request::builder()
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let result = Json::<TestPayload>::from_request(req, &()).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_empty_object_json() {
    let req = Request::builder()
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let result = Json::<EmptyPayload>::from_request(req, &()).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_json_with_unicode() {
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct UnicodePayload {
        text: String,
    }

    let payload = UnicodePayload {
        text: "Hello 世界 🌍".to_string(),
    };

    let json_str = serde_json::to_string(&payload).unwrap();
    let req = Request::builder()
        .header("content-type", "application/json")
        .body(Body::from(json_str))
        .unwrap();

    let result = Json::<UnicodePayload>::from_request(req, &()).await;

    assert!(result.is_ok());
    let Json(extracted) = result.unwrap();
    assert_eq!(extracted, payload);
}

#[tokio::test]
async fn test_json_with_nested_structures() {
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Address {
        city: String,
        zip: String,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Person {
        name: String,
        address: Address,
    }

    let payload = Person {
        name: "David".to_string(),
        address: Address {
            city: "New York".to_string(),
            zip: "10001".to_string(),
        },
    };

    let json_str = serde_json::to_string(&payload).unwrap();
    let req = Request::builder()
        .header("content-type", "application/json")
        .body(Body::from(json_str))
        .unwrap();

    let result = Json::<Person>::from_request(req, &()).await;

    assert!(result.is_ok());
    let Json(extracted) = result.unwrap();
    assert_eq!(extracted, payload);
}

#[tokio::test]
async fn test_json_with_arrays() {
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct ListPayload {
        items: Vec<String>,
    }

    let payload = ListPayload {
        items: vec![
            "apple".to_string(),
            "banana".to_string(),
            "cherry".to_string(),
        ],
    };

    let json_str = serde_json::to_string(&payload).unwrap();
    let req = Request::builder()
        .header("content-type", "application/json")
        .body(Body::from(json_str))
        .unwrap();

    let result = Json::<ListPayload>::from_request(req, &()).await;

    assert!(result.is_ok());
    let Json(extracted) = result.unwrap();
    assert_eq!(extracted, payload);
}

#[tokio::test]
async fn test_json_response_serialization() {
    let payload = TestPayload {
        name: "Eve".to_string(),
        age: 28,
    };

    let json_response = Json(payload.clone());
    let response = json_response.into_response();

    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response.headers().get("content-type").unwrap();
    assert_eq!(content_type, "application/json");

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let deserialized: TestPayload = serde_json::from_slice(&body).unwrap();

    assert_eq!(deserialized, payload);
}

#[tokio::test]
async fn test_json_response_with_empty_object() {
    let payload = EmptyPayload {};

    let json_response = Json(payload);
    let response = json_response.into_response();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    assert_eq!(body_str, "{}");
}

#[tokio::test]
async fn test_json_with_numbers() {
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct NumberPayload {
        int: i64,
        float: f64,
    }

    let payload = NumberPayload {
        int: 9223372036854775807,
        float: std::f64::consts::PI,
    };

    let json_str = serde_json::to_string(&payload).unwrap();
    let req = Request::builder()
        .header("content-type", "application/json")
        .body(Body::from(json_str))
        .unwrap();

    let result = Json::<NumberPayload>::from_request(req, &()).await;

    assert!(result.is_ok());
    let Json(extracted) = result.unwrap();
    assert_eq!(extracted.int, payload.int);
    assert!((extracted.float - payload.float).abs() < 1e-10);
}

#[tokio::test]
async fn test_json_with_boolean() {
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct BoolPayload {
        is_active: bool,
        is_verified: bool,
    }

    let payload = BoolPayload {
        is_active: true,
        is_verified: false,
    };

    let json_str = serde_json::to_string(&payload).unwrap();
    let req = Request::builder()
        .header("content-type", "application/json")
        .body(Body::from(json_str))
        .unwrap();

    let result = Json::<BoolPayload>::from_request(req, &()).await;

    assert!(result.is_ok());
    let Json(extracted) = result.unwrap();
    assert_eq!(extracted, payload);
}

#[tokio::test]
async fn test_json_null_values() {
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct NullablePayload {
        name: String,
        optional: Option<String>,
    }

    let req = Request::builder()
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name": "Frank", "optional": null}"#))
        .unwrap();

    let result = Json::<NullablePayload>::from_request(req, &()).await;

    assert!(result.is_ok());
    let Json(extracted) = result.unwrap();
    assert_eq!(extracted.name, "Frank");
    assert_eq!(extracted.optional, None);
}

#[tokio::test]
async fn test_large_json_payload() {
    #[derive(Debug, Serialize, Deserialize)]
    struct LargePayload {
        data: Vec<i32>,
    }

    let payload = LargePayload {
        data: (0..10000).collect(),
    };

    let json_str = serde_json::to_string(&payload).unwrap();
    let req = Request::builder()
        .header("content-type", "application/json")
        .body(Body::from(json_str))
        .unwrap();

    let result = Json::<LargePayload>::from_request(req, &()).await;

    assert!(result.is_ok());
    let Json(extracted) = result.unwrap();
    assert_eq!(extracted.data.len(), 10000);
}

#[tokio::test]
async fn test_wrong_type_fails() {
    let req = Request::builder()
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name": 123, "age": "not a number"}"#))
        .unwrap();

    let result = Json::<TestPayload>::from_request(req, &()).await;

    assert!(result.is_err());
}
