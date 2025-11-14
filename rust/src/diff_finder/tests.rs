#[cfg(test)]
mod tests {
    use crate::diff_finder::{Difference, compute_differences};
    use crate::{HttpResponseData, ParsedBody};
    use serde_json::json;
    use std::collections::{HashMap, HashSet};

    fn make_json_response(status_code: u16, json: serde_json::Value) -> HttpResponseData {
        HttpResponseData {
            status_code,
            headers: HashMap::from([("Content-Type".into(), "application/json".into())]),
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
            ("Content-Type".into(), "application/json".into()),
            ("X-Test-Header".to_string(), "value1".to_string()),
        ]);
        let headers2 = HashMap::from([
            ("Content-Type".to_string(), "application/xml".to_string()),
            ("Authorization".to_string(), "Bearer token".to_string()),
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
                    assert_eq!(old_val, "application/json");
                    assert_eq!(new_val, "application/xml");
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
        let headers1 =
            HashMap::from([("Content-Type".to_string(), "application/json".to_string())]);
        let headers2 =
            HashMap::from([("Content-Type".to_string(), "application/xml".to_string())]);

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
        assert_eq!(differences.len(), 2, "The differences should be spotted");
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
}
