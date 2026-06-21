use futures_util::stream::{self, StreamExt};
use sanitize_filename::sanitize;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::io::AsyncWriteExt;
use url::Url;

#[derive(Debug, Deserialize)]
struct DownloadRequest {
    urls: Vec<String>,
    folder: String,
    concurrency: usize,
}

#[derive(Debug, Clone)]
struct DownloadJob {
    source_url: String,
    download_url: String,
    filename: String,
}

#[derive(Debug, Serialize)]
struct DownloadResult {
    url: String,
    filename: String,
    ok: bool,
    message: String,
}

fn filename_from_url(raw_url: &str, index: usize) -> String {
    let parsed = Url::parse(raw_url);

    if let Ok(url) = parsed {
        if let Some(segment) = url
            .path_segments()
            .and_then(|segments| segments.filter(|s| !s.trim().is_empty()).last())
        {
            let decoded = percent_encoding::percent_decode_str(segment)
                .decode_utf8_lossy()
                .to_string();

            let clean = sanitize(decoded);

            if !clean.trim().is_empty() && clean.contains('.') {
                return clean;
            }
        }
    }

    format!("download-{}.bin", index + 1)
}

fn add_suffix_to_filename(filename: &str, counter: usize) -> String {
    let path = Path::new(filename);

    let stem = path
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("download");

    let extension = path.extension().and_then(|v| v.to_str());

    match extension {
        Some(ext) => format!("{} ({}).{}", stem, counter, ext),
        None => format!("{} ({})", stem, counter),
    }
}

fn github_release_api_url(raw_url: &str) -> Option<String> {
    let parsed = Url::parse(raw_url).ok()?;

    if parsed.host_str()? != "github.com" {
        return None;
    }

    let segments: Vec<String> = parsed
        .path_segments()?
        .filter(|segment| !segment.trim().is_empty())
        .map(|segment| segment.to_string())
        .collect();

    if segments.len() < 4 {
        return None;
    }

    let owner = &segments[0];
    let repo = &segments[1];

    if segments[2] != "releases" {
        return None;
    }

    match segments[3].as_str() {
        "latest" => Some(format!(
            "https://api.github.com/repos/{owner}/{repo}/releases/latest"
        )),
        "tag" => {
            if segments.len() >= 5 {
                Some(format!(
                    "https://api.github.com/repos/{owner}/{repo}/releases/tags/{}",
                    segments[4]
                ))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn asset_score(name: &str) -> i32 {
    let lower = name.to_lowercase();

    if lower.contains("portable") && lower.ends_with(".exe") {
        return 100;
    }

    if lower.ends_with(".exe") && !lower.contains("installer") && !lower.contains("setup") {
        return 90;
    }

    if lower.ends_with(".exe") {
        return 80;
    }

    if lower.ends_with(".zip") {
        return 60;
    }

    if lower.ends_with(".msi") {
        return 50;
    }

    10
}

async fn resolve_github_release_asset(
    client: &reqwest::Client,
    raw_url: &str,
) -> Option<DownloadJob> {
    let api_url = github_release_api_url(raw_url)?;

    let response = client
        .get(api_url)
        .header("User-Agent", "MPTech-Link-Downloader")
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let json: Value = response.json().await.ok()?;
    let assets = json.get("assets")?.as_array()?;

    let best_asset = assets
        .iter()
        .filter_map(|asset| {
            let name = asset.get("name")?.as_str()?.to_string();
            let download_url = asset.get("browser_download_url")?.as_str()?.to_string();

            Some((asset_score(&name), name, download_url))
        })
        .max_by_key(|(score, _, _)| *score)?;

    Some(DownloadJob {
        source_url: raw_url.to_string(),
        download_url: best_asset.2,
        filename: sanitize(best_asset.1),
    })
}

async fn build_download_jobs(client: &reqwest::Client, urls: Vec<String>) -> Vec<DownloadJob> {
    let mut jobs = Vec::new();

    for (index, url) in urls.into_iter().enumerate() {
        if let Some(job) = resolve_github_release_asset(client, &url).await {
            jobs.push(job);
        } else {
            jobs.push(DownloadJob {
                source_url: url.clone(),
                download_url: url.clone(),
                filename: filename_from_url(&url, index),
            });
        }
    }

    let mut filename_counts: HashMap<String, usize> = HashMap::new();

    jobs.into_iter()
        .map(|mut job| {
            let count = filename_counts.entry(job.filename.clone()).or_insert(0);

            if *count > 0 {
                job.filename = add_suffix_to_filename(&job.filename, *count);
            }

            *count += 1;
            job
        })
        .collect()
}

async fn avoid_existing_file(folder: &Path, filename: &str) -> PathBuf {
    let original_path = folder.join(filename);

    if tokio::fs::metadata(&original_path).await.is_err() {
        return original_path;
    }

    for counter in 1..10_000 {
        let candidate_name = add_suffix_to_filename(filename, counter);
        let candidate_path = folder.join(candidate_name);

        if tokio::fs::metadata(&candidate_path).await.is_err() {
            return candidate_path;
        }
    }

    folder.join(format!("download-copy-{}.bin", timestamp()))
}

fn timestamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

async fn download_one(client: reqwest::Client, job: DownloadJob, folder: String) -> DownloadResult {
    let folder_path = PathBuf::from(&folder);

    if tokio::fs::metadata(&folder_path).await.is_err() {
        return DownloadResult {
            url: job.source_url,
            filename: job.filename,
            ok: false,
            message: "Destination folder does not exist".to_string(),
        };
    }

    let final_path = avoid_existing_file(&folder_path, &job.filename).await;

    let response = match client.get(&job.download_url).send().await {
        Ok(response) => response,
        Err(error) => {
            return DownloadResult {
                url: job.source_url,
                filename: job.filename,
                ok: false,
                message: format!("Connection error: {error}"),
            };
        }
    };

    if !response.status().is_success() {
        return DownloadResult {
            url: job.source_url,
            filename: job.filename,
            ok: false,
            message: format!("HTTP {}", response.status()),
        };
    }

    let mut file = match tokio::fs::File::create(&final_path).await {
        Ok(file) => file,
        Err(error) => {
            return DownloadResult {
                url: job.source_url,
                filename: job.filename,
                ok: false,
                message: format!("Could not create file: {error}"),
            };
        }
    };

    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                if let Err(error) = file.write_all(&chunk).await {
                    return DownloadResult {
                        url: job.source_url,
                        filename: job.filename,
                        ok: false,
                        message: format!("File write error: {error}"),
                    };
                }
            }
            Err(error) => {
                return DownloadResult {
                    url: job.source_url,
                    filename: job.filename,
                    ok: false,
                    message: format!("Download stream error: {error}"),
                };
            }
        }
    }

    let message = if job.download_url != job.source_url {
        "Completed - GitHub release asset resolved".to_string()
    } else {
        "Completed".to_string()
    };

    DownloadResult {
        url: job.source_url,
        filename: final_path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or(&job.filename)
            .to_string(),
        ok: true,
        message,
    }
}

#[tauri::command]
async fn download_files(request: DownloadRequest) -> Vec<DownloadResult> {
    let concurrency = request.concurrency.clamp(1, 20);
    let folder = request.folder.clone();

    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return request
                .urls
                .into_iter()
                .enumerate()
                .map(|(index, url)| DownloadResult {
                    url,
                    filename: format!("download-{}.bin", index + 1),
                    ok: false,
                    message: format!("Could not create HTTP client: {error}"),
                })
                .collect();
        }
    };

    let jobs = build_download_jobs(&client, request.urls).await;

    stream::iter(jobs)
        .map(|job| {
            let client = client.clone();
            let folder = folder.clone();

            async move { download_one(client, job, folder).await }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await
}

#[tauri::command]
fn open_folder(folder: String) -> Result<(), String> {
    let path = PathBuf::from(&folder);

    if !path.exists() {
        return Err("Folder does not exist".to_string());
    }

    if !path.is_dir() {
        return Err("Path is not a folder".to_string());
    }

    Command::new("explorer")
        .arg(path)
        .spawn()
        .map_err(|error| format!("Could not open Explorer: {error}"))?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![download_files, open_folder])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}