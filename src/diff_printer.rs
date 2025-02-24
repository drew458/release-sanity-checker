use std::collections::HashSet;

use colored::Colorize;
use serde_json::Value;

use crate::HttpResponseData;

/// Represents a difference found in JSON structures
#[derive(Debug)]
enum Difference {
    ValueChanged {
        path: String,
        old_val: String,
        new_val: String,
    },
    KeyRemoved {
        path: String,
        value: String,
    },
    KeyAdded {
        path: String,
        value: String,
    },
    ArrayLengthChanged {
        path: String,
        old_len: usize,
        new_len: usize,
    },
}

impl Difference {
    fn print(&self) {
        match self {
            Difference::ValueChanged {
                path,
                old_val,
                new_val,
            } => {
                println!("    Changed value at '{}' ", path.bright_white());
                println!("      - {}", old_val.green());
                println!("      + {}", new_val.red());
            }
            Difference::KeyRemoved { path, value } => {
                println!("    Removed key at '{}' ", path.bright_white());
                println!("      - {}", value.green());
            }
            Difference::KeyAdded { path, value } => {
                println!("    Added at '{}' ", path.bright_white());
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
        differences.push(Difference::KeyRemoved {
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
        differences.push(Difference::KeyAdded {
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

fn find_json_differences(
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

    if ignored_paths.is_some_and(|ignored_paths| ignored_paths.contains(path)) {
        return;
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
            if arr1.len() != arr2.len() {
                differences.push(Difference::ArrayLengthChanged {
                    path: path.to_string(),
                    old_len: arr1.len(),
                    new_len: arr2.len(),
                });
            }
            let min_len = arr1.len().min(arr2.len());
            for i in 0..min_len {
                let new_path = format!("{}[{}]", path, i);
                find_json_differences(
                    &new_path,
                    &arr1[i],
                    &arr2[i],
                    differences,
                    max_depth,
                    current_depth + 1,
                    ignored_paths,
                );
            }
        }
        (v1, v2) if v1 != v2 => {
            differences.push(Difference::ValueChanged {
                path: path.to_string(),
                old_val: format_value(v1, 50),
                new_val: format_value(v2, 50),
            });
        }
        _ => {}
    }
}

pub fn print_differences(
    request_id: &str,
    url: &str,
    response1: &HttpResponseData,
    response2: &HttpResponseData,
    headers_ignored: bool,
    ignored_paths: Option<&HashSet<String>>,
    verbose: bool,
) {
    println!(
        "\n❌-----------------------------------------------------------------------------------------❌"
    );
    println!(
        "{}",
        format!(
            "Differences detected for request '{}' of URL '{}'",
            request_id, url
        )
        .yellow()
    );

    if response1.status_code != response2.status_code {
        println!("  Status Code Difference:");
        println!("    - {}", response1.status_code.to_string().green());
        println!("    + {}", response2.status_code.to_string().red());
    }

    let diff_preview_len = 200;

    if !headers_ignored {
        let headers1 = &response1.headers;
        let headers2 = &response2.headers;
        let mut header_differences = false;

        for (key, value1) in headers1.iter() {
            match headers2.get(key) {
                Some(value2) => {
                    if value1 != value2 {
                        if !header_differences {
                            println!("{}", "\n  Headers:".bold());
                            header_differences = true;
                        }

                        println!("    Changed Header: {}", key);

                        if verbose {
                            println!("      - {}", value1.green());
                            println!("      + {}", value2.red());
                        } else {
                            let min_len = std::cmp::min(value1.len(), value2.len());
                            println!(
                                "      - (preview): {}",
                                value1
                                    .chars()
                                    .take(min_len.min(diff_preview_len))
                                    .collect::<String>()
                                    .green()
                            );
                            println!(
                                "      + (preview):  {}",
                                value2
                                    .chars()
                                    .take(min_len.min(diff_preview_len))
                                    .collect::<String>()
                                    .red()
                            );
                        }
                    }
                }
                None => {
                    if !header_differences {
                        println!("{}", "\n  Headers:".bold());
                        header_differences = true;
                    }

                    println!("    Removed Header: {}", key);

                    if verbose {
                        println!("      Value: {}", value1.red());
                    } else {
                        println!(
                            "      Value (preview): {}",
                            value1
                                .chars()
                                .take(value1.len().min(diff_preview_len))
                                .collect::<String>()
                                .red()
                        );
                    }
                }
            }
        }

        for (key, value2) in headers2.iter() {
            if !headers1.contains_key(key) {
                if !header_differences {
                    println!("{}", "\n  Headers:".bold());
                    header_differences = true;
                }

                println!("    Added Header: {}", key);

                if verbose {
                    println!("      Value: {}", value2.red());
                } else {
                    println!(
                        "      Value (preview): {}",
                        value2
                            .chars()
                            .take(value2.len().min(diff_preview_len))
                            .collect::<String>()
                            .red()
                    );
                }
            }
        }

        if !header_differences && !headers1.is_empty() && !headers2.is_empty() {
            println!("    No header value changes detected.");
        } else if headers1.is_empty() && headers2.is_empty() {
            println!("    No headers to compare.");
        }
    }

    match (response1.body.json.clone(), response2.body.json.clone()) {
        // JSON body
        (Some(body1), Some(body2)) => {
            let mut differences = Vec::new();

            println!("{}", "\n  Body (JSON):".bold());
            find_json_differences("", &body1, &body2, &mut differences, 10, 0, &ignored_paths);

            if differences.is_empty() {
                println!("    No structural differences found");
            } else {
                for diff in differences {
                    diff.print();
                }
            }
        }
        // String body
        _ => {
            // Handle non-JSON bodies or parsing errors
            if response1.body != response2.body {
                println!("\n  Body (non-JSON or invalid JSON):");
                let body1_preview = format!("{}...", &response1.body.raw[..100]);
                let body2_preview = format!("{}...", &response2.body.raw[..100]);
                println!("    - {}", body1_preview.green());
                println!("    + {}", body2_preview.red());
            }
        }
    }

    println!(
        "❌-----------------------------------------------------------------------------------------❌"
    );
}
