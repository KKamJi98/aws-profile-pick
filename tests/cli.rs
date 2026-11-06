//! Behaviour tests ported from the 2.x pytest suite (tests/test_cli.py,
//! test_config.py, test_shell.py). The Python tests mocked `open` and
//! `Path.exists`; these run the real binary in a throwaway HOME instead.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_awspick");

struct Home {
    _tmp: tempfile::TempDir,
    path: PathBuf,
}

impl Home {
    fn new(config: Option<&str>) -> Home {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().to_path_buf();
        if let Some(c) = config {
            fs::create_dir_all(path.join(".aws")).unwrap();
            fs::write(path.join(".aws/config"), c).unwrap();
        }
        Home { _tmp: tmp, path }
    }
    fn write(&self, rel: &str, content: &str) {
        let p = self.path.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }
    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.path.join(rel)).unwrap()
    }
    fn exists(&self, rel: &str) -> bool {
        self.path.join(rel).exists()
    }
    fn backups(&self, rc: &str) -> Vec<String> {
        let p = self.path.join(rc);
        let dir = p.parent().unwrap();
        let prefix = format!("{}.bak-", p.file_name().unwrap().to_string_lossy());
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(&prefix))
            .collect();
        v.sort();
        v
    }
    fn cmd(&self, shell: Option<&str>) -> Command {
        let mut c = Command::new(BIN);
        c.env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.path)
            .current_dir(&self.path);
        if let Some(s) = shell {
            c.env("SHELL", s);
        }
        c
    }
    fn run(&self, shell: &str, args: &[&str], stdin: &str) -> Output {
        run_with(self.cmd(Some(shell)).args(args), stdin)
    }
}

fn run_with(cmd: &mut Command, stdin: &str) -> Output {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

const CFG: &str = "[default]\nregion = us-east-1\n[profile dev]\n[profile prod]\n[sso-session x]\n";

// --- test_config.py -------------------------------------------------------

#[test]
fn read_aws_profiles_lists_default_and_named_profiles_sorted() {
    let h = Home::new(Some("[profile zeta]\n[default]\n[profile alpha]\n"));
    h.write(".zshrc", "");
    let o = h.run("/bin/zsh", &[], "q\n");
    let e = stderr(&o);
    assert!(e.contains("INFO: Found 3 AWS profiles"));
    let a = e.find("alpha").unwrap();
    let d = e.find("default").unwrap();
    let z = e.find("zeta").unwrap();
    assert!(a < d && d < z, "sorted alphabetically:\n{e}");
}

#[test]
fn read_aws_profiles_without_config_exits_1() {
    let h = Home::new(None);
    let o = h.run("/bin/zsh", &[], "1\n");
    assert_eq!(o.status.code(), Some(1));
    let e = stderr(&o);
    assert!(e.contains("ERROR: AWS config file not found at"));
    assert!(e.contains("ERROR: No AWS profiles found. Please check your AWS configuration."));
}

#[test]
fn display_marks_current_profile() {
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "");
    let o = run_with(h.cmd(Some("/bin/zsh")).env("AWS_PROFILE", "PROD"), "q\n");
    let line = stderr(&o)
        .lines()
        .find(|l| l.contains(" prod "))
        .unwrap()
        .to_owned();
    assert!(line.contains('*'), "{line}");
}

#[test]
fn display_empty_list_message() {
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "");
    let o = h.run("/bin/zsh", &["-f", "nothing-matches"], "q\n");
    assert!(stderr(&o).contains("No AWS profiles found in ~/.aws/config"));
}

#[test]
fn validate_selection_by_number_name_and_case() {
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "");
    // Display order: prod, dev, default (others).
    assert!(stdout(&h.run("/bin/zsh", &[], "1\n")).contains("\"prod\""));
    assert!(stdout(&h.run("/bin/zsh", &[], "dev\n")).contains("\"dev\""));
    let o = h.run("/bin/zsh", &[], "DEFAULT\n");
    assert!(stdout(&o).contains("\"default\""));
    assert!(stderr(&o).contains("INFO: Found case-insensitive match for 'DEFAULT': 'default'"));
}

#[test]
fn grouping_keeps_preprod_out_of_prod() {
    let h = Home::new(Some("[profile acme-preprod]\n[profile acme-prod]\n"));
    h.write(".zshrc", "");
    let e = stderr(&h.run("/bin/zsh", &[], "q\n"));
    let prod = e.lines().find(|l| l.contains("acme-prod ")).unwrap();
    let preprod = e.lines().find(|l| l.contains("acme-preprod")).unwrap();
    assert!(prod.contains("│ prod "), "{prod}");
    assert!(preprod.contains("│ preprod "), "{preprod}");
}

// --- test_cli.py ----------------------------------------------------------

#[test]
fn selection_invalid_then_valid() {
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "");
    let o = h.run("/bin/zsh", &[], "nope\n2\n");
    assert_eq!(o.status.code(), Some(0));
    let e = stderr(&o);
    assert!(e.contains("ERROR: Profile 'nope' not found in available profiles"));
    assert!(e.contains("Invalid selection. Please enter a valid profile number or name."));
    assert_eq!(stdout(&o), "export AWS_PROFILE=\"dev\"\n");
}

#[test]
fn selection_quit_and_empty_exit_1_without_writing() {
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "alias a=b\n");
    for input in ["q\n", "exit\n", "\n"] {
        let o = h.run("/bin/zsh", &[], input);
        assert_eq!(o.status.code(), Some(1), "{input:?}");
        assert!(stdout(&o).is_empty());
        assert!(stderr(&o).contains("INFO: No profile selected, exiting"));
    }
    assert_eq!(h.read(".zshrc"), "alias a=b\n");
    assert!(!h.exists(".config/awspick/profile"));
}

#[test]
fn keyboard_interrupt_during_prompt_cancels_with_exit_1() {
    use std::io::Read;
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "");
    let mut child = h
        .cmd(Some("/bin/zsh"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _stdin = child.stdin.take();
    // Wait for the prompt, so the signal lands while input is being read.
    let mut err = child.stderr.take().unwrap();
    let mut seen = Vec::new();
    let mut buf = [0u8; 4096];
    while !String::from_utf8_lossy(&seen).contains("Enter profile number or name: ") {
        let n = err.read(&mut buf).unwrap();
        assert!(n > 0, "stderr closed before the prompt");
        seen.extend_from_slice(&buf[..n]);
    }
    std::thread::sleep(Duration::from_millis(100));
    Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    err.read_to_end(&mut seen).unwrap();
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(1));
    let e = String::from_utf8_lossy(&seen);
    assert!(e.contains("\nOperation cancelled.\n"), "{e}");
    assert!(e.contains("INFO: Profile selection cancelled via keyboard interrupt"));
}

#[test]
fn main_successful_update_prints_export_and_writes_files() {
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "alias a=b\n");
    let o = h.run("/bin/zsh", &[], "dev\n");
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(stdout(&o), "export AWS_PROFILE=\"dev\"\n");
    let e = stderr(&o);
    assert!(e.contains("Selected profile: dev"));
    assert!(e.contains("Run 'eval \"$(awspick)\"' to apply in the current shell"));
    assert_eq!(
        h.read(".zshrc"),
        "alias a=b\n\n# Added by AWS Pick\nexport AWS_PROFILE=\"dev\"\n"
    );
    assert_eq!(h.read(".config/awspick/profile"), "dev\n");
}

#[test]
fn main_failed_update_exits_1() {
    let h = Home::new(Some(CFG));
    let o = h.run("/bin/zsh", &[], "dev\n");
    assert_eq!(o.status.code(), Some(1));
    let e = stderr(&o);
    assert!(e.contains("ERROR: Shell config file not found at"));
    assert!(e.contains("ERROR: Failed to update AWS profile."));
    assert!(stdout(&o).is_empty());
}

#[test]
fn stdout_closed_early_is_not_a_panic() {
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "");
    let mut child = h
        .cmd(Some("/bin/zsh"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    child.stdin.take().unwrap().write_all(b"dev\n").unwrap();
    let o = child.wait_with_output().unwrap();
    assert_ne!(o.status.code(), Some(101));
    assert!(!stderr(&o).contains("panicked"));
}

// --- test_shell.py --------------------------------------------------------

#[test]
fn detect_shell_from_env_and_parent_process() {
    let h = Home::new(Some(CFG));
    h.write(".bashrc", "");
    let e = stderr(&h.run("/usr/local/bin/bash", &[], "q\n"));
    assert!(e.contains("INFO: Detected shell from SHELL env var: bash"));
    let o = run_with(&mut h.cmd(None), "q\n");
    assert!(stderr(&o).contains("INFO: Detected shell from parent process: "));
}

#[test]
fn rc_paths_per_shell() {
    let h = Home::new(Some(CFG));
    for (shell, rc) in [
        ("/bin/bash", ".bashrc"),
        ("/bin/zsh", ".zshrc"),
        ("/usr/bin/fish", ".config/fish/config.fish"),
    ] {
        let e = stderr(&h.run(shell, &[], "q\n"));
        let name = Path::new(shell).file_name().unwrap().to_string_lossy();
        assert!(
            e.contains(&format!(
                "INFO: Using {name} configuration at {}",
                h.path.join(rc).display()
            )),
            "{e}"
        );
    }
}

#[test]
fn unknown_shell_falls_back_to_bash() {
    let h = Home::new(Some(CFG));
    h.write(".bashrc", "");
    let o = h.run("/usr/bin/nu", &[], "dev\n");
    assert!(stderr(&o).contains("WARNING: Shell 'nu' not recognized, falling back to bash"));
    assert_eq!(stdout(&o), "export AWS_PROFILE=\"dev\"\n");
}

#[test]
fn current_profile_from_rc_file() {
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "export AWS_PROFILE='dev'\n");
    let e = stderr(&h.run("/bin/zsh", &[], "q\n"));
    let line = e.lines().find(|l| l.contains(" dev ")).unwrap();
    assert!(line.contains('*'), "{line}");
}

#[test]
fn backup_keeps_mode_and_rotates_to_two() {
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "x\n");
    fs::set_permissions(h.path.join(".zshrc"), fs::Permissions::from_mode(0o600)).unwrap();
    h.write(".zshrc.bak-20240101000000", "old1\n");
    h.write(".zshrc.bak-20250101000000", "old2\n");
    let o = h.run("/bin/zsh", &[], "dev\n");
    assert_eq!(o.status.code(), Some(0));
    let b = h.backups(".zshrc");
    assert_eq!(b.len(), 2, "{b:?}");
    assert_eq!(b[0], ".zshrc.bak-20250101000000");
    let newest = h.path.join(&b[1]);
    assert_eq!(fs::read_to_string(&newest).unwrap(), "x\n");
    assert_eq!(
        fs::metadata(&newest).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(h.path.join(".zshrc"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn update_bash_replaces_existing_line() {
    let h = Home::new(Some(CFG));
    h.write(".bashrc", "# c\nexport AWS_PROFILE=\"prod\"\n");
    let o = h.run("/bin/bash", &[], "dev\n");
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(h.read(".bashrc"), "# c\nexport AWS_PROFILE=\"dev\"\n");
}

#[test]
fn update_fish_writes_fish_syntax() {
    let h = Home::new(Some(CFG));
    h.write(
        ".config/fish/config.fish",
        "# Some content\nset -gx AWS_PROFILE \"prod\"\n",
    );
    let o = h.run("/usr/bin/fish", &[], "dev\n");
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(stdout(&o), "set -gx AWS_PROFILE \"dev\"\n");
    assert!(
        h.read(".config/fish/config.fish")
            .contains("set -gx AWS_PROFILE \"dev\"")
    );
}

#[test]
fn update_no_change_makes_no_backup() {
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "export AWS_PROFILE=\"dev\"\n");
    let o = h.run("/bin/zsh", &[], "dev\n");
    assert_eq!(o.status.code(), Some(0));
    assert!(stderr(&o).contains("INFO: AWS_PROFILE already set to dev, no changes needed"));
    assert!(h.backups(".zshrc").is_empty());
}

#[test]
fn update_fish_creates_missing_config() {
    let h = Home::new(Some(CFG));
    let o = h.run("/usr/bin/fish", &[], "dev\n");
    assert_eq!(o.status.code(), Some(0));
    assert!(
        h.read(".config/fish/config.fish")
            .starts_with("# Created by AWS Pick\n\n")
    );
}

#[test]
fn shared_profile_written_atomically() {
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "");
    h.run("/bin/zsh", &[], "prod\n");
    assert_eq!(h.read(".config/awspick/profile"), "prod\n");
    assert!(!h.exists(".config/awspick/profile.tmp"));
}

#[test]
fn help_lists_every_option() {
    let h = Home::new(Some(CFG));
    let o = h.run("/bin/zsh", &["-h"], "");
    assert_eq!(o.status.code(), Some(0));
    let out = stdout(&o);
    for opt in [
        "-f, --filter",
        "-x, --exclude",
        "-g, --groups",
        "--group-rules",
        "--regex",
        "--case-sensitive",
        "AWS profile picker",
    ] {
        assert!(out.contains(opt), "missing {opt}:\n{out}");
    }
    assert!(!out.contains("--export-command"));
}

#[test]
fn usage_errors_exit_2() {
    let h = Home::new(Some(CFG));
    for args in [&["--grou", "x"][..], &["extra"], &["-f"], &["--bogus"]] {
        assert_eq!(
            h.run("/bin/zsh", args, "").status.code(),
            Some(2),
            "{args:?}"
        );
    }
}

// --- Phase 3 bug fixes ----------------------------------------------------

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// Bug 1: the fish pattern never matched the `set -gx` line awspick writes,
/// so every switch appended another block.
#[test]
fn fish_switch_replaces_the_existing_line() {
    let h = Home::new(Some(CFG));
    h.write(".config/fish/config.fish", "set -gx PATH x\n");
    h.run("/usr/bin/fish", &[], "prod\n");
    h.run("/usr/bin/fish", &[], "dev\n");
    let rc = h.read(".config/fish/config.fish");
    assert_eq!(count(&rc, "AWS_PROFILE"), 1, "{rc}");
    assert!(rc.contains("set -gx AWS_PROFILE \"dev\""), "{rc}");
    // The value in config.fish is recognised as the current profile.
    let e = stderr(&h.run("/usr/bin/fish", &[], "q\n"));
    let line = e.lines().find(|l| l.contains(" dev ")).unwrap();
    assert!(line.contains('*'), "{line}");
    for flags in ["-x", "-g", "-xg", "--global --export"] {
        h.write(
            ".config/fish/config.fish",
            &format!("set {flags} AWS_PROFILE prod\n"),
        );
        h.run("/usr/bin/fish", &[], "dev\n");
        let rc = h.read(".config/fish/config.fish");
        assert_eq!(rc, "set -gx AWS_PROFILE \"dev\"\n", "{flags}");
    }
}

/// Bug 2: re.sub expanded backslash escapes from the profile name, so `a\1`
/// wrote `export AWS_PROFILE="a"c""` and `b\yz` crashed. The replacement is
/// literal (regex::NoExpand) since the Phase 1 port.
#[test]
fn backslash_in_profile_name_is_written_literally() {
    let h = Home::new(Some(
        "[profile a\\1]\n[profile b\\yz]\n[profile c$1]\n[profile d]\n",
    ));
    for name in ["a\\1", "b\\yz", "c$1"] {
        h.write(".zshrc", "export AWS_PROFILE=\"d\"\n");
        let o = h.run("/bin/zsh", &[], &format!("{name}\n"));
        assert_eq!(o.status.code(), Some(0), "{name}");
        let rc = h.read(".zshrc");
        assert_eq!(count(&rc, "AWS_PROFILE"), 1, "{rc}");
        assert!(rc.contains(name), "{name}: {rc}");
        assert!(!rc.contains("\"d\""), "{rc}");
    }
}

/// Bug 3: the profile name went unescaped into `"..."`, both in the rc file
/// and in the line printed for `eval`. A name from ~/.aws/config could run
/// commands. Profiles are picked by number and checked against the name
/// awspick reports, so the test does not depend on how section names are
/// parsed.
#[test]
fn profile_names_are_quoted_for_the_shell() {
    let config = "[profile x$(touch PWNED)]\n[profile y\";touch PWN2;\"]\n[profile it's]\n\
                  [profile back`touch PWN3`]\n[profile \"a b\"]\n[profile c\\d]\n";
    let h = Home::new(Some(config));
    for n in 1..=6 {
        h.write(".zshrc", "");
        let o = h.run("/bin/zsh", &[], &format!("{n}\n"));
        assert_eq!(o.status.code(), Some(0), "{n}: {}", stderr(&o));
        let e = stderr(&o);
        let name = e
            .lines()
            .find_map(|l| l.split_once("Selected profile: ").map(|(_, n)| n))
            .unwrap()
            .to_owned();
        // eval the printed line and source the rc file in a POSIX shell.
        for script in [
            format!(
                "eval {}; printf %s \"$AWS_PROFILE\"",
                shell_quote(stdout(&o).trim_end())
            ),
            ". ./.zshrc; printf %s \"$AWS_PROFILE\"".to_owned(),
        ] {
            let out = Command::new("/bin/sh")
                .arg("-c")
                .arg(&script)
                .current_dir(&h.path)
                .output()
                .unwrap();
            assert_eq!(String::from_utf8_lossy(&out.stdout), name, "{script}");
        }
        for f in ["PWNED", "PWN2", "PWN3"] {
            assert!(!h.exists(f), "{name} ran a command");
        }
        // Selecting the same profile again is recognised as no change.
        let again = h.run("/bin/zsh", &[], &format!("{n}\n"));
        assert!(
            stderr(&again).contains("no changes needed"),
            "{name}: {}",
            stderr(&again)
        );
    }
}

/// Single-quote a string for /bin/sh.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn wait_for_fresh_second() {
    use std::time::{SystemTime, UNIX_EPOCH};
    while SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_millis()
        > 200
    {
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Bug 4: backups are named by the second, so a second switch within the
/// same second overwrote the first backup and lost the original rc file.
#[test]
fn backups_in_the_same_second_do_not_overwrite_each_other() {
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "ORIGINAL\n");
    wait_for_fresh_second();
    h.run("/bin/zsh", &[], "prod\n");
    h.run("/bin/zsh", &[], "dev\n");
    let backups = h.backups(".zshrc");
    assert_eq!(backups.len(), 2, "{backups:?}");
    let contents: Vec<String> = backups.iter().map(|b| h.read(b)).collect();
    assert!(contents.contains(&"ORIGINAL\n".to_owned()), "{contents:?}");
}

/// Bug 5: rotation deleted every `<rc>.bak-*` file, including ones the user
/// made by hand.
#[test]
fn rotation_leaves_other_backup_files_alone() {
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "x\n");
    h.write(".zshrc.bak-0-before-migration", "MANUAL\n");
    h.write(".zshrc.bak-tabfix-20260208", "MANUAL2\n");
    h.write(".zshrc.bak-20240101000000", "old1\n");
    h.write(".zshrc.bak-20250101000000", "old2\n");
    h.run("/bin/zsh", &[], "dev\n");
    assert_eq!(h.read(".zshrc.bak-0-before-migration"), "MANUAL\n");
    assert_eq!(h.read(".zshrc.bak-tabfix-20260208"), "MANUAL2\n");
    assert!(!h.exists(".zshrc.bak-20240101000000"));
    assert!(h.exists(".zshrc.bak-20250101000000"));
}

/// Bug 6: end of input at the prompt raised EOFError with a traceback.
#[test]
fn end_of_input_cancels_like_an_empty_answer() {
    let h = Home::new(Some(CFG));
    h.write(".zshrc", "");
    for input in ["", "nope\n"] {
        let o = h.run("/bin/zsh", &[], input);
        assert_eq!(o.status.code(), Some(1));
        let e = stderr(&o);
        assert!(e.contains("No selection made."), "{e}");
        assert!(!e.contains("unexpected error"), "{e}");
        assert!(stdout(&o).is_empty());
    }
}
