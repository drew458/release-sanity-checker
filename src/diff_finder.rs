use std::collections::HashSet;

use colored::Colorize;
use serde_json::Value;

use crate::HttpResponseData;

/// Represents a difference found in JSON structures
#[derive(Debug, PartialEq)]
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

                if !before.is_empty() && !after.is_empty() {
                    let body1_preview = format!("{}...", &before[..100]);
                    let body2_preview = format!("{}...", &after[..100]);
                    println!("    - {}", body1_preview.green());
                    println!("    + {}", body2_preview.red());
                }
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

    let set1: HashSet<&Value> = arr1.iter().collect();
    let set2: HashSet<&Value> = arr2.iter().collect();

    let removed_elements = set1.difference(&set2);
    for removed_value in removed_elements {
        differences.push(Difference::ArrayElementRemoved {
            path: format!("{}[*]", path),
            value: format_value(removed_value, 50),
        });
    }

    let added_elements = set2.difference(&set1);
    for added_value in added_elements {
        differences.push(Difference::ArrayElementAdded {
            path: format!("{}[*]", path),
            value: format_value(added_value, 50),
        });
    }

    let common_elements = set1.intersection(&set2);
    for common_value in common_elements {
        // Find indices of common values in both arrays (needed for recursion)
        let indices1: Vec<usize> = arr1
            .iter()
            .enumerate()
            .filter_map(|(index, val)| {
                if val == *common_value {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        let indices2: Vec<usize> = arr2
            .iter()
            .enumerate()
            .filter_map(|(index, val)| {
                if val == *common_value {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();

        // Assuming there can be multiple identical values, to avoid re-comparing the same elements, just compare the first occurrences
        if let (Some(&index1), Some(&index2)) = (indices1.first(), indices2.first()) {
            let val1 = &arr1[index1];
            let val2 = &arr2[index2];

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
                _ => {} // Primitives are already compared by set difference
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

    let current_path = format!("/{}", path);
    if let Some(ignored_paths) = ignored_paths {
        for ignored_path in ignored_paths.iter() {
            // Normalize the ignored path by removing trailing slash (if any)
            let ignored_path = if ignored_path.ends_with('/') && ignored_path.len() > 1 {
                ignored_path.trim_end_matches('/')
            } else {
                ignored_path.as_str()
            };
            // If the current pointer exactly matches the ignore path
            // or is a sub-path of the ignore path, skip diffing.
            if current_path == ignored_path
                || current_path.starts_with(&format!("{}{}", ignored_path, "/"))
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
        // If the current values are either a Number, String, Boolean, Null, just perform a simple comparison
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
