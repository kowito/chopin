use chopin_core::extractors::{PaginatedResponse, Pagination};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestItem {
    id: u32,
    name: String,
}

#[test]
fn test_pagination_defaults() {
    let pagination = Pagination::default();
    assert_eq!(pagination.limit, 20);
    assert_eq!(pagination.offset, 0);
    assert!(pagination.page.is_none());
    assert!(pagination.per_page.is_none());
}

#[test]
fn test_pagination_clamped() {
    // Test with offset-based
    let pagination = Pagination {
        limit: 150, // Exceeds max
        offset: 5,
        page: None,
        per_page: None,
    };
    let clamped = pagination.clamped();
    assert_eq!(clamped.limit, 100); // Should be clamped to max
    assert_eq!(clamped.offset, 5);

    // Test with page-based
    let pagination = Pagination {
        limit: 20,
        offset: 0,
        page: Some(2),
        per_page: Some(15),
    };
    let clamped = pagination.clamped();
    assert_eq!(clamped.limit, 15);
    assert_eq!(clamped.offset, 15); // page 2, 15 per page = offset 15
}

#[test]
fn test_pagination_page_conversion() {
    let pagination = Pagination {
        limit: 20,
        offset: 0,
        page: Some(3),
        per_page: Some(10),
    };
    let clamped = pagination.clamped();
    assert_eq!(clamped.offset, 20); // page 3, 10 per page = skip 20
    assert_eq!(clamped.limit, 10);
}

#[test]
fn test_pagination_zero_page_treated_as_first() {
    let pagination = Pagination {
        limit: 20,
        offset: 0,
        page: Some(0),
        per_page: Some(10),
    };
    let clamped = pagination.clamped();
    assert_eq!(clamped.offset, 0); // page 0 = page 1
    assert_eq!(clamped.limit, 10);
}

#[test]
fn test_pagination_large_page_number() {
    let pagination = Pagination {
        limit: 20,
        offset: 0,
        page: Some(100),
        per_page: Some(25),
    };
    let clamped = pagination.clamped();
    assert_eq!(clamped.offset, 2475); // (100-1) * 25
    assert_eq!(clamped.limit, 25);
}

#[test]
fn test_pagination_zero_limit_clamped_to_min() {
    let pagination = Pagination {
        limit: 0,
        offset: 0,
        page: None,
        per_page: None,
    };
    let clamped = pagination.clamped();
    assert_eq!(clamped.limit, 1); // Minimum limit
}

#[test]
fn test_paginated_response_creation() {
    let items = vec![
        TestItem {
            id: 1,
            name: "Item 1".to_string(),
        },
        TestItem {
            id: 2,
            name: "Item 2".to_string(),
        },
    ];

    let pagination = Pagination {
        limit: 10,
        offset: 0,
        page: None,
        per_page: None,
    }
    .clamped();

    let response = PaginatedResponse::new(items.clone(), 50, &pagination);

    assert_eq!(response.items, items);
    assert_eq!(response.total, 50);
    assert_eq!(response.per_page, 10);
    assert_eq!(response.page, 1);
    assert_eq!(response.total_pages, 5); // 50 items / 10 per page
}

#[test]
fn test_paginated_response_page_calculation() {
    let items: Vec<TestItem> = vec![];
    let pagination = Pagination {
        limit: 10,
        offset: 20,
        page: None,
        per_page: None,
    }
    .clamped();

    let response = PaginatedResponse::new(items, 100, &pagination);

    assert_eq!(response.page, 3); // offset 20 / limit 10 + 1 = page 3
    assert_eq!(response.total_pages, 10); // 100 items / 10 per page
}

#[test]
fn test_paginated_response_last_partial_page() {
    let items: Vec<TestItem> = vec![];
    let pagination = Pagination {
        limit: 10,
        offset: 0,
        page: None,
        per_page: None,
    }
    .clamped();

    let response = PaginatedResponse::new(items, 95, &pagination);

    assert_eq!(response.total_pages, 10); // 95 items needs 10 pages (9 full + 1 partial)
}

#[test]
fn test_paginated_response_empty_results() {
    let items: Vec<TestItem> = vec![];
    let pagination = Pagination::default().clamped();

    let response = PaginatedResponse::new(items, 0, &pagination);

    assert_eq!(response.total, 0);
    assert_eq!(response.total_pages, 0);
    assert_eq!(response.page, 1);
    assert!(response.items.is_empty());
}

#[test]
fn test_paginated_response_with_page_based_query() {
    let items = vec![TestItem {
        id: 1,
        name: "Test".to_string(),
    }];

    let pagination = Pagination {
        limit: 20,
        offset: 0,
        page: Some(5),
        per_page: Some(20),
    }
    .clamped();

    let response = PaginatedResponse::new(items, 200, &pagination);

    assert_eq!(response.page, 5);
    assert_eq!(response.per_page, 20);
    assert_eq!(response.total_pages, 10); // 200 / 20
}

#[test]
fn test_pagination_default_creation() {
    let pagination = Pagination::default();
    assert_eq!(pagination.limit, 20);
    assert_eq!(pagination.offset, 0);
}

#[test]
fn test_paginated_response_serialization() {
    let items = vec![TestItem {
        id: 1,
        name: "Test".to_string(),
    }];

    let pagination = Pagination::default().clamped();
    let response = PaginatedResponse::new(items, 100, &pagination);

    let json = serde_json::to_string(&response).expect("Failed to serialize");
    assert!(json.contains("\"total\":100"));
    assert!(json.contains("\"page\":1"));
    assert!(json.contains("\"total_pages\":5"));
    assert!(json.contains("\"per_page\":20"));
}
