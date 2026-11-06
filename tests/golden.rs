//! Replays tests/golden/cases.json against the Rust binary and compares the
//! result with what awspick 2.0.2 (Python) produced for the same case.
//! Regenerate the expected side with `uv run scripts/gen_golden.py`.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use regex::Regex;
use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_awspick");

fn golden_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn b64(s: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .expect("valid base64")
}

fn materialize(home: &Path, case: &Value) {
    for (rel, value) in case["files"].as_object().unwrap() {
        let p = home.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        if let Some(t) = value.get("symlink") {
            symlink(t.as_str().unwrap(), &p).unwrap();
        } else if value.get("dir").is_some() {
            fs::create_dir_all(&p).unwrap();
        } else if let Some(b) = value.get("b64") {
            fs::write(&p, b64(b.as_str().unwrap())).unwrap();
        } else {
            fs::write(&p, value.as_str().unwrap()).unwrap();
        }
    }
    for (rel, mode) in case["modes"].as_object().unwrap() {
        let m = u32::from_str_radix(mode.as_str().unwrap(), 8).unwrap();
        fs::set_permissions(home.join(rel), fs::Permissions::from_mode(m)).unwrap();
    }
}

fn manifest(home: &Path) -> Value {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<Value>) {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        names.sort();
        for p in names {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            let meta = fs::symlink_metadata(&p).unwrap();
            if meta.file_type().is_symlink() {
                let t = fs::read_link(&p).unwrap();
                out.push(json!({"path": rel, "type": "symlink", "target": t.to_string_lossy()}));
            } else if meta.is_dir() {
                out.push(json!({"path": rel, "type": "dir",
                    "mode": format!("{:o}", meta.permissions().mode() & 0o7777)}));
                walk(root, &p, out);
            } else {
                let data = fs::read(&p).unwrap();
                let content = match String::from_utf8(data.clone()) {
                    Ok(s) => json!(s),
                    Err(_) => {
                        json!({"b64": base64::engine::general_purpose::STANDARD.encode(&data)})
                    }
                };
                out.push(json!({"path": rel, "type": "file",
                    "mode": format!("{:o}", meta.permissions().mode() & 0o7777),
                    "content": content}));
            }
        }
    }
    let mut v = Vec::new();
    walk(home, home, &mut v);
    v.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Value::Array(v)
}

fn wait_for_fresh_second() {
    loop {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        if now.subsec_millis() < 200 {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn run_case(case: &Value) -> Value {
    let tmp = tempfile::Builder::new()
        .prefix("awspick-golden-")
        .tempdir()
        .unwrap();
    let home = tmp.path().join("home");
    fs::create_dir(&home).unwrap();
    materialize(&home, case);
    let home_s = home.to_string_lossy().into_owned();

    let mut env: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in case["env"].as_object().unwrap() {
        if let Some(s) = v.as_str() {
            env.insert(k.clone(), s.to_owned());
        }
    }
    env.insert("HOME".into(), home_s.clone());
    if let Some(z) = env.get("ZDOTDIR").cloned() {
        env.insert(
            "ZDOTDIR".into(),
            home.join(z).to_string_lossy().into_owned(),
        );
    }
    if case["same_second"].as_bool().unwrap_or(false) {
        wait_for_fresh_second();
    }
    let mut runs = Vec::new();
    for r in case["runs"].as_array().unwrap() {
        let args: Vec<&str> = r["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg("\"$@\"; exit $?")
            .arg("sh")
            .arg(BIN)
            .args(&args)
            .env_clear()
            .envs(&env)
            .current_dir(&home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(r["stdin"].as_str().unwrap().as_bytes())
            .unwrap();
        let o = child.wait_with_output().unwrap();
        runs.push(json!({
            "stdout": String::from_utf8_lossy(&o.stdout).replace(&home_s, "<HOME>"),
            "stderr": String::from_utf8_lossy(&o.stderr).replace(&home_s, "<HOME>"),
            "exit": o.status.code().unwrap_or(-1),
        }));
    }
    json!({"runs": runs, "files": manifest(&home)})
}

/// Replace backup timestamps created during the run with <TS1>, <TS2>, ...
/// in ascending order. Timestamps present in the case's own files stay.
fn normalize(result: &Value, case: &Value) -> String {
    let text = serde_json::to_string_pretty(result).unwrap();
    let re = Regex::new(r"\.bak-(\d{14})").unwrap();
    let initial: Vec<String> = case["files"]
        .as_object()
        .unwrap()
        .keys()
        .filter_map(|k| re.captures(k).map(|c| c[1].to_owned()))
        .collect();
    let mut stamps: Vec<String> = re
        .captures_iter(&text)
        .map(|c| c[1].to_owned())
        .filter(|s| !initial.contains(s))
        .collect();
    stamps.sort();
    stamps.dedup();
    let mut out = text;
    for (i, s) in stamps.iter().enumerate() {
        out = out.replace(&format!(".bak-{s}"), &format!(".bak-<TS{}>", i + 1));
    }
    out
}

fn select(v: &Value, compare: &[String]) -> Value {
    let runs: Vec<Value> = v["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let mut m = serde_json::Map::new();
            for f in ["stdout", "stderr", "exit"] {
                if compare.iter().any(|c| c == f) {
                    m.insert(f.into(), r[f].clone());
                }
            }
            Value::Object(m)
        })
        .collect();
    let mut m = serde_json::Map::new();
    m.insert("runs".into(), Value::Array(runs));
    if compare.iter().any(|c| c == "files") {
        m.insert("files".into(), v["files"].clone());
    }
    Value::Object(m)
}

#[test]
fn golden_fixtures_match_python_release() {
    let cases: Vec<Value> =
        serde_json::from_str(&fs::read_to_string(golden_dir().join("cases.json")).unwrap())
            .unwrap();
    let only = std::env::var("GOLDEN_CASE").ok();
    let mut failures = Vec::new();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let compare: Vec<String> = case["compare"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_owned())
            .collect();
        let expected: Value = serde_json::from_str(
            &fs::read_to_string(golden_dir().join("expected").join(format!("{name}.json")))
                .unwrap(),
        )
        .unwrap();
        let actual = run_case(case);
        for r in actual["runs"].as_array().unwrap() {
            assert_ne!(r["exit"], json!(101), "{name}: the binary panicked");
        }
        let e = normalize(&select(&expected, &compare), case);
        let a = normalize(&select(&actual, &compare), case);
        if e != a {
            failures.push(format!("--- {name}\nexpected:\n{e}\nactual:\n{a}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} golden case(s) differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
