use chopin_core::json;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestData {
    id: u32,
    name: String,
    active: bool,
}

#[test]
fn test_to_bytes_and_from_slice() {
    let data = TestData {
        id: 42,
        name: "Test".to_string(),
        active: true,
    };

    // Serialize to bytes
    let bytes = json::to_bytes(&data).expect("Failed to serialize");
    assert!(!bytes.is_empty());

    // Deserialize back
    let deserialized: TestData = json::from_slice(&bytes).expect("Failed to deserialize");
    assert_eq!(data, deserialized);
}

#[test]
fn test_to_string_and_from_str() {
    let data = TestData {
        id: 99,
        name: "String Test".to_string(),
        active: false,
    };

    // Serialize to string
    let json_str = json::to_string(&data).expect("Failed to serialize");
    assert!(json_str.contains("99"));
    assert!(json_str.contains("String Test"));

    // Deserialize back
    let deserialized: TestData = json::from_str(&json_str).expect("Failed to deserialize");
    assert_eq!(data, deserialized);
}

#[test]
fn test_to_writer() {
    let data = TestData {
        id: 7,
        name: "Writer".to_string(),
        active: true,
    };

    let mut buf = Vec::new();
    json::to_writer(&mut buf, &data).expect("Failed to write");
    assert!(!buf.is_empty());

    // Verify the result
    let deserialized: TestData = json::from_slice(&buf).expect("Failed to deserialize");
    assert_eq!(data, deserialized);
}

#[test]
fn test_large_object_serialization() {
    // Test with larger payload to ensure buffer resizing works
    let large_data: Vec<TestData> = (0..100)
        .map(|i| TestData {
            id: i,
            name: format!("User {}", i),
            active: i % 2 == 0,
        })
        .collect();

    let bytes = json::to_bytes(&large_data).expect("Failed to serialize large object");
    assert!(!bytes.is_empty());

    let deserialized: Vec<TestData> = json::from_slice(&bytes).expect("Failed to deserialize");
    assert_eq!(large_data.len(), deserialized.len());
    assert_eq!(large_data[0], deserialized[0]);
    assert_eq!(large_data[99], deserialized[99]);
}

#[test]
fn test_empty_object() {
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Empty {}

    let empty = Empty {};
    let bytes = json::to_bytes(&empty).expect("Failed to serialize empty");
    let deserialized: Empty = json::from_slice(&bytes).expect("Failed to deserialize empty");
    assert_eq!(empty, deserialized);
}

#[test]
fn test_nested_structures() {
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Nested {
        data: TestData,
        metadata: Vec<String>,
    }

    let nested = Nested {
        data: TestData {
            id: 1,
            name: "Nested".to_string(),
            active: true,
        },
        metadata: vec!["tag1".to_string(), "tag2".to_string()],
    };

    let bytes = json::to_bytes(&nested).expect("Failed to serialize nested");
    let deserialized: Nested = json::from_slice(&bytes).expect("Failed to deserialize nested");
    assert_eq!(nested, deserialized);
}

#[test]
fn test_invalid_json_parsing() {
    let invalid_json = b"{ invalid json }";
    let result: Result<TestData, _> = json::from_slice(invalid_json);
    assert!(result.is_err());
}

#[test]
fn test_thread_local_buffer_reuse() {
    // Call to_bytes multiple times to verify buffer reuse works correctly
    for i in 0..10 {
        let data = TestData {
            id: i,
            name: format!("Iteration {}", i),
            active: true,
        };
        let bytes = json::to_bytes(&data).expect("Failed to serialize");
        let deserialized: TestData = json::from_slice(&bytes).expect("Failed to deserialize");
        assert_eq!(data.id, deserialized.id);
    }
}
