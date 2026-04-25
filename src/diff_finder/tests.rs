#[cfg(test)]
mod tests {
    use crate::diff_finder::{Difference, compute_differences};
    use crate::{HttpResponseData, ParsedBody};
    use serde_json::json;
    use std::collections::{HashMap, HashSet};

    fn make_json_response(status_code: u16, json: serde_json::Value) -> HttpResponseData {
        HttpResponseData {
            status_code,
            headers: HashMap::from([("Content-Type".into(), vec!["application/json".into()])]),
            body: ParsedBody {
                json: Some(json),
                ..Default::default()
            },
        }
    }

    #[test]
    fn test_status_code_difference() {
        let response1 = HttpResponseData {
            status_code: 200,
            headers: HashMap::new(),
            body: ParsedBody {
                raw: "".to_string(),
                json: None,
            },
        };

        let response2 = HttpResponseData {
            status_code: 404,
            headers: HashMap::new(),
            body: ParsedBody {
                raw: "".to_string(),
                json: None,
            },
        };

        let differences = compute_differences(&response1, &response2, false, None);

        assert_eq!(differences.len(), 1);
        assert!(matches!(
            differences[0],
            Difference::StatusCodeChanged {
                old_val: 200,
                new_val: 404,
            }
        ));
    }

    #[test]
    fn test_header_differences() {
        let headers1 = HashMap::from([
            ("Content-Type".into(), vec!["application/json".into()]),
            ("X-Test-Header".to_string(), vec!["value1".to_string()]),
        ]);
        let headers2 = HashMap::from([
            (
                "Content-Type".to_string(),
                vec!["application/xml".to_string()],
            ),
            (
                "Authorization".to_string(),
                vec!["Bearer token".to_string()],
            ),
        ]);

        let response1 = HttpResponseData {
            status_code: 200,
            headers: headers1,
            body: ParsedBody {
                raw: "".to_string(),
                json: None,
            },
        };

        let response2 = HttpResponseData {
            status_code: 200,
            headers: headers2,
            body: ParsedBody {
                raw: "".to_string(),
                json: None,
            },
        };

        let differences = compute_differences(&response1, &response2, false, None);

        assert_eq!(differences.len(), 3);

        // Find each type of header difference
        let mut found_changed = false;
        let mut found_removed = false;
        let mut found_added = false;

        for diff in differences {
            match diff {
                Difference::HeaderValueChanged {
                    header_name,
                    old_val,
                    new_val,
                } => {
                    assert_eq!(header_name, "Content-Type");
                    assert_eq!(old_val, vec!["application/json".to_string()]);
                    assert_eq!(new_val, vec!["application/xml".to_string()]);
                    found_changed = true;
                }
                Difference::HeaderValueRemoved { header_name } => {
                    assert_eq!(header_name, "X-Test-Header");
                    found_removed = true;
                }
                Difference::HeaderValueAdded { header_name } => {
                    assert_eq!(header_name, "Authorization");
                    found_added = true;
                }
                _ => panic!("Unexpected difference type"),
            }
        }

        assert!(found_changed, "Missing header value changed difference");
        assert!(found_removed, "Missing header value removed difference");
        assert!(found_added, "Missing header value added difference");
    }

    #[test]
    fn test_headers_ignored() {
        let headers1 = HashMap::from([(
            "Content-Type".to_string(),
            vec!["application/json".to_string()],
        )]);
        let headers2 = HashMap::from([(
            "Content-Type".to_string(),
            vec!["application/xml".to_string()],
        )]);

        let response1 = HttpResponseData {
            status_code: 200,
            headers: headers1,
            body: ParsedBody {
                raw: "".to_string(),
                json: None,
            },
        };

        let response2 = HttpResponseData {
            status_code: 200,
            headers: headers2,
            body: ParsedBody {
                raw: "".to_string(),
                json: None,
            },
        };

        // Ignore headers
        let differences = compute_differences(&response1, &response2, true, None);

        assert_eq!(
            differences.len(),
            0,
            "Differences found when headers should be ignored"
        );
    }

    #[test]
    fn test_json_body_value_changed() {
        let response1 = make_json_response(200, json!({"name": "John", "age": 30}));
        let response2 = make_json_response(200, json!({"name": "John", "age": 31}));

        let differences = compute_differences(&response1, &response2, false, None);

        assert_eq!(differences.len(), 1);
        assert!(matches!(
            differences[0],
            Difference::BodyValueChanged {
                path: _,
                old_val: _,
                new_val: _,
            }
        ));

        if let Difference::BodyValueChanged {
            path,
            old_val,
            new_val,
        } = &differences[0]
        {
            assert_eq!(path, "age");
            assert_eq!(old_val, "30");
            assert_eq!(new_val, "31");
        }
    }

    #[test]
    fn test_json_body_value_added_removed() {
        let response1 =
            make_json_response(200, json!({"name": "John", "email": "john@example.com"}));
        let response2 = make_json_response(200, json!({"name": "John", "phone": "555-1234"}));

        let differences = compute_differences(&response1, &response2, false, None);

        assert_eq!(differences.len(), 2);

        let mut found_removed = false;
        let mut found_added = false;

        for diff in differences {
            match diff {
                Difference::BodyValueRemoved { path, value } => {
                    assert_eq!(path, "email");
                    assert_eq!(value, "\"john@example.com\"");
                    found_removed = true;
                }
                Difference::BodyValueAdded { path, value } => {
                    assert_eq!(path, "phone");
                    assert_eq!(value, "\"555-1234\"");
                    found_added = true;
                }
                _ => panic!("Unexpected difference type"),
            }
        }

        assert!(found_removed, "Missing body value removed difference");
        assert!(found_added, "Missing body value added difference");
    }

    #[test]
    fn test_nested_json_differences() {
        let response1 = make_json_response(
            200,
            json!({"user": {"name": "John", "details": {"age": 30}}}),
        );
        let response2 = make_json_response(
            200,
            json!({"user": {"name": "John", "details": {"age": 31}}}),
        );

        let differences = compute_differences(&response1, &response2, false, None);

        assert_eq!(differences.len(), 1);

        if let Difference::BodyValueChanged {
            path,
            old_val,
            new_val,
        } = &differences[0]
        {
            assert_eq!(path, "user/details/age");
            assert_eq!(old_val, "30");
            assert_eq!(new_val, "31");
        } else {
            panic!("Expected BodyValueChanged difference");
        }
    }

    #[test]
    fn test_array_length_changed() {
        let response1 = make_json_response(200, json!({"items": [1, 2, 3]}));
        let response2 = make_json_response(200, json!({"items": [1, 2, 3, 4, 5]}));

        let differences = compute_differences(&response1, &response2, false, None);

        // There should be 3 differences:
        // 1. Array length changed
        // 2. Element 4 added
        // 3. Element 5 added
        assert_eq!(differences.len(), 3);

        let mut found_length_change = false;
        let mut found_element_added = 0;

        for diff in &differences {
            match diff {
                Difference::ArrayLengthChanged {
                    path,
                    old_len,
                    new_len,
                } => {
                    assert_eq!(path, "items");
                    assert_eq!(*old_len, 3);
                    assert_eq!(*new_len, 5);
                    found_length_change = true;
                }
                Difference::ArrayElementAdded { path, value } => {
                    assert_eq!(path, "items[*]");
                    assert!(value == "4" || value == "5");
                    found_element_added += 1;
                }
                _ => panic!("Unexpected difference type: {:?}", diff),
            }
        }

        assert!(
            found_length_change,
            "Missing array length changed difference"
        );
        assert_eq!(found_element_added, 2, "Should have found 2 added elements");
    }

    #[test]
    fn test_array_element_changed() {
        let response1 = make_json_response(
            200,
            json!({"users": [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]}),
        );
        let response2 = make_json_response(
            200,
            json!({"users": [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bobby"}]}),
        );

        let differences = compute_differences(&response1, &response2, false, None);

        // Order-independent array comparison should show:
        // 1. Element with "Bob" removed
        // 2. Element with "Bobby" added
        assert_eq!(differences.len(), 2);

        let mut found_removed = false;
        let mut found_added = false;

        for diff in &differences {
            match diff {
                Difference::ArrayElementRemoved { path, value } => {
                    assert_eq!(path, "users[*]");
                    assert!(value.contains("Bob"));
                    assert!(!value.contains("Bobby"));
                    found_removed = true;
                }
                Difference::ArrayElementAdded { path, value } => {
                    assert_eq!(path, "users[*]");
                    assert!(value.contains("Bobby"));
                    found_added = true;
                }
                _ => panic!("Unexpected difference type: {:?}", diff),
            }
        }

        assert!(found_removed);
        assert!(found_added);
    }

    #[test]
    fn test_array_order_changed() {
        let response1 = make_json_response(
            200,
            json!({
                "myKey1": "FooBar",
                "myKey2": [
                    {
                        "nestedKey31": "nestedVal31",
                        "nestedKey32": false,
                        "nestedKey33": 6
                    },
                    {
                        "nestedKey11": "nestedVal11",
                        "nestedKey12": false,
                        "nestedKey13": 4
                    },
                    {
                        "nestedKey21": "nestedVal21",
                        "nestedKey22": false,
                        "nestedKey23": 5
                    }
                ]
            }),
        );

        let response2 = make_json_response(
            200,
            json!({
                "myKey2": [
                    {
                        "nestedKey11": "nestedVal11",
                        "nestedKey12": false,
                        "nestedKey13": 4
                    },
                    {
                        "nestedKey21": "nestedVal21",
                        "nestedKey22": false,
                        "nestedKey23": 5
                    },
                    {
                        "nestedKey31": "nestedVal31",
                        "nestedKey32": false,
                        "nestedKey33": 6
                    }
                ],
                "myKey1": "FooBar"
            }),
        );

        let differences = compute_differences(&response1, &response2, false, None);
        assert_eq!(differences.len(), 0, "All differences should be ignored");

        // Now, let's change the value of two keys, the differences should be spotted...
        let response2 = make_json_response(
            200,
            json!({
                "myKey2": [
                    {
                        "nestedKey11": "nestedVal11",
                        "nestedKey12": true,
                        "nestedKey13": 4
                    },
                    {
                        "nestedKey21": "nestedVal21",
                        "nestedKey22": false,
                        "nestedKey23": 5
                    },
                    {
                        "nestedKey31": "nestedVal31",
                        "nestedKey32": false,
                        "nestedKey33": 7
                    }
                ],
                "myKey1": "FooBar"
            }),
        );

        let differences = compute_differences(&response1, &response2, false, None);
        assert_eq!(differences.len(), 4, "The differences should be spotted");
    }

    #[test]
    fn test_non_json_body_difference() {
        let response1 = HttpResponseData {
            status_code: 200,
            headers: HashMap::new(),
            body: ParsedBody {
                raw: "Hello World".to_string(),
                json: None,
            },
        };

        let response2 = HttpResponseData {
            status_code: 200,
            headers: HashMap::new(),
            body: ParsedBody {
                raw: "Hello Universe".to_string(),
                json: None,
            },
        };

        let differences = compute_differences(&response1, &response2, false, None);

        assert_eq!(differences.len(), 1);

        if let Difference::DifferentBodyString { before, after } = &differences[0] {
            assert_eq!(before, "Hello World");
            assert_eq!(after, "Hello Universe");
        } else {
            panic!("Expected DifferentBodyString difference");
        }
    }

    #[test]
    fn test_ignored_paths() {
        let response1 = make_json_response(
            200,
            json!({
                "id": "123",
                "timestamp": "2023-01-01T12:00:00Z",
                "data": {
                    "name": "Test",
                    "value": 42
                }
            }),
        );

        let response2 = make_json_response(
            200,
            json!({
                "id": "456",
                "timestamp": "2023-01-02T12:00:00Z",
                "data": {
                    "name": "Test",
                    "value": 42
                }
            }),
        );

        // Create a set of paths to ignore
        let mut ignored_paths = HashSet::new();
        ignored_paths.insert("/id".to_string());
        ignored_paths.insert("/timestamp".to_string());

        // Compute differences with ignored paths
        let differences = compute_differences(&response1, &response2, false, Some(&ignored_paths));

        // Should find no differences since the only changes are in ignored paths
        assert_eq!(differences.len(), 0);

        // Now ignore only id, should still find timestamp difference
        let mut only_id_ignored = HashSet::new();
        only_id_ignored.insert("/id".to_string());

        let differences =
            compute_differences(&response1, &response2, false, Some(&only_id_ignored));

        assert_eq!(differences.len(), 1);

        if let Difference::BodyValueChanged {
            path,
            old_val,
            new_val,
        } = &differences[0]
        {
            assert_eq!(path, "timestamp");
            assert_eq!(old_val, "\"2023-01-01T12:00:00Z\"");
            assert_eq!(new_val, "\"2023-01-02T12:00:00Z\"");
        } else {
            panic!("Expected BodyValueChanged difference");
        }
    }

    #[test]
    fn test_empty_responses() {
        let empty_response1 = HttpResponseData {
            status_code: 200,
            headers: HashMap::new(),
            body: ParsedBody {
                raw: "".to_string(),
                json: None,
            },
        };

        let empty_response2 = HttpResponseData {
            status_code: 200,
            headers: HashMap::new(),
            body: ParsedBody {
                raw: "".to_string(),
                json: None,
            },
        };

        let differences = compute_differences(&empty_response1, &empty_response2, false, None);
        assert_eq!(
            differences.len(),
            0,
            "Empty responses should have no differences"
        );
    }

    #[test]
    fn test_subpath_ignore() {
        let response1 = make_json_response(
            200,
            json!({
                "data": {
                    "user": {
                        "id": "123",
                        "name": "Alice",
                        "details": {
                            "age": 30,
                            "email": "alice@example.com"
                        }
                    }
                }
            }),
        );

        let response2 = make_json_response(
            200,
            json!({
                "data": {
                    "user": {
                        "id": "456",
                        "name": "Alice",
                        "details": {
                            "age": 31,
                            "email": "alice@example.com"
                        }
                    }
                }
            }),
        );

        // Ignore the entire user path
        let mut ignored_paths = HashSet::new();
        ignored_paths.insert("/data/user".to_string());

        let differences = compute_differences(&response1, &response2, false, Some(&ignored_paths));
        assert_eq!(differences.len(), 0, "All differences should be ignored");

        // Ignore just the user ID
        let mut only_id_ignored = HashSet::new();
        only_id_ignored.insert("/data/user/id".to_string());

        let differences =
            compute_differences(&response1, &response2, false, Some(&only_id_ignored));
        assert_eq!(differences.len(), 1, "Should only find the age difference");

        if let Difference::BodyValueChanged {
            path,
            old_val,
            new_val,
        } = &differences[0]
        {
            assert_eq!(path, "data/user/details/age");
            assert_eq!(old_val, "30");
            assert_eq!(new_val, "31");
        } else {
            panic!("Expected BodyValueChanged difference");
        }
    }

    #[test]
    fn test_multiple_header_values() {
        let headers1 = HashMap::from([(
            "Cache-Control".into(),
            vec!["no-cache".into(), "no-store".into()],
        )]);
        let headers2 = HashMap::from([(
            "Cache-Control".into(),
            vec!["no-cache".into(), "private".into()],
        )]);

        let response1 = HttpResponseData {
            status_code: 200,
            headers: headers1,
            body: ParsedBody::default(),
        };

        let response2 = HttpResponseData {
            status_code: 200,
            headers: headers2,
            body: ParsedBody::default(),
        };

        let differences = compute_differences(&response1, &response2, false, None);

        assert_eq!(differences.len(), 1);
        if let Difference::HeaderValueChanged {
            header_name,
            old_val,
            new_val,
        } = &differences[0]
        {
            assert_eq!(header_name, "Cache-Control");
            assert_eq!(
                old_val,
                &vec!["no-cache".to_string(), "no-store".to_string()]
            );
            assert_eq!(
                new_val,
                &vec!["no-cache".to_string(), "private".to_string()]
            );
        } else {
            panic!("Expected HeaderValueChanged");
        }
    }

    #[test]
    fn test_duplicate_array_elements() {
        let response1 = make_json_response(200, json!([1, 1, 2]));
        let response2 = make_json_response(200, json!([1, 2, 2]));

        let differences = compute_differences(&response1, &response2, false, None);

        // Now implementation correctly detects one '1' removed and one '2' added
        assert_eq!(differences.len(), 2);
        let mut removed_1 = false;
        let mut added_2 = false;

        for diff in differences {
            match diff {
                Difference::ArrayElementRemoved { path: _, value } if value == "1" => {
                    removed_1 = true
                }
                Difference::ArrayElementAdded { path: _, value } if value == "2" => added_2 = true,
                _ => {}
            }
        }
        assert!(removed_1);
        assert!(added_2);
    }

    #[test]
    fn test_format_value_truncation() {
        let long_string = "a".repeat(100);
        let response1 = make_json_response(200, json!({"msg": long_string}));
        let response2 = make_json_response(200, json!({"msg": "short"}));

        let differences = compute_differences(&response1, &response2, false, None);

        assert_eq!(differences.len(), 1);
        if let Difference::BodyValueChanged {
            path: _,
            old_val,
            new_val: _,
        } = &differences[0]
        {
            assert!(old_val.contains("..."));
            assert!(old_val.len() <= 55); // 50 + quotes + ...
        }
    }

    #[test]
    fn test_wildcard_ignore_nested_object_field() {
        // Test ignoring a field in nested objects using wildcard pattern: /data/*/name
        let response1 = make_json_response(
            200,
            json!({
                "data": {
                    "user": {
                        "name": "Alice",
                        "age": 30
                    },
                    "admin": {
                        "name": "Bob",
                        "age": 45
                    }
                }
            }),
        );

        let response2 = make_json_response(
            200,
            json!({
                "data": {
                    "user": {
                        "name": "Charlie",  // Changed
                        "age": 30
                    },
                    "admin": {
                        "name": "David",    // Changed
                        "age": 50           // Changed
                    }
                }
            }),
        );

        let mut ignored_paths = HashSet::new();
        ignored_paths.insert("/data/*/name".to_string());

        let differences = compute_differences(&response1, &response2, false, Some(&ignored_paths));

        // Should only find the age difference for admin, not the name differences
        assert_eq!(differences.len(), 1);
        assert!(matches!(
            differences[0],
            Difference::BodyValueChanged {
                path: ref p,
                old_val: ref ov,
                new_val: ref nv,
            } if p == "data/admin/age" && ov == "45" && nv == "50"
        ));
    }

    #[test]
    fn test_wildcard_ignore_array_elements() {
        // Test ignoring all elements in an array: /users/*/email
        let response1 = make_json_response(
            200,
            json!({
                "users": [
                    {
                        "id": 1,
                        "email": "alice@example.com"
                    },
                    {
                        "id": 2,
                        "email": "bob@example.com"
                    }
                ]
            }),
        );

        let response2 = make_json_response(
            200,
            json!({
                "users": [
                    {
                        "id": 1,
                        "email": "newalice@example.com"  // Email changed (ignored)
                    },
                    {
                        "id": 2,
                        "email": "newbob@example.com"    // Email changed (ignored)
                    }
                ]
            }),
        );

        let mut ignored_paths = HashSet::new();
        ignored_paths.insert("/users/*/email".to_string());

        let differences = compute_differences(&response1, &response2, false, Some(&ignored_paths));

        // Should find no differences since only emails changed and they're ignored
        assert_eq!(differences.len(), 0);
    }

    #[test]
    fn test_wildcard_array_with_id_changes() {
        // Test that non-ignored fields in arrays are still detected
        let response1 = make_json_response(
            200,
            json!({
                "users": [
                    {
                        "id": 1,
                        "email": "alice@example.com"
                    },
                    {
                        "id": 2,
                        "email": "bob@example.com"
                    }
                ]
            }),
        );

        let response2 = make_json_response(
            200,
            json!({
                "users": [
                    {
                        "id": 1,
                        "email": "newalice@example.com"
                    },
                    {
                        "id": 3,                         // ID changed
                        "email": "newbob@example.com"
                    }
                ]
            }),
        );

        let mut ignored_paths = HashSet::new();
        ignored_paths.insert("/users/*/email".to_string());

        let differences = compute_differences(&response1, &response2, false, Some(&ignored_paths));

        // Should find 2 differences: object with id=2 removed, object with id=3 added
        // (emails are ignored, so comparison is based on id field only)
        assert_eq!(differences.len(), 2);
        
        let has_removed = differences.iter().any(|d| matches!(
            d,
            Difference::ArrayElementRemoved { path, value }
            if path == "users[*]" && value.contains("\"id\":2")
        ));
        
        let has_added = differences.iter().any(|d| matches!(
            d,
            Difference::ArrayElementAdded { path, value }
            if path == "users[*]" && value.contains("\"id\":3")
        ));
        
        assert!(has_removed, "Should find array element with id=2 removed");
        assert!(has_added, "Should find array element with id=3 added");
    }

    #[test]
    fn test_wildcard_with_multiple_levels() {
        // Test wildcard at multiple levels: /data/*/items/*/price
        let response1 = make_json_response(
            200,
            json!({
                "data": {
                    "store1": {
                        "items": [
                            {"name": "apple", "price": 1.50},
                            {"name": "banana", "price": 0.75}
                        ]
                    },
                    "store2": {
                        "items": [
                            {"name": "orange", "price": 2.00}
                        ]
                    }
                }
            }),
        );

        let response2 = make_json_response(
            200,
            json!({
                "data": {
                    "store1": {
                        "items": [
                            {"name": "apple", "price": 1.75},     // price changed
                            {"name": "pear", "price": 0.85}       // name changed
                        ]
                    },
                    "store2": {
                        "items": [
                            {"name": "orange", "price": 2.50}     // price changed
                        ]
                    }
                }
            }),
        );

        let mut ignored_paths = HashSet::new();
        ignored_paths.insert("/data/*/items/*/price".to_string());

        let differences = compute_differences(&response1, &response2, false, Some(&ignored_paths));

        // Should find 2 differences (banana removed, pear added) since price is ignored
        // and order-independent array comparison sees these as different elements
        assert_eq!(differences.len(), 2);
        
        let has_removed = differences.iter().any(|d| matches!(
            d,
            Difference::ArrayElementRemoved { path, value }
            if path == "data/store1/items[*]" && value.contains("\"banana\"")
        ));
        
        let has_added = differences.iter().any(|d| matches!(
            d,
            Difference::ArrayElementAdded { path, value }
            if path == "data/store1/items[*]" && value.contains("\"pear\"")
        ));
        
        assert!(has_removed, "Should find array element with banana removed");
        assert!(has_added, "Should find array element with pear added");
    }

    #[test]
    fn test_wildcard_and_exact_paths_together() {
        // Test mixing wildcard and exact paths
        let response1 = make_json_response(
            200,
            json!({
                "id": "123",
                "timestamp": "2024-01-01",
                "data": {
                    "user1": {"name": "Alice", "email": "alice@example.com"},
                    "user2": {"name": "Bob", "email": "bob@example.com"}
                }
            }),
        );

        let response2 = make_json_response(
            200,
            json!({
                "id": "456",              // changed
                "timestamp": "2024-01-02", // changed
                "data": {
                    "user1": {"name": "Alice", "email": "newalice@example.com"}, // email changed
                    "user2": {"name": "Charlie", "email": "newbob@example.com"}  // both changed
                }
            }),
        );

        let mut ignored_paths = HashSet::new();
        ignored_paths.insert("/id".to_string());           // exact path
        ignored_paths.insert("/data/*/email".to_string()); // wildcard path

        let differences = compute_differences(&response1, &response2, false, Some(&ignored_paths));

        // Should find timestamp and user2 name differences only
        assert_eq!(differences.len(), 2);

        let has_timestamp = differences.iter().any(|d| matches!(
            d,
            Difference::BodyValueChanged {
                path,
                old_val: _,
                new_val: _,
            } if path == "timestamp"
        ));

        let has_name = differences.iter().any(|d| matches!(
            d,
            Difference::BodyValueChanged {
                path,
                old_val,
                new_val,
            } if path == "data/user2/name" && old_val == "\"Bob\"" && new_val == "\"Charlie\""
        ));

        assert!(has_timestamp, "Should find timestamp difference");
        assert!(has_name, "Should find user2 name difference");
    }

    #[test]
    fn test_exact_paths_still_work() {
        // Ensure exact paths still work without wildcards
        let response1 = make_json_response(
            200,
            json!({
                "id": "123",
                "name": "Alice",
                "timestamp": "2024-01-01"
            }),
        );

        let response2 = make_json_response(
            200,
            json!({
                "id": "456",
                "name": "Bob",
                "timestamp": "2024-01-02"
            }),
        );

        let mut ignored_paths = HashSet::new();
        ignored_paths.insert("/id".to_string());
        ignored_paths.insert("/timestamp".to_string());

        let differences = compute_differences(&response1, &response2, false, Some(&ignored_paths));

        // Should only find name difference
        assert_eq!(differences.len(), 1);
        assert!(matches!(
            differences[0],
            Difference::BodyValueChanged {
                path: ref p,
                old_val: ref ov,
                new_val: ref nv,
            } if p == "name" && ov == "\"Alice\"" && nv == "\"Bob\""
        ));
    }

    #[test]
    fn test_non_matching_wildcard() {
        // Ensure non-matching wildcard patterns don't ignore anything
        let response1 = make_json_response(
            200,
            json!({
                "user": {
                    "name": "Alice",
                    "age": 30
                }
            }),
        );

        let response2 = make_json_response(
            200,
            json!({
                "user": {
                    "name": "Bob",
                    "age": 35
                }
            }),
        );

        let mut ignored_paths = HashSet::new();
        ignored_paths.insert("/admin/*/name".to_string()); // Non-matching pattern

        let differences = compute_differences(&response1, &response2, false, Some(&ignored_paths));

        // Should find both differences since the pattern doesn't match
        assert_eq!(differences.len(), 2);
    }

    #[test]
    fn test_wildcard_ignore_entire_subtree() {
        // Test ignoring an entire subtree with wildcard: /data/*
        let response1 = make_json_response(
            200,
            json!({
                "id": "123",
                "data": {
                    "user": {"name": "Alice", "age": 30},
                    "settings": {"theme": "dark", "lang": "en"}
                }
            }),
        );

        let response2 = make_json_response(
            200,
            json!({
                "id": "123",
                "data": {
                    "user": {"name": "Bob", "age": 35},
                    "settings": {"theme": "light", "lang": "fr"}
                }
            }),
        );

        let mut ignored_paths = HashSet::new();
        ignored_paths.insert("/data/*".to_string());

        let differences = compute_differences(&response1, &response2, false, Some(&ignored_paths));

        // Should find no differences since all changes are under /data/*
        assert_eq!(differences.len(), 0, "All differences should be ignored");
    }
}
