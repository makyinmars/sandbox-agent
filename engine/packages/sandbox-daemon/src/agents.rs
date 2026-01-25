use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentId {
    Claude,
    Codex,
    Opencode,
    Amp,
}

impl AgentId {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentId::Claude => "claude",
            AgentId::Codex => "codex",
            AgentId::Opencode => "opencode",
            AgentId::Amp => "amp",
        }
    }

    pub fn binary_name(self) -> &'static str {
        match self {
            AgentId::Claude => "claude",
            AgentId::Codex => "codex",
            AgentId::Opencode => "opencode",
            AgentId::Amp => "amp",
        }
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    LinuxX64,
    LinuxX64Musl,
    LinuxArm64,
    MacosArm64,
    MacosX64,
}

impl Platform {
    pub fn detect() -> Result<Self, AgentError> {
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        let is_musl = cfg!(target_env = "musl");

        match (os, arch, is_musl) {
            ("linux", "x86_64", true) => Ok(Self::LinuxX64Musl),
            ("linux", "x86_64", false) => Ok(Self::LinuxX64),
            ("linux", "aarch64", _) => Ok(Self::LinuxArm64),
            ("macos", "aarch64", _) => Ok(Self::MacosArm64),
            ("macos", "x86_64", _) => Ok(Self::MacosX64),
            _ => Err(AgentError::UnsupportedPlatform {
                os: os.to_string(),
                arch: arch.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentManager {
    install_dir: PathBuf,
    platform: Platform,
}

impl AgentManager {
    pub fn new(install_dir: impl Into<PathBuf>) -> Result<Self, AgentError> {
        Ok(Self {
            install_dir: install_dir.into(),
            platform: Platform::detect()?,
        })
    }

    pub fn with_platform(
        install_dir: impl Into<PathBuf>,
        platform: Platform,
    ) -> Self {
        Self {
            install_dir: install_dir.into(),
            platform,
        }
    }

    pub fn install(&self, agent: AgentId, options: InstallOptions) -> Result<InstallResult, AgentError> {
        let install_path = self.binary_path(agent);
        if install_path.exists() && !options.reinstall {
            return Ok(InstallResult {
                path: install_path,
                version: self.version(agent).unwrap_or(None),
            });
        }

        fs::create_dir_all(&self.install_dir)?;

        match agent {
            AgentId::Claude => install_claude(&install_path, self.platform, options.version.as_deref())?,
            AgentId::Codex => install_codex(&install_path, self.platform, options.version.as_deref())?,
            AgentId::Opencode => install_opencode(&install_path, self.platform, options.version.as_deref())?,
            AgentId::Amp => install_amp(&install_path, self.platform, options.version.as_deref())?,
        }

        Ok(InstallResult {
            path: install_path,
            version: self.version(agent).unwrap_or(None),
        })
    }

    pub fn is_installed(&self, agent: AgentId) -> bool {
        self.binary_path(agent).exists() || find_in_path(agent.binary_name()).is_some()
    }

    pub fn binary_path(&self, agent: AgentId) -> PathBuf {
        self.install_dir.join(agent.binary_name())
    }

    pub fn version(&self, agent: AgentId) -> Result<Option<String>, AgentError> {
        let path = self.resolve_binary(agent)?;
        let attempts = [vec!["--version"], vec!["version"], vec!["-V"]];
        for args in attempts {
            let output = Command::new(&path).args(args).output();
            if let Ok(output) = output {
                if output.status.success() {
                    if let Some(version) = parse_version_output(&output) {
                        return Ok(Some(version));
                    }
                }
            }
        }
        Ok(None)
    }

    pub fn spawn(&self, agent: AgentId, options: SpawnOptions) -> Result<SpawnResult, AgentError> {
        let path = self.resolve_binary(agent)?;
        let working_dir = options
            .working_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let mut command = Command::new(&path);
        command.current_dir(&working_dir);

        match agent {
            AgentId::Claude => {
                command
                    .arg("--print")
                    .arg("--output-format")
                    .arg("stream-json")
                    .arg("--verbose")
                    .arg("--dangerously-skip-permissions");
                if let Some(model) = options.model.as_deref() {
                    command.arg("--model").arg(model);
                }
                if let Some(session_id) = options.session_id.as_deref() {
                    command.arg("--resume").arg(session_id);
                }
                if let Some(permission_mode) = options.permission_mode.as_deref() {
                    if permission_mode == "plan" {
                        command.arg("--permission-mode").arg("plan");
                    }
                }
                command.arg(&options.prompt);
            }
            AgentId::Codex => {
                command
                    .arg("exec")
                    .arg("--json")
                    .arg("--dangerously-bypass-approvals-and-sandbox");
                if let Some(model) = options.model.as_deref() {
                    command.arg("-m").arg(model);
                }
                command.arg(&options.prompt);
            }
            AgentId::Opencode => {
                command
                    .arg("run")
                    .arg("--format")
                    .arg("json");
                if let Some(model) = options.model.as_deref() {
                    command.arg("-m").arg(model);
                }
                if let Some(agent_mode) = options.agent_mode.as_deref() {
                    command.arg("--agent").arg(agent_mode);
                }
                if let Some(variant) = options.variant.as_deref() {
                    command.arg("--variant").arg(variant);
                }
                if let Some(session_id) = options.session_id.as_deref() {
                    command.arg("-s").arg(session_id);
                }
                command.arg(&options.prompt);
            }
            AgentId::Amp => {
                let output = spawn_amp(&path, &working_dir, &options)?;
                return Ok(SpawnResult {
                    status: output.status,
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                });
            }
        }

        for (key, value) in options.env {
            command.env(key, value);
        }

        let output = command.output().map_err(AgentError::Io)?;
        Ok(SpawnResult {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    fn resolve_binary(&self, agent: AgentId) -> Result<PathBuf, AgentError> {
        let path = self.binary_path(agent);
        if path.exists() {
            return Ok(path);
        }
        if let Some(path) = find_in_path(agent.binary_name()) {
            return Ok(path);
        }
        Err(AgentError::BinaryNotFound { agent })
    }
}

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub reinstall: bool,
    pub version: Option<String>,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            reinstall: false,
            version: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallResult {
    pub path: PathBuf,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SpawnOptions {
    pub prompt: String,
    pub model: Option<String>,
    pub variant: Option<String>,
    pub agent_mode: Option<String>,
    pub permission_mode: Option<String>,
    pub session_id: Option<String>,
    pub working_dir: Option<PathBuf>,
    pub env: HashMap<String, String>,
}

impl SpawnOptions {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            model: None,
            variant: None,
            agent_mode: None,
            permission_mode: None,
            session_id: None,
            working_dir: None,
            env: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpawnResult {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("unsupported platform {os}/{arch}")]
    UnsupportedPlatform { os: String, arch: String },
    #[error("unsupported agent {agent}")]
    UnsupportedAgent { agent: String },
    #[error("binary not found for {agent}")]
    BinaryNotFound { agent: AgentId },
    #[error("download failed: {url}")]
    DownloadFailed { url: Url },
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("url parse error: {0}")]
    UrlParse(#[from] url::ParseError),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("extract failed: {0}")]
    ExtractFailed(String),
}

fn parse_version_output(output: &std::process::Output) -> Option<String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);
    combined
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.to_string())
}

fn spawn_amp(
    path: &Path,
    working_dir: &Path,
    options: &SpawnOptions,
) -> Result<std::process::Output, AgentError> {
    let flags = detect_amp_flags(path, working_dir).unwrap_or_default();
    let mut args: Vec<&str> = Vec::new();
    if flags.execute {
        args.push("--execute");
    } else if flags.print {
        args.push("--print");
    }
    if flags.output_format {
        args.push("--output-format");
        args.push("stream-json");
    }
    if flags.dangerously_skip_permissions {
        args.push("--dangerously-skip-permissions");
    }

    let mut command = Command::new(path);
    command.current_dir(working_dir);
    if let Some(model) = options.model.as_deref() {
        command.arg("--model").arg(model);
    }
    if let Some(session_id) = options.session_id.as_deref() {
        command.arg("--continue").arg(session_id);
    }
    command.args(&args).arg(&options.prompt);
    for (key, value) in &options.env {
        command.env(key, value);
    }
    let output = command.output().map_err(AgentError::Io)?;
    if output.status.success() {
        return Ok(output);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("unknown option")
        || stderr.contains("unknown flag")
        || stderr.contains("User message must be provided")
    {
        return spawn_amp_fallback(path, working_dir, options);
    }

    Ok(output)
}

#[derive(Debug, Default, Clone, Copy)]
struct AmpFlags {
    execute: bool,
    print: bool,
    output_format: bool,
    dangerously_skip_permissions: bool,
}

fn detect_amp_flags(path: &Path, working_dir: &Path) -> Option<AmpFlags> {
    let output = Command::new(path)
        .current_dir(working_dir)
        .arg("--help")
        .output()
        .ok()?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Some(AmpFlags {
        execute: text.contains("--execute"),
        print: text.contains("--print"),
        output_format: text.contains("--output-format"),
        dangerously_skip_permissions: text.contains("--dangerously-skip-permissions"),
    })
}

fn spawn_amp_fallback(
    path: &Path,
    working_dir: &Path,
    options: &SpawnOptions,
) -> Result<std::process::Output, AgentError> {
    let attempts = vec![
        vec!["--execute"],
        vec!["--print", "--output-format", "stream-json"],
        vec!["--output-format", "stream-json"],
        vec!["--dangerously-skip-permissions"],
        vec![],
    ];

    for args in attempts {
        let mut command = Command::new(path);
        command.current_dir(working_dir);
        if let Some(model) = options.model.as_deref() {
            command.arg("--model").arg(model);
        }
        if let Some(session_id) = options.session_id.as_deref() {
            command.arg("--continue").arg(session_id);
        }
        if !args.is_empty() {
            command.args(&args);
        }
        command.arg(&options.prompt);
        for (key, value) in &options.env {
            command.env(key, value);
        }
        let output = command.output().map_err(AgentError::Io)?;
        if output.status.success() {
            return Ok(output);
        }
    }

    let mut command = Command::new(path);
    command.current_dir(working_dir);
    if let Some(model) = options.model.as_deref() {
        command.arg("--model").arg(model);
    }
    if let Some(session_id) = options.session_id.as_deref() {
        command.arg("--continue").arg(session_id);
    }
    command.arg(&options.prompt);
    for (key, value) in &options.env {
        command.env(key, value);
    }
    Ok(command.output().map_err(AgentError::Io)?)
}

fn find_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for path in std::env::split_paths(&path_var) {
        let candidate = path.join(binary_name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn download_bytes(url: &Url) -> Result<Vec<u8>, AgentError> {
    let client = Client::builder().build()?;
    let mut response = client.get(url.clone()).send()?;
    if !response.status().is_success() {
        return Err(AgentError::DownloadFailed { url: url.clone() });
    }
    let mut bytes = Vec::new();
    response.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn install_claude(path: &Path, platform: Platform, version: Option<&str>) -> Result<(), AgentError> {
    let version = match version {
        Some(version) => version.to_string(),
        None => {
            let url = Url::parse(
                "https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/claude-code-releases/latest",
            )?;
            let text = String::from_utf8(download_bytes(&url)?).map_err(|err| AgentError::ExtractFailed(err.to_string()))?;
            text.trim().to_string()
        }
    };

    let platform_segment = match platform {
        Platform::LinuxX64 => "linux-x64",
        Platform::LinuxX64Musl => "linux-x64-musl",
        Platform::LinuxArm64 => "linux-arm64",
        Platform::MacosArm64 => "darwin-arm64",
        Platform::MacosX64 => "darwin-x64",
    };

    let url = Url::parse(&format!(
        "https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/claude-code-releases/{version}/{platform_segment}/claude"
    ))?;
    let bytes = download_bytes(&url)?;
    write_executable(path, &bytes)?;
    Ok(())
}

fn install_amp(path: &Path, platform: Platform, version: Option<&str>) -> Result<(), AgentError> {
    let version = match version {
        Some(version) => version.to_string(),
        None => {
            let url = Url::parse("https://storage.googleapis.com/amp-public-assets-prod-0/cli/cli-version.txt")?;
            let text = String::from_utf8(download_bytes(&url)?).map_err(|err| AgentError::ExtractFailed(err.to_string()))?;
            text.trim().to_string()
        }
    };

    let platform_segment = match platform {
        Platform::LinuxX64 | Platform::LinuxX64Musl => "linux-x64",
        Platform::LinuxArm64 => "linux-arm64",
        Platform::MacosArm64 => "darwin-arm64",
        Platform::MacosX64 => "darwin-x64",
    };

    let url = Url::parse(&format!(
        "https://storage.googleapis.com/amp-public-assets-prod-0/cli/{version}/amp-{platform_segment}"
    ))?;
    let bytes = download_bytes(&url)?;
    write_executable(path, &bytes)?;
    Ok(())
}

fn install_codex(path: &Path, platform: Platform, version: Option<&str>) -> Result<(), AgentError> {
    let target = match platform {
        Platform::LinuxX64 | Platform::LinuxX64Musl => "x86_64-unknown-linux-musl",
        Platform::LinuxArm64 => "aarch64-unknown-linux-musl",
        Platform::MacosArm64 => "aarch64-apple-darwin",
        Platform::MacosX64 => "x86_64-apple-darwin",
    };

    let url = match version {
        Some(version) => Url::parse(&format!(
            "https://github.com/openai/codex/releases/download/{version}/codex-{target}.tar.gz"
        ))?,
        None => Url::parse(&format!(
            "https://github.com/openai/codex/releases/latest/download/codex-{target}.tar.gz"
        ))?,
    };

    let bytes = download_bytes(&url)?;
    let temp_dir = tempfile::tempdir()?;
    let cursor = io::Cursor::new(bytes);
    let mut archive = tar::Archive::new(GzDecoder::new(cursor));
    archive.unpack(temp_dir.path())?;

    let expected = format!("codex-{target}");
    let binary = find_file_recursive(temp_dir.path(), &expected)?
        .ok_or_else(|| AgentError::ExtractFailed(format!("missing {expected}")))?;
    move_executable(&binary, path)?;
    Ok(())
}

fn install_opencode(path: &Path, platform: Platform, version: Option<&str>) -> Result<(), AgentError> {
    match platform {
        Platform::MacosArm64 => {
            let url = match version {
                Some(version) => Url::parse(&format!(
                    "https://github.com/anomalyco/opencode/releases/download/{version}/opencode-darwin-arm64.zip"
                ))?,
                None => Url::parse(
                    "https://github.com/anomalyco/opencode/releases/latest/download/opencode-darwin-arm64.zip",
                )?,
            };
            install_zip_binary(path, &url, "opencode")
        }
        Platform::MacosX64 => {
            let url = match version {
                Some(version) => Url::parse(&format!(
                    "https://github.com/anomalyco/opencode/releases/download/{version}/opencode-darwin-x64.zip"
                ))?,
                None => Url::parse(
                    "https://github.com/anomalyco/opencode/releases/latest/download/opencode-darwin-x64.zip",
                )?,
            };
            install_zip_binary(path, &url, "opencode")
        }
        _ => {
            let platform_segment = match platform {
                Platform::LinuxX64 => "linux-x64",
                Platform::LinuxX64Musl => "linux-x64-musl",
                Platform::LinuxArm64 => "linux-arm64",
                Platform::MacosArm64 | Platform::MacosX64 => unreachable!(),
            };
            let url = match version {
                Some(version) => Url::parse(&format!(
                    "https://github.com/anomalyco/opencode/releases/download/{version}/opencode-{platform_segment}.tar.gz"
                ))?,
                None => Url::parse(&format!(
                    "https://github.com/anomalyco/opencode/releases/latest/download/opencode-{platform_segment}.tar.gz"
                ))?,
            };

            let bytes = download_bytes(&url)?;
            let temp_dir = tempfile::tempdir()?;
            let cursor = io::Cursor::new(bytes);
            let mut archive = tar::Archive::new(GzDecoder::new(cursor));
            archive.unpack(temp_dir.path())?;
            let binary = find_file_recursive(temp_dir.path(), "opencode")?
                .ok_or_else(|| AgentError::ExtractFailed("missing opencode".to_string()))?;
            move_executable(&binary, path)?;
            Ok(())
        }
    }
}

fn install_zip_binary(path: &Path, url: &Url, binary_name: &str) -> Result<(), AgentError> {
    let bytes = download_bytes(url)?;
    let reader = io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(|err| AgentError::ExtractFailed(err.to_string()))?;
    let temp_dir = tempfile::tempdir()?;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|err| AgentError::ExtractFailed(err.to_string()))?;
        if !file.name().ends_with(binary_name) {
            continue;
        }
        let out_path = temp_dir.path().join(binary_name);
        let mut out_file = fs::File::create(&out_path)?;
        io::copy(&mut file, &mut out_file)?;
        move_executable(&out_path, path)?;
        return Ok(());
    }
    Err(AgentError::ExtractFailed(format!("missing {binary_name}")))
}

fn write_executable(path: &Path, bytes: &[u8]) -> Result<(), AgentError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    set_executable(path)?;
    Ok(())
}

fn move_executable(source: &Path, dest: &Path) -> Result<(), AgentError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if dest.exists() {
        fs::remove_file(dest)?;
    }
    fs::copy(source, dest)?;
    set_executable(dest)?;
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), AgentError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), AgentError> {
    Ok(())
}

fn find_file_recursive(dir: &Path, filename: &str) -> Result<Option<PathBuf>, AgentError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_recursive(&path, filename)? {
                return Ok(Some(found));
            }
        } else if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            if name == filename {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}
