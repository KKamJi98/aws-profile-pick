//! Shell detection and the rc-file update.
//!
//! This is the part of awspick that writes to the user's files, so it keeps
//! the 2.x behaviour exactly: the same files, the same line format, the same
//! regular expressions (including their anchoring at column 0, which is what
//! leaves an indented `export AWS_PROFILE="$(cat ...)"` inside a sync hook
//! alone), the same backup naming and rotation, and an in-place rewrite that
//! keeps a symlinked rc file a symlink.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::{NoExpand, Regex};

use crate::out;
use crate::py;

pub const BACKUP_RETENTION_COUNT: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bash,
    Zsh,
    Fish,
}

#[derive(Clone, Debug)]
pub struct ShellConfig {
    pub kind: Kind,
    pub rc_path: PathBuf,
}

impl ShellConfig {
    pub fn name(&self) -> &'static str {
        match self.kind {
            Kind::Bash => "bash",
            Kind::Zsh => "zsh",
            Kind::Fish => "fish",
        }
    }

    pub fn profile_line(&self, profile: &str) -> String {
        match self.kind {
            Kind::Fish => format!("set -gx AWS_PROFILE \"{profile}\""),
            _ => format!("export AWS_PROFILE=\"{profile}\""),
        }
    }

    /// The 2.x patterns, with `\s` widened to Python's whitespace set.
    pub fn profile_pattern(&self) -> Regex {
        let pattern = match self.kind {
            // `set` with any mix of -g/-x/-gx/-xg/--global/--export before the
            // name. 2.x accepted only a single -g or -x, which never matched
            // the `set -gx` line it wrote itself.
            Kind::Fish => {
                r#"(?m)^set(?:[ \t]+(?:-[gx]{1,2}|--global|--export))+[ \t]+AWS_PROFILE[ \t]+["']?(.+?)["']?[ \t]*$"#
            }
            _ => r"(?m)^export[\s\x1c-\x1f]+AWS_PROFILE=(.+)$",
        };
        Regex::new(pattern).expect("static pattern")
    }
}

/// `detect_shell`: `$SHELL`'s basename, else the parent process name.
pub fn detect_shell() -> String {
    if let Some(shell) = std::env::var_os("SHELL").filter(|s| !s.is_empty()) {
        let shell = shell.to_string_lossy().into_owned();
        let name = py::basename(&shell).to_owned();
        out::info(&format!("Detected shell from SHELL env var: {name}"));
        return name;
    }
    let ppid = std::os::unix::process::parent_id().to_string();
    let result = Command::new("ps")
        .args(["-p", &ppid, "-o", "comm="])
        .output();
    match result {
        Ok(o) if o.status.success() => match String::from_utf8(o.stdout) {
            Ok(s) => {
                let mut name = py::strip(&s).to_owned();
                if name.contains('/') {
                    name = py::basename(&name).to_owned();
                }
                out::info(&format!("Detected shell from parent process: {name}"));
                name
            }
            Err(_) => {
                out::warning("Failed to detect shell: 'utf-8' codec can't decode ps output");
                "bash".to_owned()
            }
        },
        Ok(o) => {
            let code = o
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "-1".to_owned());
            out::warning(&format!(
                "Failed to detect shell: Command '['ps', '-p', '{ppid}', '-o', 'comm=']' returned non-zero exit status {code}."
            ));
            "bash".to_owned()
        }
        Err(e) => {
            out::warning(&format!(
                "Failed to detect shell: {}",
                py::os_error(&e, Path::new("ps"))
            ));
            "bash".to_owned()
        }
    }
}

pub fn shell_configs(home: &Path) -> [ShellConfig; 3] {
    [
        ShellConfig {
            kind: Kind::Bash,
            rc_path: home.join(".bashrc"),
        },
        ShellConfig {
            kind: Kind::Zsh,
            rc_path: home.join(".zshrc"),
        },
        ShellConfig {
            kind: Kind::Fish,
            rc_path: home.join(".config").join("fish").join("config.fish"),
        },
    ]
}

/// `get_rc_path`. `None` detects the shell first, as the Python default did.
pub fn get_rc_path(home: &Path, shell_name: Option<&str>) -> ShellConfig {
    let detected;
    let shell_name = match shell_name {
        Some(s) => s,
        None => {
            detected = detect_shell();
            &detected
        }
    };
    let normalized = py::lower(shell_name);
    let configs = shell_configs(home);
    for c in &configs {
        if normalized.starts_with(c.name()) {
            out::info(&format!(
                "Using {} configuration at {}",
                c.name(),
                c.rc_path.display()
            ));
            return c.clone();
        }
    }
    out::warning(&format!(
        "Shell '{shell_name}' not recognized, falling back to bash"
    ));
    configs[0].clone()
}

pub fn shared_profile_path(home: &Path) -> PathBuf {
    home.join(".config").join("awspick").join("profile")
}

/// Write the profile to `~/.config/awspick/profile` through a temp file.
pub fn write_shared_profile(home: &Path, profile: &str) -> Result<PathBuf, String> {
    let shared = shared_profile_path(home);
    let attempt = || -> io::Result<()> {
        if let Some(parent) = shared.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = shared.with_extension("tmp");
        {
            let mut f = File::create(&tmp)?;
            f.write_all(format!("{profile}\n").as_bytes())?;
        }
        fs::rename(&tmp, &shared)?;
        Ok(())
    };
    match attempt() {
        Ok(()) => {
            out::info(&format!("Wrote shared profile to {}", shared.display()));
            Ok(shared)
        }
        Err(e) => {
            let msg = py::os_error(&e, &shared);
            out::error(&format!("Failed to write shared profile file: {msg}"));
            Err(msg)
        }
    }
}

/// Local time as `YYYYmmddHHMMSS`, the 2.x backup suffix.
fn timestamp() -> String {
    chrono::Local::now().format("%Y%m%d%H%M%S").to_string()
}

/// `shutil.copy2`: follow symlinks, copy the bytes, then the permission bits
/// and the access and modification times.
fn copy2(src: &Path, dst: &Path) -> io::Result<()> {
    let data = fs::read(src)?;
    let meta = fs::metadata(src)?;
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(dst)?;
        f.write_all(&data)?;
    }
    fs::set_permissions(dst, meta.permissions())?;
    let times = fs::FileTimes::new()
        .set_accessed(meta.accessed()?)
        .set_modified(meta.modified()?);
    File::options().write(true).open(dst)?.set_times(times)?;
    Ok(())
}

/// Back up the rc file and keep the newest `BACKUP_RETENTION_COUNT`.
pub fn backup_rc_file(rc_path: &Path) -> Result<PathBuf, String> {
    let backup = PathBuf::from(format!("{}.bak-{}", rc_path.display(), timestamp()));
    if let Err(e) = copy2(rc_path, &backup) {
        let msg = py::os_error(&e, rc_path);
        out::error(&format!("Failed to create backup: {msg}"));
        return Err(msg);
    }
    out::info(&format!("Backup created at {}", backup.display()));

    let parent = rc_path.parent().unwrap_or(Path::new("."));
    let prefix = format!(
        "{}.bak-",
        rc_path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default()
    );
    let mut backups: Vec<PathBuf> = match fs::read_dir(parent) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .map(|e| e.path())
            .collect(),
        Err(_) => Vec::new(),
    };
    backups.sort();
    if backups.len() > BACKUP_RETENTION_COUNT {
        let remove = backups.len() - BACKUP_RETENTION_COUNT;
        for old in &backups[..remove] {
            match fs::remove_file(old) {
                Ok(()) => out::info(&format!("Removed old backup {}", old.display())),
                Err(e) => out::warning(&format!(
                    "Failed to remove old backup {}: {}",
                    old.display(),
                    py::os_error(&e, old)
                )),
            }
        }
    }
    Ok(backup)
}

/// Read a text file the way Python's `open(path, "r").read()` did.
fn read_text(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| py::os_error(&e, path))?;
    let text = py::decode_utf8(&bytes)?;
    Ok(py::universal_newlines(&text))
}

/// `get_current_profile`: `$AWS_PROFILE`, else the value in the rc file.
pub fn get_current_profile(home: &Path) -> Option<String> {
    if let Some(env) = std::env::var_os("AWS_PROFILE") {
        let env = env.to_string_lossy().into_owned();
        let env = py::strip(&env);
        if !env.is_empty() {
            out::info("Using AWS_PROFILE from environment");
            return Some(env.to_owned());
        }
    }
    let config = get_rc_path(home, None);
    if !config.rc_path.exists() {
        return None;
    }
    let content = match read_text(&config.rc_path) {
        Ok(c) => c,
        Err(msg) => {
            out::warning(&format!(
                "Failed to read current AWS_PROFILE from {}: {msg}",
                config.rc_path.display()
            ));
            return None;
        }
    };
    let caps = config.profile_pattern().captures(&content)?;
    let value = py::strip(&caps[1]).trim_matches(['"', '\'']);
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

pub enum Update {
    /// The rc file already had this profile; nothing was written.
    Unchanged,
    /// The rc file was rewritten; the backup that was made first.
    Written(PathBuf),
}

/// `update_aws_profile`.
/// Why an update failed. `Unexpected` errors escaped `update_aws_profile`
/// in 2.x and were reported by the top-level handler instead.
pub enum UpdateError {
    Failed,
    Unexpected,
}

pub fn update_aws_profile(
    home: &Path,
    profile: &str,
    shell_name: &str,
) -> Result<Update, UpdateError> {
    let config = get_rc_path(home, Some(shell_name));
    let rc = &config.rc_path;

    if !rc.exists() {
        if config.kind == Kind::Fish {
            let created = (|| -> io::Result<()> {
                if let Some(parent) = rc.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(rc, "# Created by AWS Pick\n\n")
            })();
            if let Err(e) = created {
                out::error(&format!(
                    "An unexpected error occurred: {}",
                    py::os_error(&e, rc)
                ));
                return Err(UpdateError::Unexpected);
            }
            out::info(&format!("Created new config file at {}", rc.display()));
        } else {
            out::error(&format!("Shell config file not found at {}", rc.display()));
            return Err(UpdateError::Failed);
        }
    }

    let fail = |msg: &str| {
        out::error(&format!("Failed to update AWS profile: {msg}"));
    };

    let content = match read_text(rc) {
        Ok(c) => c,
        Err(msg) => {
            fail(&msg);
            return Err(UpdateError::Failed);
        }
    };
    let pattern = config.profile_pattern();
    let current = pattern
        .captures(&content)
        .map(|c| c[1].trim_matches(['"', '\'']).to_owned());

    if current.as_deref() == Some(profile) {
        out::info(&format!(
            "AWS_PROFILE already set to {profile}, no changes needed"
        ));
        return Ok(Update::Unchanged);
    }

    let backup = match backup_rc_file(rc) {
        Ok(b) => b,
        Err(msg) => {
            fail(&msg);
            return Err(UpdateError::Failed);
        }
    };

    let line = config.profile_line(profile);
    let new_content = if let Some(cur) = &current {
        let replaced = pattern.replace_all(&content, NoExpand(&line)).into_owned();
        out::info(&format!(
            "Replacing existing AWS_PROFILE={cur} with {profile}"
        ));
        replaced
    } else {
        let appended = format!("{}\n\n# Added by AWS Pick\n{line}\n", py::rstrip(&content));
        out::info(&format!("Adding new AWS_PROFILE={profile} entry"));
        appended
    };

    // Rewrite in place, like open(path, "w"): a symlink stays a symlink and
    // the file keeps its mode.
    let written = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(rc)
        .and_then(|mut f| f.write_all(new_content.as_bytes()));
    if let Err(e) = written {
        fail(&py::os_error(&e, rc));
        return Err(UpdateError::Failed);
    }
    out::info(&format!(
        "Successfully updated {} with AWS_PROFILE={profile}",
        rc.display()
    ));
    Ok(Update::Written(backup))
}

pub fn generate_export_command(home: &Path, profile: &str, shell_name: &str) -> String {
    get_rc_path(home, Some(shell_name)).profile_line(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(kind: Kind) -> ShellConfig {
        ShellConfig {
            kind,
            rc_path: PathBuf::from("/h/rc"),
        }
    }

    #[test]
    fn indented_export_is_not_matched() {
        let content = "export AWS_PROFILE=\"company\"\nawspick_sync() {\n    export AWS_PROFILE=\"$(cat \"$f\")\"\n}\n";
        let p = cfg(Kind::Zsh).profile_pattern();
        let all: Vec<_> = p.captures_iter(content).map(|c| c[1].to_owned()).collect();
        assert_eq!(all, vec!["\"company\""]);
    }

    #[test]
    fn replacement_is_literal() {
        let p = cfg(Kind::Zsh).profile_pattern();
        let out = p.replace_all(
            "export AWS_PROFILE=\"c\"\n",
            NoExpand("export AWS_PROFILE=\"a$1\\1\""),
        );
        assert_eq!(out, "export AWS_PROFILE=\"a$1\\1\"\n");
    }

    #[test]
    fn fish_pattern_matches_the_line_it_writes() {
        let p = cfg(Kind::Fish).profile_pattern();
        assert!(p.is_match("set -gx AWS_PROFILE \"a\""));
        assert!(p.is_match("set --global --export AWS_PROFILE a"));
        assert!(p.is_match("set -x AWS_PROFILE 'a'"));
        assert!(!p.is_match("set -gx AWS_PROFILE_X a"));
        assert!(!p.is_match("  set -gx AWS_PROFILE a"));
        assert!(p.is_match("set -gx AWS_PROFILE \"prod\"\n"));
        assert!(p.is_match("x\n\n# Added by AWS Pick\nset -gx AWS_PROFILE \"prod\"\n"));
    }
}
