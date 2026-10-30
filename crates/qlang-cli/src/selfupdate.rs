//! `qlang self-update`: replace this program by a release of qlang, and the `qlang = "..."`
//! requirement of a project's `qlang.toml`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const DEFAULT_RELEASES: &str = "https://github.com/quenti77/qlang/releases";
const DEFAULT_API: &str = "https://api.github.com/repos/quenti77/qlang/releases/latest";

/// The build of this program in the releases (`qlang-<target>.tar.gz`, `.zip` on Windows).
fn target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-musl"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-musl"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        _ => None,
    }
}

/// `0.63.1` or `v0.63`: the numbers (missing ones are 0).
pub fn parse_version(text: &str) -> Option<[u64; 3]> {
    let mut out = [0u64; 3];
    let parts: Vec<&str> = text.trim().trim_start_matches('v').split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    for (i, p) in parts.iter().enumerate() {
        out[i] = p.parse().ok()?;
    }
    Some(out)
}

/// Whether `current` satisfies the `qlang = "..."` of a manifest: a minimum (`"0.64"` means
/// 0.64.0 or newer), `">=0.64.1"` the same, `"=0.64.2"` exactly that version.
pub fn requirement_met(requirement: &str, current: &str) -> Result<bool, String> {
    let req = requirement.trim();
    let bad =
        || format!("`qlang = {requirement:?}` is not a version (try \"0.64\" or \">=0.64.1\")");
    let (exact, number) = match req.strip_prefix('=') {
        Some(rest) => (true, rest),
        None => (false, req.strip_prefix(">=").unwrap_or(req)),
    };
    let wanted = parse_version(number.trim()).ok_or_else(bad)?;
    let have = parse_version(current).ok_or_else(bad)?;
    Ok(if exact {
        have == wanted
    } else {
        have >= wanted
    })
}

/// The error to show when the project asks for a newer qlang than the one running.
pub fn check_project_requirement(manifest: &toml::Table) -> Result<(), String> {
    let Some(req) = manifest
        .get("package")
        .and_then(|p| p.get("qlang"))
        .and_then(|v| v.as_str())
    else {
        return Ok(());
    };
    let current = env!("CARGO_PKG_VERSION");
    if requirement_met(req, current)? {
        return Ok(());
    }
    Err(format!(
        "this project needs qlang {req} (qlang.toml), this is qlang {current}: run `qlang self-update` \
         (or `qlang self-update --version X.Y.Z`)"
    ))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .into()
}

fn get(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let resp = agent()
        .get(url)
        .header("User-Agent", "qlang-self-update")
        .call()
        .map_err(|e| format!("cannot download {url}: {e}"))?;
    let mut out = Vec::new();
    resp.into_body()
        .into_reader()
        .take(limit)
        .read_to_end(&mut out)
        .map_err(|e| format!("cannot download {url}: {e}"))?;
    Ok(out)
}

fn sha256_hex(data: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, data)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The newest version published (its tag without the `v`).
fn latest_version() -> Result<String, String> {
    let api = std::env::var("QLANG_API_URL").unwrap_or_else(|_| DEFAULT_API.to_string());
    let body = get(&api, 1 << 20)?;
    let json: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("unexpected answer from {api}: {e}"))?;
    json.get("tag_name")
        .and_then(|t| t.as_str())
        .map(|t| t.trim_start_matches('v').to_string())
        .ok_or_else(|| format!("{api} does not give a tag_name"))
}

/// The program inside a release archive.
fn extract(archive: &[u8], name: &str) -> Result<Vec<u8>, String> {
    let exe = if cfg!(windows) { "qlang.exe" } else { "qlang" };
    let mut out = Vec::new();
    if name.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive))
            .map_err(|e| format!("{name}: {e}"))?;
        for i in 0..zip.len() {
            let mut file = zip.by_index(i).map_err(|e| format!("{name}: {e}"))?;
            if file.name().ends_with(&format!("/{exe}")) || file.name() == exe {
                file.read_to_end(&mut out)
                    .map_err(|e| format!("{name}: {e}"))?;
                return Ok(out);
            }
        }
    } else {
        let gz = flate2::read::GzDecoder::new(archive);
        let mut tar = tar::Archive::new(gz);
        for entry in tar.entries().map_err(|e| format!("{name}: {e}"))? {
            let mut entry = entry.map_err(|e| format!("{name}: {e}"))?;
            let path = entry
                .path()
                .map_err(|e| format!("{name}: {e}"))?
                .into_owned();
            if path.file_name().is_some_and(|f| f == exe) {
                entry
                    .read_to_end(&mut out)
                    .map_err(|e| format!("{name}: {e}"))?;
                return Ok(out);
            }
        }
    }
    Err(format!("{name} does not contain {exe}"))
}

/// Puts `bytes` in place of the program at `exe`. The running program is renamed first (a
/// running `.exe` cannot be overwritten on Windows, but it can be renamed).
fn replace(exe: &Path, bytes: &[u8]) -> Result<(), String> {
    let new: PathBuf = exe.with_extension("new");
    let old: PathBuf = exe.with_extension("old");
    std::fs::write(&new, bytes).map_err(|e| format!("cannot write {}: {e}", new.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755));
    }
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).map_err(|e| {
        let _ = std::fs::remove_file(&new);
        format!(
            "cannot replace {}: {e} (is the directory writable?)",
            exe.display()
        )
    })?;
    if let Err(e) = std::fs::rename(&new, exe) {
        let _ = std::fs::rename(&old, exe);
        let _ = std::fs::remove_file(&new);
        return Err(format!("cannot replace {}: {e}", exe.display()));
    }
    let _ = std::fs::remove_file(&old);
    Ok(())
}

pub fn run(args: &[&str]) -> ExitCode {
    let mut version: Option<String> = None;
    let mut check = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match *a {
            "--check" => check = true,
            "--version" => match it.next() {
                Some(v) => version = Some(v.trim_start_matches('v').to_string()),
                None => return usage("--version needs a value"),
            },
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other => return usage(&format!("unknown option `{other}`")),
        }
    }
    match update(version, check) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("qlang self-update: {e}");
            ExitCode::from(1)
        }
    }
}

const USAGE: &str = "usage: qlang self-update [--version X.Y.Z] [--check]

  replace this qlang by the latest release (or by X.Y.Z, older versions too);
  the download is checked against the SHA256SUMS of the release first.
  --check   only say whether a newer version exists

  QLANG_BASE_URL overrides https://github.com/quenti77/qlang/releases
  QLANG_API_URL overrides the address that tells which release is the latest";

fn usage(message: &str) -> ExitCode {
    eprintln!("{message}\n{USAGE}");
    ExitCode::from(64)
}

fn update(version: Option<String>, check: bool) -> Result<(), String> {
    let current = env!("CARGO_PKG_VERSION");
    let target = target().ok_or_else(|| {
        "there is no prebuilt qlang for this system: `cargo install --git https://github.com/quenti77/qlang qlang-cli`"
            .to_string()
    })?;
    let wanted = match &version {
        Some(v) => v.clone(),
        None => latest_version()?,
    };
    if check {
        match (parse_version(&wanted), parse_version(current)) {
            (Some(w), Some(c)) if w > c => {
                println!("qlang {wanted} is available (this is {current})")
            }
            _ => println!("qlang {current} is up to date"),
        }
        return Ok(());
    }
    if wanted == current {
        println!("qlang {current} is already installed");
        return Ok(());
    }
    let base = std::env::var("QLANG_BASE_URL").unwrap_or_else(|_| DEFAULT_RELEASES.to_string());
    let url = format!("{base}/download/v{wanted}");
    let name = if cfg!(windows) {
        format!("qlang-{target}.zip")
    } else {
        format!("qlang-{target}.tar.gz")
    };
    println!("Downloading qlang {wanted} for {target}...");
    let sums = String::from_utf8(get(&format!("{url}/SHA256SUMS"), 1 << 20)?)
        .map_err(|_| "SHA256SUMS is not text".to_string())?;
    let expected = sums
        .lines()
        .find_map(|l| {
            let (sum, file) = l.split_once("  ")?;
            (file.trim() == name).then(|| sum.to_string())
        })
        .ok_or_else(|| format!("{name} is not listed in the SHA256SUMS of qlang {wanted}"))?;
    let archive = get(&format!("{url}/{name}"), 256 << 20)?;
    let actual = sha256_hex(&archive);
    if actual != expected {
        return Err(format!(
            "checksum mismatch for {name} (expected {expected}, got {actual})"
        ));
    }
    let program = extract(&archive, &name)?;
    let exe = std::env::current_exe().map_err(|e| format!("cannot find this program: {e}"))?;
    replace(&exe, &program)?;
    println!(
        "Updated: qlang {current} -> qlang {wanted} ({})",
        exe.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_compared_by_numbers() {
        assert_eq!(parse_version("v0.64"), Some([0, 64, 0]));
        assert!(parse_version("0.x").is_none());
        assert!(requirement_met("0.64", "0.64.0").unwrap());
        assert!(requirement_met(">=0.63.1", "0.64.0").unwrap());
        assert!(!requirement_met("0.64", "0.63.9").unwrap());
        assert!(requirement_met("=0.64.2", "0.64.2").unwrap());
        assert!(!requirement_met("=0.64.2", "0.64.3").unwrap());
        assert!(requirement_met("0.100", "0.99.0").is_ok_and(|ok| !ok));
        assert!(requirement_met("soon", "0.64.0").is_err());
    }
}
