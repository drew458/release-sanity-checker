mod tests;

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
        old_val: Vec<String>,
        new_val: Vec<String>,
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
                println!("      - {}", format!("{:?}", old_val).green());
                println!("      + {}", format!("{:?}", new_val).red());
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
) {
    if arr1.len() != arr2.len() {
        differences.push(Difference::ArrayLengthChanged {
            path: path.to_string(),
            old_len: arr1.len(),
            new_len: arr2.len(),
        });
    }

    let mut counts1 = std::collections::HashMap::new();
    for val in arr1 {
        *counts1.entry(val).or_insert(0) += 1;
    }

    let mut counts2 = std::collections::HashMap::new();
    for val in arr2 {
        *counts2.entry(val).or_insert(0) += 1;
    }

    let keys1: HashSet<&&Value> = counts1.keys().collect();
    let keys2: HashSet<&&Value> = counts2.keys().collect();

    // Elements in arr1 but not in arr2 (or with higher count)
    for &&val in &keys1 {
        let count1 = counts1[val];
        let count2 = *counts2.get(val).unwrap_or(&0);
        if count1 > count2 {
            for _ in 0..(count1 - count2) {
                differences.push(Difference::ArrayElementRemoved {
                    path: format!("{}[*]", path),
                    value: format_value(val, 50),
                });
            }
        }
    }

    // Elements in arr2 but not in arr1 (or with higher count)
    for &&val in &keys2 {
        let count1 = *counts1.get(val).unwrap_or(&0);
        let count2 = counts2[val];
        if count2 > count1 {
            for _ in 0..(count2 - count1) {
                differences.push(Difference::ArrayElementAdded {
                    path: format!("{}[*]", path),
                    value: format_value(val, 50),
                });
            }
        }
    }

    // If both are objects or arrays, we might still want to recurse to find deep differences
    // BUT only for elements that were NOT identical (otherwise we find nothing).
    // The current order-independent logic is simplified: either elements match exactly or they don't.
    // If we want to find "similar" objects in the array, we'd need a heuristic to match them.
    // Given the current design, we'll stick to exact match for order-independent elements.
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
        // If the current pointer exactly matches the ignore path
        // or is a sub-path of the ignore path, skip diffing.
        if ignored_paths.contains(&current_path)
            || ignored_paths
                .iter()
                .any(|ip| current_path.starts_with(&format!("{}/", ip)))
        {
            return;
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
                differences
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
    // Pre-normalize ignored paths
    let normalized_ignored_paths: Option<HashSet<String>> = ignored_paths.map(|paths| {
        paths
            .iter()
            .map(|p| {
                if p.ends_with('/') && p.len() > 1 {
                    p.trim_end_matches('/').to_string()
                } else {
                    p.clone()
                }
            })
            .collect()
    });
    let ignored_paths_ref = normalized_ignored_paths.as_ref();
    let mut differences = Vec::new();

    if response1.status_code != response2.status_code {
        differences.push(Difference::StatusCodeChanged {
            old_val: response1.status_code,
            new_val: response2.status_code,
        });
    }

    if !headers_ignored {
        let headers1 = &response1.headers;
        let headers2 = &response2.headers;

        if headers1 != headers2 {
            for (key, value1) in headers1.iter() {
                match headers2.get(key) {
                    Some(value2) => {
                        if value1 != value2 {
                            differences.push(Difference::HeaderValueChanged {
                                header_name: key.to_string(),
                                old_val: value1.clone(),
                                new_val: value2.clone(),
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
            find_json_differences(
                "",
                body1,
                body2,
                &mut differences,
                10,
                0,
                &ignored_paths_ref,
            );
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
