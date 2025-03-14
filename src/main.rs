mod diff_finder;
mod print_actor;

use diff_finder::compute_differences;

use log::debug;
use print_actor::{DifferencesPrinter, DifferencesPrinterMessage};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Pool, Row, Sqlite,
};
use std::{
    collections::{HashMap, HashSet},
    env::{self},
    path::PathBuf,
    process,
    str::FromStr,
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};
use tokio::{
    fs,
    sync::{RwLock, Semaphore},
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
        HttpResponseData {
            status_code,
            headers,
            body: ParsedBody {
                json: serde_json::from_str(&body).ok(),
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

#[derive(Default)]
struct CmdConfig {
    config_paths: Vec<PathBuf>,
    headers_ignored: bool,
    baseline_mode: bool,
    verbose: bool,
}

async fn fetch_response(
    url: &str,
    headers: &HashMap<String, String>,
    body: &Value,
    client: &Client,
    sempahore: &Semaphore,
) -> Result<HttpResponseData, Box<dyn std::error::Error + Send + Sync>> {
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

    let _permit = sempahore.acquire().await?;
    let response = request_builder.headers(header_map).send().await?;

    Ok(HttpResponseData::new(
        response.status().as_u16(),
        response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
            .collect(),
        response.text().await?,
    ))
}

/// Find previous response for a request ID, if it exists
async fn find_previous_response(
    request_id: &str,
    url: &str,
    headers_ignored: bool,
    db: &Pool<Sqlite>,
) -> Result<Option<HttpResponseData>, Box<dyn std::error::Error + Send + Sync>> {
    let query = if headers_ignored {
        "SELECT baseline_status_code, baseline_body FROM response WHERE url = ? AND request_id = ?"
    } else {
        "SELECT baseline_status_code, baseline_body, baseline_headers FROM response WHERE url = ? AND request_id = ?"
    };

    match sqlx::query(query)
        .persistent(true)
        .bind(url)
        .bind(request_id)
        .fetch_optional(db)
        .await?
    {
        Some(row) => {
            let headers = if !headers_ignored {
                serde_json::from_str(row.get("baseline_headers")).unwrap_or_default()
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

async fn process_args(
    args: Vec<String>,
) -> Result<CmdConfig, Box<dyn std::error::Error + Send + Sync>> {
    let program_name = &args[0];

    if args.len() > 1 && args[1] == "--help" {
        print_usage(program_name);
        process::exit(0);
    }

    let mut config_paths: Vec<PathBuf> = Vec::new();
    let mut mode_file = true; // Default mode is --file
    let mut headers_ignored = false;
    let mut baseline_mode = false;
    let mut verbose = false;

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
            "--baseline" => {
                baseline_mode = true;
            }
            "--verbose" => verbose = true,
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

    Ok(CmdConfig {
        config_paths,
        headers_ignored,
        baseline_mode,
        verbose,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let requests_per_host: u16 = env::var("REQUESTS_PER_HOST")
        .unwrap_or(30.to_string())
        .parse()
        .unwrap_or(30);

    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let cmd_config = process_args(args).await?;

    let db = Arc::new(
        SqlitePoolOptions::new()
            .max_connections(20)
            .acquire_timeout(Duration::from_secs(1))
            .connect_with(
                SqliteConnectOptions::from_str("sqlite://release-sanity-checker-data.db")?
                    .create_if_missing(true)
                    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                    .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
                    .locking_mode(sqlx::sqlite::SqliteLockingMode::Normal),
            )
            .await?,
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
                PRIMARY KEY(request_id, url)
            );
            CREATE INDEX IF NOT EXISTS url_idx ON response(url);",
    )
    .execute(db.as_ref())
    .await;

    let http_client = reqwest::ClientBuilder::new()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(requests_per_host.into())
        .tcp_keepalive(Duration::from_secs(60))
        .build()?;

    let url_to_semaphore = Arc::new(RwLock::new(HashMap::new()));
    let requests_counter = Arc::new(AtomicUsize::new(0));
    let changed_requests_counter = Arc::new(AtomicUsize::new(0));

    let mut tasks = JoinSet::new();
    let mut errors_count = 0;

    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    {
        let (sender, receiver) = tokio::sync::mpsc::channel(100);
        let printer = DifferencesPrinter::new(receiver, done_tx);
        tokio::task::spawn(print_actor::run_differences_printer(printer));

        println!("Starting to process requests...\n");

        for config_path in cmd_config.config_paths {
            let config: SanityCheckConfig =
                serde_json::from_str(&fs::read_to_string(&config_path).await?)?;

            // Process requests inside config file concurrently
            for request_config in config.requests {
                let db = db.clone();
                let http_client = http_client.clone();
                let url_to_semaphore = url_to_semaphore.clone();
                let requests_counter = requests_counter.clone();
                let changed_requests_counter = changed_requests_counter.clone();
                let print_sender = sender.clone();

                tasks.spawn(async move {
                    requests_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                    debug!("Checking request '{}'", request_config.id);

                    // Flow is processed serially
                    for i in 0..request_config.flow.len() {
                        let flow = request_config.flow.get(i).unwrap();

                        let mut semaphore_exists_for_url = true;
                        {
                            if url_to_semaphore.read().await.get(&flow.url).is_none() {
                                semaphore_exists_for_url = false;
                            }
                        }
                        if !semaphore_exists_for_url {
                            let mut url_to_semaphore = url_to_semaphore.write().await;
                            url_to_semaphore.insert(
                                flow.url.clone(),
                                Arc::new(Semaphore::new(requests_per_host.into())),
                            );
                        }
                        let semaphore: Arc<Semaphore>;
                        {
                            let binding = url_to_semaphore.read().await;
                            semaphore = binding.get(&flow.url).cloned().unwrap();
                        }

                        let mut retries: i8 = 3;
                        let mut current_response = Option::None;
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
                                        retries -= 1;
                                    } else {
                                        current_response = Some(res);
                                        break;
                                    }
                                },
                                Err(e) => {
                                    debug!("{}", e);
                                    retries -= 1;
                                },
                            }
                        }

                        if current_response.is_none() {
                            return Err(format!("Failed to get response for request '{}' to '{}' after multiple retries", 
                                request_config.id, flow.url).into());
                        }

                        debug!("Request {} to {} done", request_config.id, flow.url);

                        let current_response = current_response.expect("Response cannot be None!");

                        // Try to find a previous response for that request (identified by id and URL)
                        let prev_response = find_previous_response(
                            &request_config.id,
                            &flow.url,
                            cmd_config.headers_ignored,
                            db.as_ref(),
                        )
                        .await?;

                        // If it's the last request of the flow, run the check on the response
                        if i == request_config.flow.len() - 1 {
                            if !cmd_config.baseline_mode {
                                if let Some(prev_response) = prev_response {
                                    let differences = compute_differences(
                                        &prev_response,
                                        &current_response,
                                        cmd_config.headers_ignored,
                                        request_config.ignore_paths.as_ref(),
                                    );

                                    if differences.is_empty() {
                                        if cmd_config.verbose {
                                            println!(
                                                "\n✅ Request '{}' of URL '{}' has not changed. ✅",
                                                request_config.id, flow.url
                                            );
                                        }
                                    } else {
                                        changed_requests_counter
                                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                        print_sender.send(DifferencesPrinterMessage::PrintDifferences {
                                            differences, request_id: request_config.id.clone(), url: flow.url.clone()
                                        }).await?
                                    }
                                }
                            }

                            let query_str = if cmd_config.baseline_mode {
                                "INSERT INTO response (request_id, url, baseline_status_code, baseline_body, baseline_headers)
                                    VALUES (?, ?, ?, ?, ?)
                                    ON CONFLICT (request_id, url) DO UPDATE SET baseline_status_code = excluded.baseline_status_code,
                                            baseline_body = excluded.baseline_body,
                                            baseline_headers = excluded.baseline_headers".to_string()
                            } else {
                                "INSERT INTO response (request_id, url, checktime_status_code, checktime_body, checktime_headers)
                                    VALUES (?, ?, ?, ?, ?)
                                    ON CONFLICT (request_id, url) DO UPDATE SET checktime_status_code = excluded.checktime_status_code,
                                            checktime_body = excluded.checktime_body,
                                            checktime_headers = excluded.checktime_headers".to_string()
                            };
                            sqlx::query(&query_str)
                                .persistent(true)
                                .bind(&request_config.id)
                                .bind(&flow.url)
                                .bind(current_response.status_code)
                                .bind(&current_response.body.raw,)
                                .bind(serde_json::to_string(&current_response.headers)?)
                                .execute(db.as_ref())
                                .await?;
                        };
                    }

                    Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                });
            }
        }

        // Wait for all tasks for finish
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(r) => {
                    if let Err(e) = r {
                        errors_count += 1;
                        eprintln!("{}", e)
                    }
                }
                Err(e) => eprintln!("{}", e),
            }
        }
    }

    let _ = done_rx.await; // Wait for print_actor to confirm it's done

    if cmd_config.baseline_mode {
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

fn print_usage(program_name: &str) {
    println!("Usage: {program_name} [options] <config_path>");
    println!();
    println!("Options:");
    println!("  --file <config_path>          Run with a specific config file (default mode).");
    println!("  --directory <dir_path>        Run with all config files found in the directory.");
    println!("  --ignore-headers              Do not look for changes in response headers.");
    println!("  --baseline                    Build the baseline for the requests.");
    println!("  --verbose                     Print the full response body/header when changed and response that didn't change.");
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
