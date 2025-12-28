mod diff_finder;
mod printer;

use crate::diff_finder::compute_differences;
use anyhow::{Context, Result, bail};
use clap::{Args, Parser};
use log::debug;
use printer::{DifferencesPrinter, DifferencesPrinterMessage};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{
    Pool, Row, Sqlite,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::cmp::max;
use std::{
    collections::{HashMap, HashSet},
    env::{self},
    path::PathBuf,
    process,
    str::FromStr,
    sync::{Arc, atomic::AtomicUsize},
    time::Duration,
};
use tokio::{
    fs,
    sync::{Mutex, Semaphore},
    task::JoinSet,
};

#[derive(Serialize, Deserialize, PartialEq, Debug, Default, Clone)]
struct ParsedBody {
    raw: String,
    json: Option<Value>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Default)]
struct HttpResponseData {
    status_code: u16,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    headers: HashMap<String, String>,
    body: ParsedBody,
}

impl HttpResponseData {
    fn new(status_code: u16, headers: HashMap<String, String>, body: String) -> HttpResponseData {
        let mut json_body = None;

        // Check if the response is JSON
        match (headers.get("Content-Type"), headers.get("content-type")) {
            (Some(content_type), Some(_))
            | (Some(content_type), None)
            | (None, Some(content_type)) => {
                if content_type.starts_with("application/json") {
                    json_body = serde_json::from_str(&body).ok()
                }
            }
            _ => {}
        }

        HttpResponseData {
            status_code,
            headers,
            body: ParsedBody {
                json: json_body,
                raw: body,
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct RequestResponse {
    request_id: String,
    url: String,
    status_code: u16,
    headers: String,
    body: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct RequestConfig {
    url: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    body: Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct RequestFlowConfig {
    id: String,
    flow: Vec<RequestConfig>,
    ignore_paths: Option<HashSet<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SanityCheckConfig {
    requests: Vec<RequestFlowConfig>,
}

async fn fetch_response(
    url: &str,
    headers: &HashMap<String, String>,
    body: &Value,
    client: &Client,
    semaphore: &Semaphore,
) -> Result<HttpResponseData> {
    let request_builder = if body.is_null() {
        client.get(url)
    } else {
        client.post(url).body(body.to_string())
    };

    let header_map: reqwest::header::HeaderMap = headers
        .iter()
        .filter_map(
            |(k, v)| match (k.parse::<reqwest::header::HeaderName>(), v.parse()) {
                (Ok(header_name), Ok(header_value)) => Some((header_name, header_value)),
                _ => {
                    eprintln!("Could not parse header {}:{}", k, v);
                    None
                }
            },
        )
        .collect();

    debug!("Acquiring semaphore for request to {}...", url);
    let _permit = semaphore
        .acquire()
        .await
        .context("Failed to acquire semaphore")?;
    debug!(
        "Semaphore for request to {} acquired! Sending request...",
        url
    );
    let response = request_builder
        .headers(header_map)
        .send()
        .await
        .with_context(|| format!("Failed to send request to {}", url))?;

    let status = response.status().as_u16();
    let resp_headers = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
        .collect();
    let text = response
        .text()
        .await
        .with_context(|| format!("Failed to read response body from {}", url))?;

    Ok(HttpResponseData::new(status, resp_headers, text))
}

/// Find previous response for a request ID, if it exists
async fn find_previous_response(
    request_id: &str,
    headers_ignored: bool,
    db: &Pool<Sqlite>,
) -> Result<Option<HttpResponseData>> {
    let query = if headers_ignored {
        "SELECT baseline_status_code, baseline_body FROM response WHERE request_id = ?"
    } else {
        "SELECT baseline_status_code, baseline_body, baseline_headers FROM response WHERE request_id = ?"
    };

    match sqlx::query(query)
        .persistent(true)
        .bind(request_id)
        .fetch_optional(db)
        .await
        .context("Failed to query previous response from database")?
    {
        Some(row) => {
            let headers = if !headers_ignored {
                let headers_str: &str = row.get("baseline_headers");
                serde_json::from_str(headers_str).unwrap_or_default()
            } else {
                HashMap::new()
            };

            let body: String = row.get("baseline_body");

            Ok(Some(HttpResponseData {
                status_code: row.get("baseline_status_code"),
                headers,
                body: ParsedBody {
                    json: serde_json::from_str(&body).ok(),
                    raw: body,
                },
            }))
        }
        None => Ok(None),
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(value_name = "FILE")]
    files: Vec<PathBuf>,

    #[command(flatten)]
    options: Options,
}

#[derive(Args, Debug)]
struct Options {
    #[arg(long, value_name = "DIRECTORY", conflicts_with = "files")]
    directory: Option<PathBuf>,

    #[arg(long)]
    ignore_headers: bool,

    #[arg(long)]
    baseline: bool,

    #[arg(long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let requests_per_host: usize = env::var("REQUESTS_PER_HOST")
        .unwrap_or(30.to_string())
        .parse()
        .context("Invalid REQUESTS_PER_HOST env variable")?;
    let max_retries: u16 = max(
        1,
        env::var("MAX_RETRIES")
            .unwrap_or(3.to_string())
            .parse()
            .context("Invalid MAX_RETRIES env variable")?,
    );

    let cli = Cli::parse();
    let mut config_paths: Vec<PathBuf> = Vec::new();

    // Handle directory option
    if let Some(dir_path) = cli.options.directory {
        if !dir_path.is_dir() {
            eprintln!("Error: '{}' is not a valid directory.", dir_path.display());
            process::exit(1);
        }

        let mut files = fs::read_dir(&dir_path)
            .await
            .context(format!("Failed to read directory {:?}", dir_path))?;
        let mut found_files = false;

        while let Some(file) = files
            .next_entry()
            .await
            .context("Failed to read next entry in directory")?
        {
            let path = file.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                config_paths.push(path);
                found_files = true;
            }
        }

        if !found_files {
            eprintln!(
                "Warning: No JSON config files found in directory '{}'.",
                dir_path.display()
            );
        }
    } else {
        // Handle individual files
        config_paths = cli.files;
    }

    if config_paths.is_empty() {
        eprintln!("Error: No config file or directory specified.");
        process::exit(1);
    }

    let db_path = "release-sanity-checker-data.db";
    let db = Arc::new(
        SqlitePoolOptions::new()
            .max_connections(20)
            .acquire_timeout(Duration::from_secs(1))
            .connect_with(
                SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path))
                    .context("Failed to parse SQLite connection options")?
                    .create_if_missing(true)
                    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                    .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
                    .locking_mode(sqlx::sqlite::SqliteLockingMode::Normal),
            )
            .await
            .context(format!("Failed to connect to database at {}", db_path))?,
    );
    let _ = sqlx::query(
        "CREATE TABLE IF NOT EXISTS response (
                request_id              TEXT NOT NULL,
                url                     TEXT NOT NULL, 
                baseline_status_code    INTEGER,
                checktime_status_code   INTEGER,
                baseline_headers        TEXT,
                checktime_headers       TEXT,
                baseline_body           TEXT,
                checktime_body          TEXT,
                PRIMARY KEY(request_id)
            );
            CREATE INDEX IF NOT EXISTS url_idx ON response(request_id);",
    )
    .execute(db.as_ref())
    .await
    .context("Failed to initialize database schema")?;

    let http_client = reqwest::ClientBuilder::new()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(requests_per_host)
        .tcp_keepalive(Duration::from_secs(60))
        .build()
        .context("Failed to build HTTP client")?;

    let url_to_semaphore = Arc::new(Mutex::new(HashMap::new()));
    let requests_counter = Arc::new(AtomicUsize::new(0));
    let changed_requests_counter = Arc::new(AtomicUsize::new(0));

    let mut tasks = JoinSet::new();
    let mut errors_count = 0;

    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    {
        let (sender, receiver) = tokio::sync::mpsc::channel(100);
        let printer = DifferencesPrinter::new(receiver, done_tx);
        tokio::task::spawn(printer::run_differences_printer(printer));

        println!("Starting to process requests...\n");

        for config_path in config_paths {
            debug!("Reading config path at {:#?}...", config_path);
            let content = tokio::fs::read_to_string(&config_path)
                .await
                .with_context(|| format!("Failed to read config file {:?}", config_path))?;
            let config: SanityCheckConfig = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse JSON config at {:?}", config_path))?;

            // Process requests inside config file concurrently
            for request_config in config.requests {
                let db = db.clone();
                let http_client = http_client.clone();
                let url_to_semaphore = url_to_semaphore.clone();
                let requests_counter = requests_counter.clone();
                let changed_requests_counter = changed_requests_counter.clone();
                let print_sender = sender.clone();

                tasks.spawn(async move {
                    requests_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                    debug!("Checking request '{}'", request_config.id);

                    // Flow is processed serially
                    for i in 0..request_config.flow.len() {
                        let flow = request_config.flow.get(i).unwrap();

                        let semaphore = {
                            let mut map = url_to_semaphore.lock().await;
                            map.entry(flow.url.clone())
                                .or_insert_with(|| Arc::new(Semaphore::new(requests_per_host)))
                                .clone()
                        };

                        let mut retries = max_retries.clone();
                        let mut current_response = None;
                        while retries > 0 {
                            debug!("Sending request {} to {}", request_config.id, flow.url);

                            match fetch_response(
                                &flow.url,
                                &flow.headers,
                                &flow.body,
                                &http_client,
                                &semaphore,
                            )
                            .await {
                                Ok(res) => {
                                    if res.status_code >= 500 {
                                        debug!("Request to url {} has errors (status code: {})", flow.url, res.status_code);
                                        retries -= 1;
                                    } else {
                                        current_response = Some(res);
                                        break;
                                    }
                                },
                                Err(e) => {
                                    debug!("Error fetching response: {:#}", e);
                                    retries -= 1;
                                },
                            }
                        }

                        let current_response = match current_response {
                            Some(res) => res,
                            None => bail!("Failed to get response for request '{}' to '{}' after multiple retries", 
                                request_config.id, flow.url),
                        };

                        debug!("Request {} to {} done", request_config.id, flow.url);

                        // If it's the last request of the flow, run the check on the response
                        if i == request_config.flow.len() - 1 {
                            if !cli.options.baseline {
                                // Try to find a previous response for that request (identified by id)
                                let prev_response = find_previous_response(
                                    &request_config.id,
                                    cli.options.ignore_headers,
                                    db.as_ref(),
                                )
                                    .await?;

                                if let Some(prev_response) = prev_response {
                                    let differences = compute_differences(
                                        &prev_response,
                                        &current_response,
                                        cli.options.ignore_headers,
                                        request_config.ignore_paths.as_ref(),
                                    );

                                    if differences.is_empty() {
                                        if cli.options.verbose {
                                            println!(
                                                "\n✅ Request with ID: '{}' has not changed. ✅",
                                                request_config.id
                                            );
                                        }
                                    } else {
                                        changed_requests_counter
                                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                                        print_sender.send(DifferencesPrinterMessage::PrintDifferences {
                                            differences, request_id: request_config.id.clone()
                                        }).await.context("Failed to send differences to printer")?
                                    }
                                }
                            }

                            let query_str = if cli.options.baseline {
                                "INSERT INTO response (request_id, url, baseline_status_code, baseline_body, baseline_headers)
                                    VALUES (?, ?, ?, ?, ?)
                                    ON CONFLICT (request_id) DO UPDATE SET url = excluded.url, baseline_status_code = excluded.baseline_status_code,
                                            baseline_body = excluded.baseline_body,
                                            baseline_headers = excluded.baseline_headers".to_string()
                            } else {
                                "INSERT INTO response (request_id, url, checktime_status_code, checktime_body, checktime_headers)
                                    VALUES (?, ?, ?, ?, ?)
                                    ON CONFLICT (request_id) DO UPDATE SET checktime_status_code = excluded.checktime_status_code,
                                            checktime_body = excluded.checktime_body,
                                            checktime_headers = excluded.checktime_headers".to_string()
                            };
                            sqlx::query(&query_str)
                                .persistent(true)
                                .bind(&request_config.id)
                                .bind(&flow.url)
                                .bind(current_response.status_code)
                                .bind(&current_response.body.raw,)
                                .bind(serde_json::to_string(&current_response.headers).context("Failed to serialize headers")?)
                                .execute(db.as_ref())
                                .await
                                .context("Failed to save response to database")?;
                        };
                    }

                    Ok::<(), anyhow::Error>(())
                });
            }
        }

        // Wait for all tasks for finish
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(r) => {
                    if let Err(e) = r {
                        errors_count += 1;
                        eprintln!("Error processing request: {:#}", e)
                    }
                }
                Err(e) => eprintln!("Task join error: {}", e),
            }
        }
    }

    let _ = done_rx.await; // Wait for print_actor to confirm it's done

    if cli.options.baseline {
        println!(
            "\nBaseline built successfully. Processed {} requests, errors: {}",
            requests_counter.load(std::sync::atomic::Ordering::Relaxed),
            errors_count
        );
    } else {
        println!(
            "\nResponse check completed. Changed request: {} out of {}. Errors: {}",
            changed_requests_counter.load(std::sync::atomic::Ordering::Relaxed),
            requests_counter.load(std::sync::atomic::Ordering::Relaxed),
            errors_count
        );
    }

    Ok(())
}
