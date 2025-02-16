use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{sqlite::SqliteConnectOptions, Pool, Row, Sqlite, SqlitePool};
use std::{
    collections::HashMap,
    env::{self},
    path::PathBuf,
    process,
    str::FromStr,
    sync::Arc,
};
use tokio::{fs, task::JoinSet};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct HttpResponseData {
    status_code: u16,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    headers: HashMap<String, String>,
    body: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct RequestResponse {
    request_id: String,
    response: HttpResponseData,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct RequestConfig {
    id: String,
    headers: HashMap<String, String>,
    #[serde(default)]
    body: Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Config {
    url: String,
    requests: Vec<RequestConfig>,
}

async fn fetch_response(
    url: &str,
    headers: &HashMap<String, String>,
    body: &Value,
) -> Result<HttpResponseData, reqwest::Error> {
    let client = reqwest::Client::new();

    let request_builder = if body.is_null() {
        client.get(url)
    } else {
        client.post(url).body(body.to_string())
    };

    let mut header_map = HeaderMap::new();
    headers.iter().for_each(|(k, v)| {
        if let (Ok(k), Ok(v)) = (k.parse::<HeaderName>(), HeaderValue::from_str(v)) {
            header_map.append(k, v);
        } else {
            eprintln!("Could not parse header {k}:{v}");
        }
    });

    let response = request_builder.headers(header_map).send().await?;

    Ok(HttpResponseData {
        status_code: response.status().as_u16(),
        headers: response
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_string(),
                    v.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect(),
        body: response.text().await?,
    })
}

// Find previous response for a request ID, if it exists
async fn find_previous_response(
    request_id: &str,
    url: &str,
    headers_ignored: bool,
    db: &Pool<Sqlite>,
) -> Result<Option<HttpResponseData>, Box<dyn std::error::Error + Send + Sync>> {
    let query = if headers_ignored {
        "SELECT status_code, body FROM response WHERE url = ? AND request_id = ?"
    } else {
        "SELECT status_code, body, headers FROM response WHERE url = ? AND request_id = ?"
    };

    match sqlx::query(query)
        .bind(url)
        .bind(request_id)
        .fetch_optional(db)
        .await?
    {
        Some(row) => {
            let headers = if !headers_ignored {
                serde_json::from_str(row.get("headers")).unwrap_or_default()
            } else {
                HashMap::new()
            };

            Ok(Some(HttpResponseData {
                status_code: row.get("status_code"),
                headers,
                body: row.get("body"),
            }))
        }
        None => Ok(None),
    }
}

fn are_responses_equal(
    response1: &HttpResponseData,
    response2: &HttpResponseData,
    headers_ignored: bool,
) -> bool {
    if headers_ignored {
        response1.status_code == response2.status_code && response1.body == response2.body
    } else {
        response1 == response2
    }
}

fn print_differences(
    response_name: &str,
    response1: &HttpResponseData,
    response2: &HttpResponseData,
    headers_ignored: bool,
) {
    println!("Differences detected for request: '{}'", response_name);

    if response1.status_code != response2.status_code {
        println!("  Status Code Difference:");
        println!("    Before: {}", response1.status_code);
        println!("    After:  {}", response2.status_code);
    }

    if !headers_ignored {
        let headers1 = &response1.headers;
        let headers2 = &response2.headers;

        let mut header_differences = false;
        println!("  Header Differences:");

        for (key, value1) in headers1.iter() {
            if let Some(value2) = headers2.get(key) {
                if value1 != value2 {
                    println!("    Changed Header: {}", key);
                    println!("      Before: {}", value1);
                    println!("      After:  {}", value2);
                    header_differences = true;
                }
            } else {
                println!("    Removed Header: {}", key);
                println!("      Value: {}", value1);
                header_differences = true;
            }
        }

        for (key, value2) in headers2.iter() {
            if !headers1.contains_key(key) {
                println!("    Added Header: {}", key);
                println!("      Value: {}", value2);
                header_differences = true;
            }
        }

        if !header_differences && !headers1.is_empty() && !headers2.is_empty() {
            println!("    No header value changes detected.");
        } else if headers1.is_empty() && headers2.is_empty() {
            println!("    No headers to compare.");
        }
    }

    if response1.body != response2.body {
        println!("  Body Difference:");
        let len1 = response1.body.len();
        let len2 = response2.body.len();
        let min_len = std::cmp::min(len1, len2);
        let diff_preview_len = 100;

        println!(
            "    Before (preview): {}",
            response1
                .body
                .chars()
                .take(min_len.min(diff_preview_len))
                .collect::<String>()
        );
        println!(
            "    After  (preview): {}",
            response2
                .body
                .chars()
                .take(min_len.min(diff_preview_len))
                .collect::<String>()
        );

        if len1 != len2 {
            println!("    Body length changed: Before: {}, After: {}", len1, len2);
        }
    } else {
        println!("  No Body Difference.");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<String> = env::args().collect();
    let program_name = args[0].clone();

    if args.len() > 1 && args[1] == "--help" {
        print_usage(&program_name);
        return Ok(());
    }

    let mut config_paths: Vec<PathBuf> = Vec::new();
    let mut mode_file = true; // Default mode is --file
    let mut headers_ignored = false;

    let mut arg_iter = args.iter().skip(1);

    while let Some(arg) = arg_iter.next() {
        match arg.as_str() {
            "--file" => {
                mode_file = true;
                if let Some(path_str) = arg_iter.next() {
                    config_paths.push(PathBuf::from(path_str));
                } else {
                    eprintln!("Error: Missing path for --file option.");
                    process::exit(1);
                }
            }
            "--directory" => {
                mode_file = false;
                if let Some(dir_path_str) = arg_iter.next() {
                    let dir_path = PathBuf::from(dir_path_str);
                    if dir_path.is_dir() {
                        let mut entries = fs::read_dir(dir_path).await?;

                        while let Some(entry_result) = entries.next_entry().await? {
                            let path = entry_result.path();
                            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                                config_paths.push(path);
                            }
                        }

                        if config_paths.is_empty() {
                            eprintln!(
                                "Warning: No JSON config files found in directory '{}'.",
                                dir_path_str
                            );
                        }
                    } else {
                        eprintln!(
                            "Error: Directory path '{}' is not a valid directory.",
                            dir_path_str
                        );
                        process::exit(1);
                    }
                } else {
                    eprintln!("Error: Missing directory path for --directory option.");
                    process::exit(1);
                }
            }
            "--ignore-headers" => {
                headers_ignored = true;
            }
            config_path if mode_file => {
                // Default to file mode if no flag and it's the first non-flag arg
                config_paths.push(PathBuf::from(config_path));
                mode_file = false; // Ensure subsequent args are not treated as file paths in default mode
            }
            _ => {
                eprintln!("Error: Unknown option '{}'.", arg);
                process::exit(1);
            }
        }
    }

    if config_paths.is_empty() {
        eprintln!("Error: No config file or directory specified.");
        process::exit(1);
    }

    let db = Arc::new(
        SqlitePool::connect_with(
            SqliteConnectOptions::from_str("sqlite://release-sanity-checker-data.db")?.create_if_missing(true),
        )
        .await?,
    );
    let _ = sqlx::query(
        "CREATE TABLE IF NOT EXISTS response (
                request_id    TEXT NOT NULL,
                url           TEXT NOT NULL, 
                status_code   INTEGER NOT NULL,
                headers       TEXT,
                body          TEXT,
                PRIMARY KEY(request_id, url)
            );
            CREATE INDEX IF NOT EXISTS url_idx ON response(url);",
    )
    .execute(db.as_ref())
    .await;

    let mut file_tasks = JoinSet::new();

    for config_path in config_paths {
        let db = db.clone();
        let headers_ignored = headers_ignored;

        // Process config files concurrently
        file_tasks.spawn(async move {
            let config: Config = serde_json::from_str(&fs::read_to_string(&config_path).await?)?;
            let config_url = Arc::new(config.url);

            let mut request_tasks = JoinSet::new();

            // Process requests concurrently
            for request_config in config.requests {
                let url = Arc::clone(&config_url);
                let db = db.clone();

                request_tasks.spawn(async move {
                    println!("\nChecking request: '{}' of URL '{}'", request_config.id, url);

                    let current_response =
                        fetch_response(&url, &request_config.headers, &request_config.body).await?;

                    if let Some(prev_response) = find_previous_response(
                        &request_config.id,
                        &url,
                        headers_ignored,
                        db.as_ref(),
                    )
                    .await?
                    {
                        if !are_responses_equal(&prev_response, &current_response, headers_ignored)
                        {
                            print_differences(
                                &request_config.id,
                                &prev_response,
                                &current_response,
                                headers_ignored,
                            );
                        } else {
                            println!(
                                "Request '{}' of URL '{}' has not changed.",
                                request_config.id, url
                            );
                        }
                    }

                    sqlx::query(
                        "INSERT INTO response (request_id, url, status_code, body, headers)
                        VALUES (?, ?, ?, ?, ?)
                        ON CONFLICT (request_id, url)
                        DO UPDATE SET status_code = excluded.status_code,
                                    body = excluded.body,
                                    headers = excluded.headers",
                    )
                    .bind(&request_config.id)
                    .bind(url.as_ref())
                    .bind(current_response.status_code)
                    .bind(&current_response.body)
                    .bind(serde_json::to_string(&current_response.headers).unwrap_or_default())
                    .execute(db.as_ref())
                    .await?;

                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
                });
            }

            while let Some(result) = request_tasks.join_next().await {
                result??;
            }

            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        });
    }

    while let Some(result) = file_tasks.join_next().await {
        result??;
    }

    println!("\nResponse check completed.");
    Ok(())
}

fn print_usage(program_name: &str) {
    println!("Usage: {program_name} [options] <config_path>");
    println!();
    println!("Options:");
    println!("  --file <config_path>          Run with a specific config file (default mode).");
    println!("  --directory <dir_path>        Run with all config files found in the directory.");
    println!("  --ignore-headers              Do not look for changes in response headers.");
    println!();
    println!("Examples:");
    println!(
        "  {} config.json              (Default: Run with config.json)",
        program_name
    );
    println!(
        "  {} --file config.json      Run with config.json",
        program_name
    );
    println!(
        "  {} --directory examples    Run with all .json files in the 'examples' directory",
        program_name
    );
}
