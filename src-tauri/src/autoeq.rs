use std::{
    fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use flate2::read::GzDecoder;
use reqwest::{blocking::Client, header};
use serde::{Deserialize, Serialize};
use tar::Archive;
use tauri::{AppHandle, Emitter, Runtime};

use crate::{
    logging::append_log_line,
    state::{AppError, APP_FOLDER_NAME},
};

pub const AUTOEQ_PROGRESS_EVENT: &str = "smart-eq://autoeq-progress";

const INDEX_URL: &str =
    "https://raw.githubusercontent.com/timschneeb/AutoEqPackages/main/index.json";
const VERSION_URL: &str =
    "https://raw.githubusercontent.com/timschneeb/AutoEqPackages/main/version.json";
const ARCHIVE_URL_FALLBACK: &str =
    "https://raw.githubusercontent.com/timschneeb/AutoEqPackages/main/archive.tar.gz";
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 12);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoEqIndexEntry {
    pub n: String,
    pub s: String,
    pub r: i32,
    pub i: u32,
}


#[derive(Debug, Clone, Copy, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AutoEqPresetVariant {
    Auto,
    Parametric,
    Graphic,
}

impl AutoEqPresetVariant {
    fn preferred_order(self) -> &'static [AutoEqPresetKind] {
        match self {
            AutoEqPresetVariant::Auto => {
                #[cfg(any(target_os = "linux", target_os = "windows"))]
                {
                    &[AutoEqPresetKind::Parametric, AutoEqPresetKind::Graphic]
                }
                #[cfg(not(any(target_os = "linux", target_os = "windows")))]
                {
                    &[AutoEqPresetKind::Graphic, AutoEqPresetKind::Parametric]
                }
            }
            AutoEqPresetVariant::Parametric => &[AutoEqPresetKind::Parametric],
            AutoEqPresetVariant::Graphic => &[AutoEqPresetKind::Graphic],
        }
    }

    fn label(self) -> &'static str {
        match self {
            AutoEqPresetVariant::Auto => "Auto-selected",
            AutoEqPresetVariant::Parametric => "ParametricEQ",
            AutoEqPresetVariant::Graphic => "GraphicEQ",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum AutoEqPresetKind {
    Parametric,
    Graphic,
}

impl AutoEqPresetKind {
    fn cache_key(self) -> &'static str {
        match self {
            Self::Parametric => "parametric",
            Self::Graphic => "graphic",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Parametric => "ParametricEQ",
            Self::Graphic => "GraphicEQ",
        }
    }

    fn ready_message(self, source: CachedArtifactSource) -> &'static str {
        match (self, source) {
            (Self::Parametric, CachedArtifactSource::Cache) => "ParametricEQ preset ready from cache.",
            (Self::Parametric, CachedArtifactSource::Network) => {
                "ParametricEQ preset ready from downloaded package."
            }
            (Self::Parametric, CachedArtifactSource::StaleCache) => {
                "ParametricEQ preset ready from stale cached package."
            }
            (Self::Graphic, CachedArtifactSource::Cache) => "GraphicEQ preset ready from cache.",
            (Self::Graphic, CachedArtifactSource::Network) => {
                "GraphicEQ preset ready from downloaded package."
            }
            (Self::Graphic, CachedArtifactSource::StaleCache) => {
                "GraphicEQ preset ready from stale cached package."
            }
        }
    }
}


#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutoEqProgressOperation {
    Index,
    Preset,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutoEqProgressPhase {
    Start,
    CheckCache,
    FetchIndex,
    FetchVersion,
    DownloadArchive,
    ExtractPreset,
    CacheHit,
    Done,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutoEqProgressSource {
    Cache,
    Network,
    StaleCache,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoEqProgressPayload {
    pub operation: AutoEqProgressOperation,
    pub phase: AutoEqProgressPhase,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<AutoEqProgressSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset_source: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AutoEqVersionEntry {
    package_url: String,
    commit: String,
}

#[derive(Debug, Clone, Copy)]
enum CachedArtifactSource {
    Cache,
    Network,
    StaleCache,
}

impl CachedArtifactSource {
    fn progress_source(self) -> AutoEqProgressSource {
        match self {
            Self::Cache => AutoEqProgressSource::Cache,
            Self::Network => AutoEqProgressSource::Network,
            Self::StaleCache => AutoEqProgressSource::StaleCache,
        }
    }
}

struct ArchiveResolution {
    path: PathBuf,
    source: CachedArtifactSource,
}

pub fn load_index<R: Runtime>(
    app: &AppHandle<R>,
    force_refresh: bool,
) -> Result<Vec<AutoEqIndexEntry>, AppError> {
    emit_index_progress(
        app,
        AutoEqProgressPhase::Start,
        "Preparing AutoEQ index.",
        None,
    );
    emit_index_progress(
        app,
        AutoEqProgressPhase::CheckCache,
        "Checking local AutoEQ index cache.",
        None,
    );

    let cache_path = index_cache_path()?;

    if !force_refresh && cache_path.exists() && is_cache_recent(&cache_path) {
        emit_index_progress(
            app,
            AutoEqProgressPhase::CacheHit,
            "Loaded AutoEQ index from cache.",
            Some(AutoEqProgressSource::Cache),
        );
        let entries = read_index_from_disk(&cache_path)?;
        emit_index_progress(
            app,
            AutoEqProgressPhase::Done,
            "AutoEQ index ready.",
            Some(AutoEqProgressSource::Cache),
        );
        return Ok(entries);
    }

    emit_index_progress(
        app,
        AutoEqProgressPhase::FetchIndex,
        "Fetching the latest AutoEQ index from GitHub.",
        None,
    );

    match download_text(INDEX_URL) {
        Ok(text) => {
            let entries = parse_index(&text)?;
            write_text_atomically(&cache_path, &text)?;
            emit_index_progress(
                app,
                AutoEqProgressPhase::Done,
                "AutoEQ index ready.",
                Some(AutoEqProgressSource::Network),
            );
            Ok(entries)
        }
        Err(error) => {
            if cache_path.exists() {
                append_log_line(
                    "WARN",
                    format!(
                        "Failed to refresh AutoEQ index from GitHub; using cached copy instead: {error}"
                    ),
                );
                emit_index_progress(
                    app,
                    AutoEqProgressPhase::CacheHit,
                    "GitHub unavailable. Using cached AutoEQ index.",
                    Some(AutoEqProgressSource::StaleCache),
                );
                let entries = read_index_from_disk(&cache_path)?;
                emit_index_progress(
                    app,
                    AutoEqProgressPhase::Done,
                    "AutoEQ index ready from stale cache.",
                    Some(AutoEqProgressSource::StaleCache),
                );
                return Ok(entries);
            }

            let message = format!("Failed to load the AutoEQ index from GitHub: {error}");
            emit_index_progress(app, AutoEqProgressPhase::Error, message.as_str(), None);
            Err(AppError::Message(message))
        }
    }
}

pub fn get_graphic_preset<R: Runtime>(
    app: &AppHandle<R>,
    name: &str,
    source: &str,
) -> Result<String, AppError> {
    get_preset_variant(app, name, source, AutoEqPresetVariant::Graphic)
}

pub fn get_preset_variant<R: Runtime>(
    app: &AppHandle<R>,
    name: &str,
    source: &str,
    variant: AutoEqPresetVariant,
) -> Result<String, AppError> {
    emit_preset_progress(
        app,
        name,
        source,
        AutoEqProgressPhase::Start,
        format!("Preparing {} preset.", variant.label()).as_str(),
        None,
    );
    emit_preset_progress(
        app,
        name,
        source,
        AutoEqProgressPhase::CheckCache,
        "Checking local AutoEQ preset cache.",
        None,
    );

    let archive_path = archive_cache_path()?;

    for kind in variant.preferred_order() {
        let extracted_cache_path = extracted_preset_cache_path(name, source, kind.cache_key())?;

        if can_use_extracted_cache(&extracted_cache_path, &archive_path) {
            emit_preset_progress(
                app,
                name,
                source,
                AutoEqProgressPhase::CacheHit,
                format!("Loaded {} preset from cache.", kind.label()).as_str(),
                Some(AutoEqProgressSource::Cache),
            );
            let content = fs::read_to_string(&extracted_cache_path)?;
            emit_preset_progress(
                app,
                name,
                source,
                AutoEqProgressPhase::Done,
                format!("{} preset ready.", kind.label()).as_str(),
                Some(AutoEqProgressSource::Cache),
            );
            return Ok(content);
        }
    }

    let archive_resolution = ensure_archive_current(app, name, source)?;

    for kind in variant.preferred_order() {
        emit_preset_progress(
            app,
            name,
            source,
            AutoEqProgressPhase::ExtractPreset,
            format!("Extracting {} preset from the package archive.", kind.label()).as_str(),
            Some(archive_resolution.source.progress_source()),
        );

        match extract_preset_from_archive(&archive_resolution.path, name, source, *kind) {
            Ok(content) => {
                let extracted_cache_path =
                    extracted_preset_cache_path(name, source, kind.cache_key())?;
                write_text_atomically(&extracted_cache_path, &content)?;

                emit_preset_progress(
                    app,
                    name,
                    source,
                    AutoEqProgressPhase::Done,
                    kind.ready_message(archive_resolution.source),
                    Some(archive_resolution.source.progress_source()),
                );
                return Ok(content);
            }
            Err(error) => {
                append_log_line(
                    "INFO",
                    format!(
                        "{} preset was not available for '{} / {}': {}",
                        kind.label(),
                        name,
                        source,
                        error
                    ),
                );
            }
        }
    }

    let message = match variant {
        AutoEqPresetVariant::Auto => {
            "No compatible AutoEQ preset variant was found. Tried ParametricEQ first, then GraphicEQ fallback."
        }
        AutoEqPresetVariant::Parametric => {
            "ParametricEQ preset not found for this AutoEQ entry."
        }
        AutoEqPresetVariant::Graphic => "GraphicEQ preset not found for this AutoEQ entry.",
    };
    emit_preset_progress(app, name, source, AutoEqProgressPhase::Error, message, None);
    Err(AppError::Message(message.to_string()))
}

fn extract_preset_from_archive(
    archive_path: &Path,
    name: &str,
    source: &str,
    kind: AutoEqPresetKind,
) -> Result<String, AppError> {
    let archive_file = File::open(archive_path)?;
    let decoder = GzDecoder::new(archive_file);
    let mut archive = Archive::new(decoder);

    let base = normalize_archive_path(Path::new(name).join(source).as_path());
    let export_base = normalize_archive_path(Path::new("export").join(name).join(source).as_path());
    let exact_candidates = archive_exact_candidates(name, source, kind);

    let mut loose_match: Option<String> = None;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?;
        let normalized_path = normalize_archive_path(entry_path.as_ref());

        if exact_candidates.iter().any(|candidate| candidate == &normalized_path) {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            return Ok(content);
        }

        if loose_match.is_none()
            && archive_entry_matches_variant(normalized_path.as_str(), base.as_str(), export_base.as_str(), kind)
        {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            loose_match = Some(content);
        }
    }

    loose_match.ok_or_else(|| {
        AppError::Message(format!(
            "{} preset file was not found in AutoEQ package.",
            kind.label()
        ))
    })
}

fn archive_exact_candidates(name: &str, source: &str, kind: AutoEqPresetKind) -> Vec<String> {
    let names: &[&str] = match kind {
        AutoEqPresetKind::Graphic => &[
            "graphic.txt",
            "graphiceq.txt",
            "graphic_eq.txt",
            "GraphicEQ.txt",
        ],
        AutoEqPresetKind::Parametric => &[
            "parametric.txt",
            "parametric_eq.txt",
            "parametriceq.txt",
            "ParametricEQ.txt",
            "filters.txt",
            "Filters.txt",
            "eqapo.txt",
            "equalizerapo.txt",
            "EqualizerAPO.txt",
        ],
    };

    names
        .iter()
        .flat_map(|file_name| {
            [
                normalize_archive_path(Path::new(name).join(source).join(file_name).as_path()),
                normalize_archive_path(
                    Path::new("export")
                        .join(name)
                        .join(source)
                        .join(file_name)
                        .as_path(),
                ),
            ]
        })
        .collect()
}

fn archive_entry_matches_variant(
    normalized_path: &str,
    base: &str,
    export_base: &str,
    kind: AutoEqPresetKind,
) -> bool {
    let is_in_entry_dir = normalized_path.starts_with(&format!("{base}/"))
        || normalized_path.starts_with(&format!("{export_base}/"));
    if !is_in_entry_dir || !normalized_path.to_ascii_lowercase().ends_with(".txt") {
        return false;
    }

    let file_name = normalized_path
        .rsplit('/')
        .next()
        .unwrap_or(normalized_path)
        .to_ascii_lowercase();

    match kind {
        AutoEqPresetKind::Graphic => file_name.contains("graphic"),
        AutoEqPresetKind::Parametric => {
            file_name.contains("parametric")
                || file_name.contains("filter")
                || file_name.contains("eqapo")
                || file_name.contains("equalizerapo")
        }
    }
}

fn ensure_archive_current<R: Runtime>(
    app: &AppHandle<R>,
    preset_name: &str,
    preset_source: &str,
) -> Result<ArchiveResolution, AppError> {
    emit_preset_progress(
        app,
        preset_name,
        preset_source,
        AutoEqProgressPhase::FetchVersion,
        "Checking package metadata.",
        None,
    );

    let archive_path = archive_cache_path()?;
    let version_path = version_cache_path()?;
    let cached_version = read_version_entry(&version_path).ok();

    if archive_path.exists() && version_path.exists() && is_cache_recent(&version_path) {
        return Ok(ArchiveResolution {
            path: archive_path,
            source: CachedArtifactSource::Cache,
        });
    }

    match download_text(VERSION_URL) {
        Ok(text) => {
            let remote_version = parse_version_entry(&text)?;
            let should_download = !archive_path.exists()
                || cached_version
                    .as_ref()
                    .map(|cached| {
                        cached.commit != remote_version.commit
                            || cached.package_url != remote_version.package_url
                    })
                    .unwrap_or(true);

            if should_download {
                emit_preset_progress(
                    app,
                    preset_name,
                    preset_source,
                    AutoEqProgressPhase::DownloadArchive,
                    "Downloading AutoEQ package archive. First preview can take a bit; the next previews use cache.",
                    None,
                );
                let archive_url = if remote_version.package_url.trim().is_empty() {
                    ARCHIVE_URL_FALLBACK
                } else {
                    remote_version.package_url.as_str()
                };
                let archive_bytes = download_bytes_with_fallback(archive_url, ARCHIVE_URL_FALLBACK)?;
                write_bytes_atomically(&archive_path, archive_bytes.as_slice())?;
            }

            write_text_atomically(&version_path, &text)?;

            Ok(ArchiveResolution {
                path: archive_path,
                source: if should_download {
                    CachedArtifactSource::Network
                } else {
                    CachedArtifactSource::Cache
                },
            })
        }
        Err(error) => {
            if archive_path.exists() {
                append_log_line(
                    "WARN",
                    format!(
                        "Failed to refresh AutoEQ package metadata; using cached archive instead: {error}"
                    ),
                );
                emit_preset_progress(
                    app,
                    preset_name,
                    preset_source,
                    AutoEqProgressPhase::CacheHit,
                    "GitHub unavailable. Using cached AutoEQ package archive.",
                    Some(AutoEqProgressSource::StaleCache),
                );
                return Ok(ArchiveResolution {
                    path: archive_path,
                    source: CachedArtifactSource::StaleCache,
                });
            }

            let message = format!("Failed to load AutoEQ package metadata from GitHub: {error}");
            emit_preset_progress(
                app,
                preset_name,
                preset_source,
                AutoEqProgressPhase::Error,
                message.as_str(),
                None,
            );
            Err(AppError::Message(message))
        }
    }
}

fn emit_index_progress<R: Runtime>(
    app: &AppHandle<R>,
    phase: AutoEqProgressPhase,
    message: &str,
    source: Option<AutoEqProgressSource>,
) {
    emit_progress(
        app,
        AutoEqProgressPayload {
            operation: AutoEqProgressOperation::Index,
            phase,
            message: message.to_string(),
            source,
            preset_name: None,
            preset_source: None,
        },
    );
}

fn emit_preset_progress<R: Runtime>(
    app: &AppHandle<R>,
    preset_name: &str,
    preset_source: &str,
    phase: AutoEqProgressPhase,
    message: &str,
    source: Option<AutoEqProgressSource>,
) {
    emit_progress(
        app,
        AutoEqProgressPayload {
            operation: AutoEqProgressOperation::Preset,
            phase,
            message: message.to_string(),
            source,
            preset_name: Some(preset_name.to_string()),
            preset_source: Some(preset_source.to_string()),
        },
    );
}

fn emit_progress<R: Runtime>(app: &AppHandle<R>, payload: AutoEqProgressPayload) {
    if let Err(error) = app.emit(AUTOEQ_PROGRESS_EVENT, payload.clone()) {
        append_log_line(
            "WARN",
            format!(
                "Failed to emit AutoEQ progress event '{}': {error}",
                payload.message
            ),
        );
    }
}

fn parse_index(text: &str) -> Result<Vec<AutoEqIndexEntry>, AppError> {
    Ok(serde_json::from_str(text)?)
}

fn parse_version_entry(text: &str) -> Result<AutoEqVersionEntry, AppError> {
    let entries = serde_json::from_str::<Vec<AutoEqVersionEntry>>(text)?;
    entries
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Message("AutoEQ package metadata was empty.".to_string()))
}

fn read_index_from_disk(path: &Path) -> Result<Vec<AutoEqIndexEntry>, AppError> {
    parse_index(fs::read_to_string(path)?.as_str())
}

fn read_version_entry(path: &Path) -> Result<AutoEqVersionEntry, AppError> {
    parse_version_entry(fs::read_to_string(path)?.as_str())
}

fn extracted_preset_cache_path(name: &str, source: &str, variant: &str) -> Result<PathBuf, AppError> {
    let cache_root = cache_root()?.join(variant);
    fs::create_dir_all(&cache_root)?;
    let cache_key = URL_SAFE_NO_PAD.encode(format!("{name}\n{source}"));
    Ok(cache_root.join(format!("{cache_key}.txt")))
}

fn index_cache_path() -> Result<PathBuf, AppError> {
    Ok(cache_root()?.join("index.json"))
}

fn version_cache_path() -> Result<PathBuf, AppError> {
    Ok(cache_root()?.join("version.json"))
}

fn archive_cache_path() -> Result<PathBuf, AppError> {
    Ok(cache_root()?.join("archive.tar.gz"))
}

fn cache_root() -> Result<PathBuf, AppError> {
    let root = dirs::config_dir()
        .ok_or(AppError::AppDataUnavailable)?
        .join(APP_FOLDER_NAME)
        .join("autoeq");
    fs::create_dir_all(&root)?;
    Ok(root)
}

fn is_cache_recent(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    let Ok(modified_at) = metadata.modified() else {
        return false;
    };
    let Ok(age) = modified_at.elapsed() else {
        return false;
    };

    age < CACHE_TTL
}

fn can_use_extracted_cache(extracted_path: &Path, archive_path: &Path) -> bool {
    if !extracted_path.exists() || !archive_path.exists() {
        return false;
    }

    let Ok(extracted_modified) = fs::metadata(extracted_path).and_then(|value| value.modified())
    else {
        return true;
    };
    let Ok(archive_modified) = fs::metadata(archive_path).and_then(|value| value.modified()) else {
        return true;
    };

    extracted_modified >= archive_modified
}

fn download_text(url: &str) -> Result<String, AppError> {
    let bytes = download_bytes(url)?;
    String::from_utf8(bytes).map_err(|error| {
        AppError::Message(format!(
            "GitHub returned non-UTF8 text for AutoEQ metadata: {error}"
        ))
    })
}

fn download_bytes_with_fallback(url: &str, fallback_url: &str) -> Result<Vec<u8>, AppError> {
    match download_bytes(url) {
        Ok(bytes) => Ok(bytes),
        Err(primary_error) if url != fallback_url => {
            append_log_line(
                "WARN",
                format!(
                    "AutoEQ download failed from primary URL; retrying fallback. Primary error: {primary_error}"
                ),
            );
            download_bytes(fallback_url)
        }
        Err(error) => Err(error),
    }
}

fn download_bytes(url: &str) -> Result<Vec<u8>, AppError> {
    let response = http_client()?.get(url).send()?;
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Message(format!(
            "GitHub responded with HTTP {status} for {url}."
        )));
    }
    Ok(response.bytes()?.to_vec())
}

fn http_client() -> Result<Client, AppError> {
    Ok(Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent("SmartEQPresetSwitcher/0.2.0")
        .default_headers({
            let mut headers = header::HeaderMap::new();
            headers.insert(header::ACCEPT_ENCODING, header::HeaderValue::from_static("identity"));
            headers.insert(header::ACCEPT, header::HeaderValue::from_static("application/json, text/plain, application/octet-stream, */*"));
            headers
        })
        .build()?)
}

fn write_text_atomically(path: &Path, content: &str) -> Result<(), AppError> {
    write_bytes_atomically(path, content.as_bytes())
}

fn write_bytes_atomically(path: &Path, content: &[u8]) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("cache")
    ));

    fs::write(&temp_path, content)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&temp_path, path)?;
    Ok(())
}

fn normalize_archive_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}
