use futures_util::stream::{self, StreamExt};
use sanitize_filename::sanitize;
use serde::{Deserialize, Serialize};
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
    url: String,
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

fn build_download_jobs(urls: Vec<String>) -> Vec<DownloadJob> {
    let mut filename_counts: HashMap<String, usize> = HashMap::new();

    urls.into_iter()
        .enumerate()
        .map(|(index, url)| {
            let base_filename = filename_from_url(&url, index);
            let count = filename_counts.entry(base_filename.clone()).or_insert(0);

            let final_filename = if *count == 0 {
                base_filename.clone()
            } else {
                add_suffix_to_filename(&base_filename, *count)
            };

            *count += 1;

            DownloadJob {
                url,
                filename: final_filename,
            }
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
            url: job.url,
            filename: job.filename,
            ok: false,
            message: "La carpeta destino no existe".to_string(),
        };
    }

    let final_path = avoid_existing_file(&folder_path, &job.filename).await;

    let response = match client.get(&job.url).send().await {
        Ok(response) => response,
        Err(error) => {
            return DownloadResult {
                url: job.url,
                filename: job.filename,
                ok: false,
                message: format!("Error de conexión: {error}"),
            };
        }
    };

    if !response.status().is_success() {
        return DownloadResult {
            url: job.url,
            filename: job.filename,
            ok: false,
            message: format!("HTTP {}", response.status()),
        };
    }

    let mut file = match tokio::fs::File::create(&final_path).await {
        Ok(file) => file,
        Err(error) => {
            return DownloadResult {
                url: job.url,
                filename: job.filename,
                ok: false,
                message: format!("No se pudo crear el archivo: {error}"),
            };
        }
    };

    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                if let Err(error) = file.write_all(&chunk).await {
                    return DownloadResult {
                        url: job.url,
                        filename: job.filename,
                        ok: false,
                        message: format!("Error escribiendo archivo: {error}"),
                    };
                }
            }
            Err(error) => {
                return DownloadResult {
                    url: job.url,
                    filename: job.filename,
                    ok: false,
                    message: format!("Error leyendo descarga: {error}"),
                };
            }
        }
    }

    DownloadResult {
        url: job.url,
        filename: final_path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or(&job.filename)
            .to_string(),
        ok: true,
        message: "Completado".to_string(),
    }
}

#[tauri::command]
async fn download_files(request: DownloadRequest) -> Vec<DownloadResult> {
    let concurrency = request.concurrency.clamp(1, 20);
    let folder = request.folder.clone();
    let jobs = build_download_jobs(request.urls);

    let client = match reqwest::Client::builder().build() {
        Ok(client) => client,
        Err(error) => {
            return jobs
                .into_iter()
                .map(|job| DownloadResult {
                    url: job.url,
                    filename: job.filename,
                    ok: false,
                    message: format!("No se pudo crear el cliente HTTP: {error}"),
                })
                .collect();
        }
    };

    stream::iter(jobs)
        .map(|job| {
            let client = client.clone();
            let folder = folder.clone();

            async move {
                download_one(client, job, folder).await
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await
}

#[tauri::command]
fn open_folder(folder: String) -> Result<(), String> {
    let path = PathBuf::from(&folder);

    if !path.exists() {
        return Err("La carpeta no existe".to_string());
    }

    if !path.is_dir() {
        return Err("La ruta no es una carpeta".to_string());
    }

    Command::new("explorer")
        .arg(path)
        .spawn()
        .map_err(|error| format!("No se pudo abrir el explorador: {error}"))?;

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
