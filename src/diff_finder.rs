use std::collections::{HashMap, HashSet};

use colored::Colorize;
use serde_json::Value;

use crate::HttpResponseData;

/// Represents a difference found in JSON structures
#[derive(Debug)]
pub enum Difference {
    StatusCodeChanged {
        old_val: u16,
        new_val: u16,
    },
    HeaderValueChanged {
        header_name: String,
        old_val: String,
        new_val: String,
    },
    HeaderValueRemoved {
        header_name: String,
    },
    HeaderValueAdded {
        header_name: String,
    },
    BodyValueChanged {
        path: String,
        old_val: String,
        new_val: String,
    },
    BodyValueRemoved {
        path: String,
        value: String,
    },
    BodyValueAdded {
        path: String,
        value: String,
    },
    ArrayLengthChanged {
        path: String,
        old_len: usize,
        new_len: usize,
    },
    ArrayElementRemoved {
        path: String,
        value: String,
    },
    ArrayElementAdded {
        path: String,
        value: String,
    },
    DifferentBodyString {
        before: String,
        after: String,
    },
}

impl Difference {
    pub fn print(&self) {
        match self {
            Difference::StatusCodeChanged { old_val, new_val } => {
                println!("  Status Code Difference:");
                println!("    - {}", old_val.to_string().green());
                println!("    + {}", new_val.to_string().red());
            }
            Difference::HeaderValueChanged {
                header_name,
                old_val,
                new_val,
            } => {
                println!("    Changed Header: {}", header_name);
                println!("      - {}", old_val.green());
                println!("      + {}", new_val.red());
            }
            Difference::HeaderValueRemoved { header_name } => {
                println!("    Removed Header: {}", header_name);
            }
            Difference::HeaderValueAdded { header_name } => {
                println!("    Added Header: {}", header_name);
            }
            Difference::BodyValueChanged {
                path,
                old_val,
                new_val,
            } => {
                println!("    Changed body value at '{}' ", path.bright_white());
                println!("      - {}", old_val.green());
                println!("      + {}", new_val.red());
            }
            Difference::BodyValueRemoved { path, value } => {
                println!("    Removed body value at '{}' ", path.bright_white());
                println!("      - {}", value.green());
            }
            Difference::BodyValueAdded { path, value } => {
                println!("    Added body value at '{}' ", path.bright_white());
                println!("      + {}", value.red());
            }
            Difference::ArrayLengthChanged {
                path,
                old_len,
                new_len,
            } => {
                println!("    Array length changed at '{}' ", path.bright_white());
                println!("      - length: {}", old_len.to_string().green());
                println!("      + length: {}", new_len.to_string().red());
            }
            Difference::ArrayElementRemoved { path, value } => {
                println!("    Array element removed at '{}' ", path.bright_white());
                println!("      - {}", value.green());
            }
            Difference::ArrayElementAdded { path, value } => {
                println!("    Array element added at '{}' ", path.bright_white());
                println!("      + {}", value.red());
            }
            Difference::DifferentBodyString { before, after } => {
                println!("\n  Body (non-JSON or invalid JSON):");
                let body1_preview = format!("{}...", &before[..100]);
                let body2_preview = format!("{}...", &after[..100]);
                println!("    - {}", body1_preview.green());
                println!("    + {}", body2_preview.red());
            }
        }
    }
}

fn format_value(value: &Value, max_length: usize) -> String {
    match value {
        Value::String(s) => {
            if s.len() > max_length {
                format!("\"{}...\"", &s[..max_length])
            } else {
                format!("\"{}\"", s)
            }
        }
        Value::Array(arr) => {
            if arr.len() > 3 {
                format!("Array[{}]", arr.len())
            } else {
                format!("{}", value)
            }
        }
        Value::Object(obj) => {
            if obj.len() > 2 {
                format!("Object{{{}keys}}", obj.len())
            } else {
                format!("{}", value)
            }
        }
        _ => value.to_string(),
    }
}

fn compare_objects(
    path: &str,
    map1: &serde_json::Map<String, Value>,
    map2: &serde_json::Map<String, Value>,
    differences: &mut Vec<Difference>,
    max_depth: usize,
    current_depth: usize,
    ignored_paths: &Option<&HashSet<String>>,
) {
    let keys1: HashSet<&String> = map1.keys().collect();
    let keys2: HashSet<&String> = map2.keys().collect();

    // Find removed keys
    for key in keys1.difference(&keys2) {
        let new_path = if path.is_empty() {
            key.to_string()
        } else {
            format!("{}/{}", path, key)
        };
        differences.push(Difference::BodyValueRemoved {
            path: new_path,
            value: format_value(&map1[*key], 50),
        });
    }

    // Find added keys
    for key in keys2.difference(&keys1) {
        let new_path = if path.is_empty() {
            key.to_string()
        } else {
            format!("{}/{}", path, key)
        };
        differences.push(Difference::BodyValueAdded {
            path: new_path,
            value: format_value(&map2[*key], 50),
        });
    }

    // Compare common keys
    for key in keys1.intersection(&keys2) {
        let new_path = if path.is_empty() {
            key.to_string()
        } else {
            format!("{}/{}", path, key)
        };
        find_json_differences(
            &new_path,
            &map1[*key],
            &map2[*key],
            differences,
            max_depth,
            current_depth + 1,
            ignored_paths,
        );
    }
}

/// Calculate a hash representation of a JSON value for comparing array elements
fn get_value_hash(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut sorted_keys: Vec<&String> = map.keys().collect();
            sorted_keys.sort();

            let mut hash_parts = Vec::new();
            for key in sorted_keys {
                let key_hash = format!("{}:{}", key, get_value_hash(&map[key]));
                hash_parts.push(key_hash);
            }
            format!("{{{}}}", hash_parts.join(","))
        }
        Value::Array(arr) => {
            let mut hash_parts: Vec<String> = arr.iter().map(get_value_hash).collect();
            hash_parts.sort(); // Sort to ensure order-independent comparison
            format!("[{}]", hash_parts.join(","))
        }
        _ => value.to_string(),
    }
}

fn compare_arrays_order_independent(
    path: &str,
    arr1: &[Value],
    arr2: &[Value],
    differences: &mut Vec<Difference>,
    max_depth: usize,
    current_depth: usize,
    ignored_paths: &Option<&HashSet<String>>,
) {
    if arr1.len() != arr2.len() {
        differences.push(Difference::ArrayLengthChanged {
            path: path.to_string(),
            old_len: arr1.len(),
            new_len: arr2.len(),
        });
    }

    // Create a map of value hashes to count for each array
    let mut hash_count1: HashMap<String, usize> = HashMap::new();
    let mut hash_count2: HashMap<String, usize> = HashMap::new();

    // Count occurrences in first array
    for value in arr1 {
        let hash = get_value_hash(value);
        *hash_count1.entry(hash).or_insert(0) += 1;
    }

    // Count occurrences in second array
    for value in arr2 {
        let hash = get_value_hash(value);
        *hash_count2.entry(hash).or_insert(0) += 1;
    }

    // Find elements removed from arr1
    for (hash, count1) in &hash_count1 {
        let count2 = hash_count2.get(hash).unwrap_or(&0);
        if count1 > count2 {
            // Find items that are in arr1 but not in arr2 (or fewer in arr2)
            for item in arr1 {
                if get_value_hash(item) == *hash {
                    differences.push(Difference::ArrayElementRemoved {
                        path: format!("{}[*]", path),
                        value: format_value(item, 50),
                    });

                    // Only report the difference for the number of elements that are actually missing
                    if count1 <= &(count2 + differences.iter().filter(|d| {
                        matches!(d, Difference::ArrayElementRemoved { path: p, .. } if p == &format!("{}[*]", path))
                    }).count()) {
                        break;
                    }
                }
            }
        }
    }

    // Find elements added to arr2
    for (hash, count2) in &hash_count2 {
        let count1 = hash_count1.get(hash).cloned().unwrap_or(0);
        if count2 > &count1 {
            // Find items that are in arr2 but not in arr1 (or fewer in arr1)
            for item in arr2 {
                if get_value_hash(item) == *hash {
                    differences.push(Difference::ArrayElementAdded {
                        path: format!("{}[*]", path),
                        value: format_value(item, 50),
                    });

                    // Only report the difference for the number of elements that are actually added
                    if count2 <= &(count1 + differences.iter().filter(|d| {
                        matches!(d, Difference::ArrayElementAdded { path: p, .. } if p == &format!("{}[*]", path))
                    }).count()) {
                        break;
                    }
                }
            }
        }
    }

    // For each matching hash (same element appears in both arrays)
    // we still need to recurse into the values to find differences in nested structures
    let common_hashes: HashSet<_> = hash_count1
        .keys()
        .collect::<HashSet<_>>()
        .intersection(&hash_count2.keys().collect::<HashSet<_>>())
        .cloned()
        .collect();

    for hash in common_hashes {
        // Find one representative element from each array with this hash
        if let Some(val1_idx) = arr1.iter().position(|val| get_value_hash(val) == *hash) {
            if let Some(val2_idx) = arr2.iter().position(|val| get_value_hash(val) == *hash) {
                let val1 = &arr1[val1_idx];
                let val2 = &arr2[val2_idx];

                // Only recurse into objects and arrays - primitives will have identical hashes if they match
                match (val1, val2) {
                    (Value::Object(_), Value::Object(_)) | (Value::Array(_), Value::Array(_)) => {
                        let new_path = format!("{}[*]", path); // Use [*] to indicate order-independent position
                        find_json_differences(
                            &new_path,
                            val1,
                            val2,
                            differences,
                            max_depth,
                            current_depth + 1,
                            ignored_paths,
                        );
                    }
                    _ => {} // Primitives with same hash are identical
                }
            }
        }
    }
}

pub fn find_json_differences(
    path: &str,
    val1: &Value,
    val2: &Value,
    differences: &mut Vec<Difference>,
    max_depth: usize,
    current_depth: usize,
    ignored_paths: &Option<&HashSet<String>>,
) {
    if current_depth > max_depth {
        return;
    }

    let current_pointer = format!("/{}", path);
    if let Some(ignored_paths) = ignored_paths {
        for ignored_path in ignored_paths.iter() {
            // Normalize the ignored path by removing trailing slash (if any)
            let normalized_ignore = if ignored_path.ends_with('/') && ignored_path.len() > 1 {
                ignored_path.trim_end_matches('/')
            } else {
                ignored_path.as_str()
            };
            // If the current pointer exactly matches the ignore path
            // or is a sub-path of the ignore path, skip diffing.
            if current_pointer == normalized_ignore
                || current_pointer.starts_with(&format!("{}{}", normalized_ignore, "/"))
            {
                return;
            }
        }
    }

    match (val1, val2) {
        (Value::Object(map1), Value::Object(map2)) => {
            compare_objects(
                path,
                map1,
                map2,
                differences,
                max_depth,
                current_depth,
                ignored_paths,
            );
        }
        (Value::Array(arr1), Value::Array(arr2)) => {
            // Use order-independent comparison for arrays
            compare_arrays_order_independent(
                path,
                arr1,
                arr2,
                differences,
                max_depth,
                current_depth,
                ignored_paths,
            );
        }
        (v1, v2) if v1 != v2 => {
            differences.push(Difference::BodyValueChanged {
                path: path.to_string(),
                old_val: format_value(v1, 50),
                new_val: format_value(v2, 50),
            });
        }
        _ => {}
    }
}

pub fn compute_differences(
    response1: &HttpResponseData,
    response2: &HttpResponseData,
    headers_ignored: bool,
    ignored_paths: Option<&HashSet<String>>,
) -> Vec<Difference> {
    let mut differences = Vec::new();

    if response1.status_code != response2.status_code {
        differences.push(Difference::StatusCodeChanged {
            old_val: response1.status_code,
            new_val: response2.status_code,
        });
    }

    let diff_preview_len = 300;

    if !headers_ignored {
        let headers1 = &response1.headers;
        let headers2 = &response2.headers;

        if headers1 != headers2 {
            for (key, value1) in headers1.iter() {
                match headers2.get(key) {
                    Some(value2) => {
                        if value1 != value2 {
                            let min_len = std::cmp::min(value1.len(), value2.len());
    
                            differences.push(Difference::HeaderValueChanged {
                                header_name: key.to_string(),
                                old_val: value1
                                    .chars()
                                    .take(min_len.min(diff_preview_len))
                                    .collect::<String>(),
                                new_val: value2
                                    .chars()
                                    .take(min_len.min(diff_preview_len))
                                    .collect::<String>(),
                            });
                        }
                    }
                    None => {
                        differences.push(Difference::HeaderValueRemoved {
                            header_name: key.to_string(),
                        });
                    }
                }
            }
    
            for (key, _value2) in headers2.iter() {
                if !headers1.contains_key(key) {
                    differences.push(Difference::HeaderValueAdded {
                        header_name: key.to_string(),
                    });
                }
            }
        }
    }

    match (&response1.body.json, &response2.body.json) {
        // JSON body
        (Some(body1), Some(body2)) => {
            find_json_differences("", body1, body2, &mut differences, 10, 0, &ignored_paths);
        }
        // String body
        _ => {
            if response1.body != response2.body {
                differences.push(Difference::DifferentBodyString {
                    before: response1.body.raw.clone(),
                    after: response2.body.raw.clone(),
                });
            }
        }
    }

    differences
}
