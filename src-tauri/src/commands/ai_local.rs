//! Optional, verified local inference assets. Nothing is downloaded at startup.
use std::{
    fs,
    io::Read,
    net::TcpListener,
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::Duration,
};

use reqwest::Client;
use serde::Serialize;
use sha2::{Digest, Sha256};
use strawberrydisk_core::ai::{AiConfiguration, AiError, AiServiceMode, ReasoningMode};
use tauri::{ipc::Channel, AppHandle, Manager, State};
use tokio::{io::AsyncWriteExt, sync::watch};

const MODEL_NAME: &str = "Qwen3-0.6B.Q4_K_M.gguf";
const MODEL_URL: &str =
    "https://huggingface.co/QuantFactory/Qwen3-0.6B-GGUF/resolve/main/Qwen3-0.6B.Q4_K_M.gguf";
const MODEL_SHA256: &str = "7af3fdf842f87b24672f8a7f1dd50404043f0bfb71093ff91c31d2b49df4631d";
const MODEL_MAX_BYTES: u64 = 550_000_000;
const RUNTIME_TAG: &str = "b11415";
const RUNTIME_MAX_BYTES: u64 = 75_000_000;

struct RuntimeArtifact {
    name: &'static str,
    sha256: &'static str,
}

fn runtime_artifact() -> Option<RuntimeArtifact> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some(RuntimeArtifact {
            name: "llama-b11415-bin-win-cpu-x64.zip",
            sha256: "2d96c8540f978638a8f64bc90e87c94ed16e8ccef6e62798007e9ce3f181d8cd",
        }),
        ("windows", "aarch64") => Some(RuntimeArtifact {
            name: "llama-b11415-bin-win-cpu-arm64.zip",
            sha256: "90691213229ca280a924a972be45b0f052ad5e524ad53a9a4a4bb35018d42572",
        }),
        ("macos", "aarch64") => Some(RuntimeArtifact {
            name: "llama-b11415-bin-macos-arm64.tar.gz",
            sha256: "79ecce300907d8884e7d72c7e13325238ce5fd462e07202951f5a44ad2c758fc",
        }),
        ("macos", "x86_64") => Some(RuntimeArtifact {
            name: "llama-b11415-bin-macos-x64.tar.gz",
            sha256: "cff91b9e720cc0c871a76637515acf4ab112519e2058c1911ff75716023f933e",
        }),
        ("linux", "x86_64") => Some(RuntimeArtifact {
            name: "llama-b11415-bin-ubuntu-x64.tar.gz",
            sha256: "fa86a5745dd22fb1c9011db6af90b7f7518c615a4784400328be1b748f9986f0",
        }),
        ("linux", "aarch64") => Some(RuntimeArtifact {
            name: "llama-b11415-bin-ubuntu-arm64.tar.gz",
            sha256: "9d1723aee5d5f71504987f127a69d6da3c46dd3b7343d3ec754faf1ad64470d9",
        }),
        _ => None,
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalAiStatus {
    pub supported: bool,
    pub installed: bool,
    pub model_bytes: u64,
    pub download_bytes: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalAiProgress {
    pub stage: &'static str,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

struct LocalServer {
    child: Child,
    endpoint: String,
    key: String,
}

#[derive(Default)]
pub(crate) struct LocalAiRuntime {
    install: Mutex<Option<watch::Sender<bool>>>,
    server: tokio::sync::Mutex<Option<LocalServer>>,
}

impl Drop for LocalAiRuntime {
    fn drop(&mut self) {
        if let Ok(mut server) = self.server.try_lock() {
            if let Some(server) = server.as_mut() {
                let _ = server.child.kill();
                let _ = server.child.wait();
            }
        }
    }
}

fn asset_root(app: &AppHandle) -> Result<PathBuf, AiError> {
    app.path()
        .app_local_data_dir()
        .map(|path| path.join("ai-local").join("v1"))
        .map_err(|_| AiError::ConfigurationUnavailable)
}

fn model_path(root: &Path) -> PathBuf {
    root.join(MODEL_NAME)
}

fn server_path(root: &Path) -> Option<PathBuf> {
    fn find(directory: &Path, depth: usize, binary: &str) -> Option<PathBuf> {
        if depth > 3 {
            return None;
        }
        for entry in fs::read_dir(directory).ok()?.flatten() {
            let path = entry.path();
            let kind = entry.file_type().ok()?;
            if kind.is_file() && entry.file_name() == binary {
                return Some(path);
            }
            if kind.is_dir() {
                if let Some(found) = find(&path, depth + 1, binary) {
                    return Some(found);
                }
            }
        }
        None
    }
    find(
        &root.join("runtime"),
        0,
        if cfg!(windows) {
            "llama-server.exe"
        } else {
            "llama-server"
        },
    )
}

fn status_for(root: &Path) -> LocalAiStatus {
    let model_bytes = fs::symlink_metadata(model_path(root))
        .ok()
        .filter(|meta| meta.file_type().is_file())
        .map(|meta| meta.len())
        .unwrap_or(0);
    LocalAiStatus {
        supported: runtime_artifact().is_some(),
        installed: model_bytes > 0 && server_path(root).is_some(),
        model_bytes,
        download_bytes: 484_000_000 + 20_000_000,
    }
}

#[tauri::command]
pub(crate) fn ai_local_status(app: AppHandle) -> Result<LocalAiStatus, AiError> {
    Ok(status_for(&asset_root(&app)?))
}

fn hex_sha256(path: &Path) -> Result<String, AiError> {
    let mut file = fs::File::open(path).map_err(|_| AiError::LocalIntegrityFailed)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| AiError::LocalIntegrityFailed)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

struct DownloadSpec<'a> {
    url: &'a str,
    expected_hash: &'a str,
    max_bytes: u64,
    stage: &'static str,
}

async fn download(
    client: &Client,
    spec: DownloadSpec<'_>,
    target: &mut tokio::fs::File,
    cancelled: &mut watch::Receiver<bool>,
    progress: &Channel<LocalAiProgress>,
) -> Result<(), AiError> {
    let mut response = client
        .get(spec.url)
        .send()
        .await
        .map_err(|_| AiError::LocalDownloadFailed)?;
    if !response.status().is_success() {
        return Err(AiError::LocalDownloadFailed);
    }
    let total = response.content_length();
    if total.is_some_and(|size| size > spec.max_bytes) {
        return Err(AiError::LocalDownloadFailed);
    }
    if *cancelled.borrow() {
        return Err(AiError::Cancelled);
    }
    let mut hash = Sha256::new();
    let mut received = 0;
    loop {
        let chunk = tokio::select! {
            biased;
            _ = cancelled.changed() => return Err(AiError::Cancelled),
            chunk = response.chunk() => chunk.map_err(|_| AiError::LocalDownloadFailed)?,
        };
        let Some(chunk) = chunk else { break };
        received += chunk.len() as u64;
        if received > spec.max_bytes {
            return Err(AiError::LocalDownloadFailed);
        }
        hash.update(&chunk);
        target
            .write_all(&chunk)
            .await
            .map_err(|_| AiError::ConfigurationUnavailable)?;
        let _ = progress.send(LocalAiProgress {
            stage: spec.stage,
            downloaded_bytes: received,
            total_bytes: total,
        });
    }
    target
        .flush()
        .await
        .map_err(|_| AiError::ConfigurationUnavailable)?;
    target
        .sync_all()
        .await
        .map_err(|_| AiError::ConfigurationUnavailable)?;
    if format!("{:x}", hash.finalize()) != spec.expected_hash {
        return Err(AiError::LocalIntegrityFailed);
    }
    Ok(())
}

fn safe_archive_path(path: &Path) -> bool {
    path.components()
        .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
}

fn unpack_runtime(archive: &Path, destination: &Path) -> Result<(), AiError> {
    fs::create_dir_all(destination).map_err(|_| AiError::ConfigurationUnavailable)?;
    let mut count = 0usize;
    let mut expanded = 0u64;
    if cfg!(windows) {
        let file = fs::File::open(archive).map_err(|_| AiError::LocalIntegrityFailed)?;
        let mut zip = zip::ZipArchive::new(file).map_err(|_| AiError::LocalIntegrityFailed)?;
        for index in 0..zip.len() {
            let mut entry = zip
                .by_index(index)
                .map_err(|_| AiError::LocalIntegrityFailed)?;
            let relative = entry.enclosed_name().ok_or(AiError::LocalIntegrityFailed)?;
            if !safe_archive_path(&relative) {
                return Err(AiError::LocalIntegrityFailed);
            }
            count += 1;
            expanded += entry.size();
            if count > 128 || expanded > 250_000_000 {
                return Err(AiError::LocalIntegrityFailed);
            }
            let output = destination.join(relative);
            if entry.is_dir() {
                fs::create_dir_all(output).map_err(|_| AiError::ConfigurationUnavailable)?;
            } else {
                if let Some(parent) = output.parent() {
                    fs::create_dir_all(parent).map_err(|_| AiError::ConfigurationUnavailable)?;
                }
                let mut file =
                    fs::File::create(output).map_err(|_| AiError::ConfigurationUnavailable)?;
                std::io::copy(&mut entry, &mut file)
                    .map_err(|_| AiError::ConfigurationUnavailable)?;
            }
        }
    } else {
        let file = fs::File::open(archive).map_err(|_| AiError::LocalIntegrityFailed)?;
        let gzip = flate2::read::GzDecoder::new(file);
        let mut tar = tar::Archive::new(gzip);
        for entry in tar.entries().map_err(|_| AiError::LocalIntegrityFailed)? {
            let mut entry = entry.map_err(|_| AiError::LocalIntegrityFailed)?;
            let kind = entry.header().entry_type();
            if !kind.is_file() && !kind.is_dir() {
                return Err(AiError::LocalIntegrityFailed);
            }
            let relative = entry.path().map_err(|_| AiError::LocalIntegrityFailed)?;
            if !safe_archive_path(&relative) {
                return Err(AiError::LocalIntegrityFailed);
            }
            count += 1;
            expanded += entry.size();
            if count > 128 || expanded > 250_000_000 {
                return Err(AiError::LocalIntegrityFailed);
            }
            entry
                .unpack_in(destination)
                .map_err(|_| AiError::ConfigurationUnavailable)?;
        }
    }
    if server_path_in(destination).is_none() {
        return Err(AiError::LocalIntegrityFailed);
    }
    Ok(())
}

fn server_path_in(directory: &Path) -> Option<PathBuf> {
    fn find(directory: &Path, depth: usize, binary: &str) -> Option<PathBuf> {
        if depth > 3 {
            return None;
        }
        for entry in fs::read_dir(directory).ok()?.flatten() {
            let path = entry.path();
            if entry.file_type().ok()?.is_file() && entry.file_name() == binary {
                return Some(path);
            }
            if entry.file_type().ok()?.is_dir() {
                if let Some(found) = find(&path, depth + 1, binary) {
                    return Some(found);
                }
            }
        }
        None
    }
    find(
        directory,
        0,
        if cfg!(windows) {
            "llama-server.exe"
        } else {
            "llama-server"
        },
    )
}

#[tauri::command]
pub(crate) async fn ai_local_download(
    app: AppHandle,
    state: State<'_, LocalAiRuntime>,
    on_progress: Channel<LocalAiProgress>,
) -> Result<LocalAiStatus, AiError> {
    let artifact = runtime_artifact().ok_or(AiError::LocalUnsupported)?;
    let root = asset_root(&app)?;
    fs::create_dir_all(&root).map_err(|_| AiError::ConfigurationUnavailable)?;
    let (sender, mut cancelled) = watch::channel(false);
    {
        let mut install = state.install.lock().map_err(|_| AiError::Busy)?;
        if install.is_some() {
            return Err(AiError::Busy);
        }
        *install = Some(sender);
    }
    let result = async {
        let client = Client::builder()
            .timeout(Duration::from_secs(1800))
            .build()
            .map_err(|_| AiError::LocalDownloadFailed)?;
        if server_path(&root).is_none() {
            let archive = tempfile::NamedTempFile::new_in(&root)
                .map_err(|_| AiError::ConfigurationUnavailable)?;
            let extracted =
                tempfile::tempdir_in(&root).map_err(|_| AiError::ConfigurationUnavailable)?;
            let url = format!(
                "https://github.com/ggml-org/llama.cpp/releases/download/{RUNTIME_TAG}/{}",
                artifact.name
            );
            let mut output = tokio::fs::File::from_std(
                archive
                    .reopen()
                    .map_err(|_| AiError::ConfigurationUnavailable)?,
            );
            download(
                &client,
                DownloadSpec {
                    url: &url,
                    expected_hash: artifact.sha256,
                    max_bytes: RUNTIME_MAX_BYTES,
                    stage: "runtime",
                },
                &mut output,
                &mut cancelled,
                &on_progress,
            )
            .await?;
            drop(output);
            let archive_copy = archive.path().to_owned();
            let destination = extracted.path().to_owned();
            tauri::async_runtime::spawn_blocking(move || {
                unpack_runtime(&archive_copy, &destination)
            })
            .await
            .map_err(|_| AiError::LocalIntegrityFailed)??;
            if *cancelled.borrow() {
                return Err(AiError::Cancelled);
            }
            remove_runtime(&root)?;
            fs::rename(extracted.path(), root.join("runtime"))
                .map_err(|_| AiError::ConfigurationUnavailable)?;
        }
        if status_for(&root).model_bytes == 0 {
            let model = tempfile::NamedTempFile::new_in(&root)
                .map_err(|_| AiError::ConfigurationUnavailable)?;
            let mut output = tokio::fs::File::from_std(
                model
                    .reopen()
                    .map_err(|_| AiError::ConfigurationUnavailable)?,
            );
            download(
                &client,
                DownloadSpec {
                    url: MODEL_URL,
                    expected_hash: MODEL_SHA256,
                    max_bytes: MODEL_MAX_BYTES,
                    stage: "model",
                },
                &mut output,
                &mut cancelled,
                &on_progress,
            )
            .await?;
            drop(output);
            if *cancelled.borrow() {
                return Err(AiError::Cancelled);
            }
            if fs::symlink_metadata(model_path(&root)).is_ok() {
                return Err(AiError::ConfigurationUnavailable);
            }
            model
                .persist(model_path(&root))
                .map_err(|_| AiError::ConfigurationUnavailable)?;
        }
        let status = status_for(&root);
        if !status.installed {
            return Err(AiError::ModelUnavailable);
        }
        let _ = on_progress.send(LocalAiProgress {
            stage: "ready",
            downloaded_bytes: status.model_bytes,
            total_bytes: Some(status.model_bytes),
        });
        Ok(status)
    }
    .await;
    if let Ok(mut install) = state.install.lock() {
        *install = None;
    }
    log::info!(
        "ai_local_download_finished success={} reason={:?}",
        result.is_ok(),
        result.as_ref().err()
    );
    result
}

#[tauri::command]
pub(crate) fn ai_local_cancel_download(state: State<'_, LocalAiRuntime>) -> Result<(), AiError> {
    if let Some(sender) = state.install.lock().map_err(|_| AiError::Busy)?.as_ref() {
        sender.send_replace(true);
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn ai_local_remove(
    app: AppHandle,
    state: State<'_, LocalAiRuntime>,
    requests: State<'_, super::ai::AiRuntime>,
) -> Result<LocalAiStatus, AiError> {
    if state.install.lock().map_err(|_| AiError::Busy)?.is_some() {
        return Err(AiError::Busy);
    }
    if requests.has_running()? {
        return Err(AiError::Busy);
    }
    let mut server = state.server.lock().await;
    if let Some(mut running) = server.take() {
        running.child.kill().map_err(|_| AiError::Busy)?;
        let _ = running.child.wait();
    }
    let root = asset_root(&app)?;
    if let Ok(meta) = fs::symlink_metadata(model_path(&root)) {
        if !meta.file_type().is_file() {
            return Err(AiError::ConfigurationUnavailable);
        }
        fs::remove_file(model_path(&root)).map_err(|_| AiError::ConfigurationUnavailable)?;
    }
    remove_runtime(&root)?;
    Ok(status_for(&root))
}

fn remove_runtime(root: &Path) -> Result<(), AiError> {
    let runtime = root.join("runtime");
    if let Ok(meta) = fs::symlink_metadata(&runtime) {
        if meta.file_type().is_symlink() {
            return Err(AiError::ConfigurationUnavailable);
        }
        let canonical_root =
            fs::canonicalize(root).map_err(|_| AiError::ConfigurationUnavailable)?;
        let canonical_runtime =
            fs::canonicalize(&runtime).map_err(|_| AiError::ConfigurationUnavailable)?;
        if canonical_runtime.parent() != Some(canonical_root.as_path()) {
            return Err(AiError::ConfigurationUnavailable);
        }
        fs::remove_dir_all(runtime).map_err(|_| AiError::ConfigurationUnavailable)?;
    }
    Ok(())
}

impl LocalAiRuntime {
    pub(crate) async fn configuration(&self, app: &AppHandle) -> Result<AiConfiguration, AiError> {
        let root = asset_root(app)?;
        if !status_for(&root).installed {
            return Err(AiError::ModelUnavailable);
        }
        let mut guard = self.server.lock().await;
        if let Some(server) = guard.as_mut() {
            if server
                .child
                .try_wait()
                .map_err(|_| AiError::LocalLaunchFailed)?
                .is_none()
            {
                return Ok(local_configuration(&server.endpoint, &server.key));
            }
            *guard = None;
        }
        let model = model_path(&root);
        let expected = MODEL_SHA256;
        let model_copy = model.clone();
        if tauri::async_runtime::spawn_blocking(move || hex_sha256(&model_copy))
            .await
            .map_err(|_| AiError::LocalIntegrityFailed)??
            != expected
        {
            return Err(AiError::LocalIntegrityFailed);
        }
        let binary = server_path(&root).ok_or(AiError::ModelUnavailable)?;
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| AiError::LocalLaunchFailed)?;
        let port = listener
            .local_addr()
            .map_err(|_| AiError::LocalLaunchFailed)?
            .port();
        drop(listener);
        let key = uuid::Uuid::new_v4().to_string();
        let endpoint = format!("http://127.0.0.1:{port}/v1");
        let child = Command::new(&binary)
            .current_dir(binary.parent().ok_or(AiError::LocalLaunchFailed)?)
            .env(
                "LLAMA_ARG_CHAT_TEMPLATE_KWARGS",
                "{\"enable_thinking\":false}",
            )
            .args([
                "--model",
                model.to_str().ok_or(AiError::LocalLaunchFailed)?,
                "--alias",
                "strawberrydisk-local",
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--api-key",
                &key,
                "--ctx-size",
                "4096",
                "--threads",
                "4",
                "--poll",
                "0",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| AiError::LocalLaunchFailed)?;
        *guard = Some(LocalServer {
            child,
            endpoint: endpoint.clone(),
            key: key.clone(),
        });
        let client = Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|_| AiError::LocalLaunchFailed)?;
        for _ in 0..120 {
            if guard
                .as_mut()
                .unwrap()
                .child
                .try_wait()
                .map_err(|_| AiError::LocalLaunchFailed)?
                .is_some()
            {
                break;
            }
            if client
                .get(format!("http://127.0.0.1:{port}/health"))
                .bearer_auth(&key)
                .send()
                .await
                .is_ok_and(|reply| reply.status().is_success())
            {
                return Ok(local_configuration(&endpoint, &key));
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        if let Some(mut server) = guard.take() {
            let _ = server.child.kill();
            let _ = server.child.wait();
        }
        Err(AiError::LocalLaunchFailed)
    }
}

fn local_configuration(endpoint: &str, key: &str) -> AiConfiguration {
    let mut config = AiConfiguration::initial();
    config.mode = AiServiceMode::Custom;
    config.endpoint = endpoint.to_owned();
    config.model = "strawberrydisk-local".to_owned();
    config.api_key = key.to_owned();
    config.reasoning = ReasoningMode::Default;
    config.max_tokens = Some(512);
    config.temperature = Some(0.3);
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn archive_paths_cannot_escape_the_local_directory() {
        assert!(safe_archive_path(Path::new("bin/llama-server")));
        assert!(!safe_archive_path(Path::new("../llama-server")));
        assert!(!safe_archive_path(Path::new("/tmp/llama-server")));
    }

    #[test]
    fn file_hash_matches_known_sha256() {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), b"abc").unwrap();
        assert_eq!(
            hex_sha256(file.path()).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[cfg(windows)]
    #[test]
    fn runtime_archive_rejects_parent_path() {
        use zip::{write::SimpleFileOptions, ZipWriter};
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("runtime.zip");
        let file = fs::File::create(&archive).unwrap();
        let mut writer = ZipWriter::new(file);
        writer
            .start_file("../outside.exe", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"no").unwrap();
        writer.finish().unwrap();
        assert!(matches!(
            unpack_runtime(&archive, &directory.path().join("runtime")),
            Err(AiError::LocalIntegrityFailed)
        ));
        assert!(!directory.path().join("outside.exe").exists());
    }
}
