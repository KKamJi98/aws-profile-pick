//! Reading `~/.aws/config`, filtering, grouping and validating a selection.
//!
//! The section parser reproduces what Python's `configparser.ConfigParser`
//! (3.13, default options) accepted and rejected, because awspick 2.x listed
//! exactly the sections that parser returned and listed nothing when it
//! raised. Only section names are kept; option values are never stored, so a
//! secret in the file cannot reach an error message.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::out;
use crate::py;

/// The config file the AWS CLI reads: `$AWS_CONFIG_FILE` (with `~` and
/// `$VAR` expanded, as botocore does), else `~/.aws/config`. 2.x ignored
/// `AWS_CONFIG_FILE`.
pub fn aws_config_path(home: &Path) -> PathBuf {
    match std::env::var("AWS_CONFIG_FILE") {
        Ok(p) if !p.is_empty() => expand_user(&expand_vars(&p), home),
        _ => home.join(".aws").join("config"),
    }
}

/// `os.path.expanduser` for the current user only.
fn expand_user(p: &str, home: &Path) -> PathBuf {
    if p == "~" {
        home.to_path_buf()
    } else if let Some(rest) = p.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(p)
    }
}

/// `os.path.expandvars`: `$NAME` and `${NAME}`; unknown names stay as is.
fn expand_vars(p: &str) -> String {
    let re = Regex::new(r"\$(\w+|\{[^}]*\})").expect("static pattern");
    re.replace_all(p, |c: &regex::Captures| {
        let name = c[1].trim_start_matches('{').trim_end_matches('}');
        std::env::var(name).unwrap_or_else(|_| c[0].to_owned())
    })
    .into_owned()
}

/// `shlex.split` (POSIX mode, no comments). `None` where Python raises
/// ValueError (an open quote or a trailing backslash).
fn shlex_split(s: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\r' | '\n' => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '\'' => break,
                        ch => cur.push(ch),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => match chars.next()? {
                            e @ ('"' | '\\') => cur.push(e),
                            e => {
                                cur.push('\\');
                                cur.push(e);
                            }
                        },
                        ch => cur.push(ch),
                    }
                }
            }
            '\\' => {
                in_word = true;
                cur.push(chars.next()?);
            }
            ch => {
                in_word = true;
                cur.push(ch);
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    Some(words)
}

/// The profile a section names under botocore's rules
/// (`configloader.build_profile_map`): `default`, or a section starting with
/// `profile` that shlex splits into exactly two words, the second being the
/// name. So `[profile  a]` is `a`, `[profile "a b"]` is `a b`, and
/// `[profile a b]` is not a profile.
fn profile_name(section: &str) -> Option<String> {
    if section.starts_with("profile") {
        let parts = shlex_split(section)?;
        if parts.len() == 2 {
            return parts.into_iter().nth(1);
        }
        return None;
    }
    (section == "default").then(|| "default".to_owned())
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    MissingSectionHeader {
        lineno: usize,
    },
    DuplicateSection {
        section: String,
        lineno: usize,
    },
    DuplicateOption {
        section: String,
        option: String,
        lineno: usize,
    },
    Parsing {
        linenos: Vec<usize>,
    },
}

impl ParseError {
    /// The configparser message without the offending line's text. The
    /// Python release printed that text, which can be a secret key.
    pub fn message(&self, path: &Path) -> String {
        let src = py::repr_posix_path(path);
        match self {
            ParseError::MissingSectionHeader { lineno } => {
                format!("File contains no section headers.\nfile: {src}, line: {lineno}")
            }
            ParseError::DuplicateSection { section, lineno } => format!(
                "While reading from {src} [line {lineno:2}]: section {} already exists",
                py::repr_str(section)
            ),
            ParseError::DuplicateOption {
                section,
                option,
                lineno,
            } => format!(
                "While reading from {src} [line {lineno:2}]: option {} in section {} already exists",
                py::repr_str(option),
                py::repr_str(section)
            ),
            ParseError::Parsing { linenos } => {
                let mut m = format!("Source contains parsing errors: {src}");
                for n in linenos {
                    m.push_str(&format!("\n\t[line {n:2}]"));
                }
                m
            }
        }
    }
}

const DEFAULT_SECTION: &str = "DEFAULT";

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| py::is_space(*c)).count()
}

/// Return the section names in file order, or the error configparser raised.
/// `text` must already have had universal-newline translation applied.
pub fn parse_sections(text: &str) -> Result<Vec<String>, ParseError> {
    let mut sections: Vec<String> = Vec::new();
    let mut known: HashSet<String> = HashSet::new();
    let mut added_sections: HashSet<String> = HashSet::new();
    let mut added_options: HashSet<(String, String)> = HashSet::new();
    let mut errors: Vec<usize> = Vec::new();

    // State mirrors configparser's _ReadState.
    let mut cursect: Option<String> = None;
    let mut optname: Option<String> = None;
    let mut indent_level: usize = 0;

    for (idx, line) in text.split_inclusive('\n').enumerate() {
        let lineno = idx + 1;
        let stripped = py::strip(line);
        let is_comment = stripped.starts_with('#') || stripped.starts_with(';');
        let clean = if is_comment { "" } else { stripped };
        if clean.is_empty() {
            // Blank lines extend a multi-line value; comments do not. Either
            // way nothing that matters for section names changes.
            continue;
        }
        let cur_indent = indent_of(line);

        // Continuation line of the current option?
        if cursect.is_some()
            && optname.as_deref().is_some_and(|o| !o.is_empty())
            && cur_indent > indent_level
        {
            continue;
        }

        indent_level = cur_indent;
        if let Some(header) = section_header(clean) {
            let name = header.to_owned();
            if known.contains(&name) {
                if added_sections.contains(&name) {
                    return Err(ParseError::DuplicateSection {
                        section: name,
                        lineno,
                    });
                }
                added_sections.insert(name.clone());
            } else if name != DEFAULT_SECTION {
                known.insert(name.clone());
                sections.push(name.clone());
                added_sections.insert(name.clone());
            }
            cursect = Some(name);
            optname = None;
            continue;
        }
        let Some(sect) = cursect.clone() else {
            return Err(ParseError::MissingSectionHeader { lineno });
        };
        // Option line: needs a '=' or ':' delimiter.
        match clean.find(['=', ':']) {
            None => {
                errors.push(lineno);
            }
            Some(pos) => {
                let raw = &clean[..pos];
                if raw.is_empty() {
                    errors.push(lineno);
                }
                let name = py::lower(py::rstrip(raw));
                let key = (sect.clone(), name.clone());
                if added_options.contains(&key) {
                    return Err(ParseError::DuplicateOption {
                        section: sect,
                        option: name,
                        lineno,
                    });
                }
                added_options.insert(key);
                optname = Some(name);
            }
        }
    }
    if !errors.is_empty() {
        return Err(ParseError::Parsing { linenos: errors });
    }
    Ok(sections)
}

/// `SECTCRE.match`: a leading '[', a greedy run of at least one character,
/// and the last ']' on the line. Anything after that ']' is ignored.
fn section_header(clean: &str) -> Option<&str> {
    let rest = clean.strip_prefix('[')?;
    let end = rest.rfind(']')?;
    if end == 0 {
        // "[]" alone has no header; "[]]" has "]" as its header.
        return None;
    }
    Some(&rest[..end])
}

/// Read profile names from the AWS config file and the shared credentials
/// file, logging as awspick 2.x did. 2.x read only the config file; the AWS
/// CLI (`aws configure list-profiles`) also lists the credentials file's
/// sections, and so does awspick now.
pub fn read_aws_profiles(home: &Path) -> Vec<String> {
    let path = aws_config_path(home);
    let config_exists = path.exists();
    let mut profiles: Vec<String> = Vec::new();
    if config_exists {
        let Some(sections) = read_config_sections(&path) else {
            return Vec::new();
        };
        // Each name once, as in botocore's profile map: [default] and
        // [profile default] are the same profile.
        for section in sections {
            if let Some(name) = profile_name(&section)
                && !profiles.contains(&name)
            {
                profiles.push(name);
            }
        }
    }
    let credentials = read_credentials_profiles(&aws_credentials_path(home));
    if !config_exists && credentials.is_empty() {
        out::error(&format!("AWS config file not found at {}", path.display()));
        return Vec::new();
    }
    for name in credentials {
        if !profiles.contains(&name) {
            profiles.push(name);
        }
    }
    out::info(&format!("Found {} AWS profiles", profiles.len()));
    profiles.sort();
    profiles
}

/// The config file's sections, or `None` after logging why it was rejected.
fn read_config_sections(path: &Path) -> Option<Vec<String>> {
    // configparser.read() silently skips files it cannot open.
    let Ok(bytes) = std::fs::read(path) else {
        return Some(Vec::new());
    };
    let text = match py::decode_utf8(&bytes) {
        Ok(t) => py::universal_newlines(&t),
        Err(msg) => {
            out::error(&format!("Unexpected error reading AWS profiles: {msg}"));
            return None;
        }
    };
    match parse_sections(&text) {
        Ok(s) => Some(s),
        Err(e) => {
            out::error(&format!(
                "Error parsing AWS config file: {}",
                e.message(path)
            ));
            None
        }
    }
}

/// The shared credentials file the AWS CLI reads:
/// `$AWS_SHARED_CREDENTIALS_FILE` (expanded like `AWS_CONFIG_FILE`), else
/// `~/.aws/credentials`.
pub fn aws_credentials_path(home: &Path) -> PathBuf {
    match std::env::var("AWS_SHARED_CREDENTIALS_FILE") {
        Ok(p) if !p.is_empty() => expand_user(&expand_vars(&p), home),
        _ => home.join(".aws").join("credentials"),
    }
}

/// Profile names in the credentials file. botocore takes every section name
/// literally (no `profile ` prefix) except configparser's `DEFAULT`. The
/// file is parsed only to find its sections and to reject what botocore
/// rejects; no value is kept, and a broken file is skipped with a warning
/// instead of hiding the config file's profiles.
fn read_credentials_profiles(path: &Path) -> Vec<String> {
    if !path.is_file() {
        return Vec::new();
    }
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let parsed = py::decode_utf8(&bytes)
        .and_then(|t| parse_sections(&py::universal_newlines(&t)).map_err(|e| e.message(path)));
    match parsed {
        Ok(sections) => sections,
        Err(msg) => {
            out::warning(&format!(
                "Ignoring AWS credentials file {}: {msg}",
                path.display()
            ));
            Vec::new()
        }
    }
}

/// Ordered group rules: `[(group, [keywords])]`.
pub fn parse_group_rules(rules: Option<&str>) -> Vec<(String, Vec<String>)> {
    let rules = match rules {
        Some(r) if !r.is_empty() => r,
        _ => {
            return ["prod", "stg", "dev", "preprod"]
                .iter()
                .map(|g| ((*g).to_owned(), vec![(*g).to_owned()]))
                .collect();
        }
    };
    let mut ordered = Vec::new();
    for part in rules.split(';') {
        let part = py::strip(part);
        if part.is_empty() {
            continue;
        }
        let (name, keywords) = if let Some((name, kws)) = part.split_once('=') {
            let keywords = kws
                .split(',')
                .map(py::strip)
                .filter(|k| !k.is_empty())
                .map(str::to_owned)
                .collect();
            (py::strip(name).to_owned(), keywords)
        } else if py::lower(part) == "others" {
            ("others".to_owned(), vec!["*".to_owned()])
        } else {
            (part.to_owned(), vec![part.to_owned()])
        };
        if !name.is_empty() {
            ordered.push((name, keywords));
        }
    }
    ordered
}

/// Pattern matching for `--filter` / `--exclude`.
pub struct Matcher {
    regex: bool,
    case_sensitive: bool,
    cache: HashMap<String, fancy_regex::Regex>,
}

impl Matcher {
    pub fn new(regex: bool, case_sensitive: bool) -> Self {
        Matcher {
            regex,
            case_sensitive,
            cache: HashMap::new(),
        }
    }

    /// `_match_any`. Patterns compile lazily in the order Python's `any()`
    /// reached them, so an invalid pattern only fails when it is evaluated.
    pub fn any(&mut self, text: &str, patterns: &[String]) -> Result<bool, String> {
        if self.regex {
            for p in patterns {
                if !self.cache.contains_key(p) {
                    let re = fancy_regex::RegexBuilder::new(p)
                        .case_insensitive(!self.case_sensitive)
                        .build()
                        .map_err(|e| format!("invalid regex {}: {e}", py::repr_str(p)))?;
                    self.cache.insert(p.clone(), re);
                }
                let matched = self.cache[p].is_match(text).map_err(|e| {
                    format!(
                        "regex {} failed on {}: {e}",
                        py::repr_str(p),
                        py::repr_str(text)
                    )
                })?;
                if matched {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        if self.case_sensitive {
            Ok(patterns.iter().any(|p| text.contains(p.as_str())))
        } else {
            let text = py::lower(text);
            Ok(patterns
                .iter()
                .any(|p| text.contains(py::lower(p).as_str())))
        }
    }
}

pub fn filter_profiles(
    profiles: &[String],
    include: Option<&[String]>,
    exclude: Option<&[String]>,
    matcher: &mut Matcher,
) -> Result<Vec<String>, String> {
    let mut result: Vec<String> = profiles.to_vec();
    if let Some(inc) = include.filter(|v| !v.is_empty()) {
        let mut kept = Vec::new();
        for p in result {
            if matcher.any(&p, inc)? {
                kept.push(p);
            }
        }
        result = kept;
    }
    if let Some(exc) = exclude.filter(|v| !v.is_empty()) {
        let mut kept = Vec::new();
        for p in result {
            if !matcher.any(&p, exc)? {
                kept.push(p);
            }
        }
        result = kept;
    }
    Ok(result)
}

/// `_match_group_keyword`: the keyword as a whole token, bounded by the
/// string ends, a word boundary, '-' or '_'.
fn keyword_regex(keyword: &str) -> Regex {
    let pattern = format!(r"(?i)(^|\b|[-_]){}($|\b|[-_])", regex::escape(keyword));
    Regex::new(&pattern).expect("escaped keyword is a valid pattern")
}

pub struct GroupOptions<'a> {
    pub group_rules: Option<&'a str>,
    pub show_groups: Option<&'a [String]>,
    pub include: Option<&'a [String]>,
    pub exclude: Option<&'a [String]>,
    pub regex: bool,
    pub case_sensitive: bool,
}

/// `get_grouped_profiles`: `[(profile, group)]` in display order.
pub fn get_grouped_profiles(
    profiles: &[String],
    opts: &GroupOptions,
) -> Result<Vec<(String, String)>, String> {
    let mut matcher = Matcher::new(opts.regex, opts.case_sensitive);
    let filtered = filter_profiles(profiles, opts.include, opts.exclude, &mut matcher)?;

    let ordered = parse_group_rules(opts.group_rules);
    let mut group_order: Vec<String> = ordered.iter().map(|(n, _)| n.clone()).collect();
    let explicit: Vec<(&String, Vec<Regex>)> = ordered
        .iter()
        .filter(|(_, kws)| !kws.iter().any(|k| k == "*"))
        .map(|(n, kws)| (n, kws.iter().map(|k| keyword_regex(k)).collect()))
        .collect();
    let catchall = ordered
        .iter()
        .find(|(_, kws)| kws.iter().any(|k| k == "*"))
        .map(|(n, _)| n.clone())
        .unwrap_or_else(|| "others".to_owned());

    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    for name in &group_order {
        groups.entry(name.clone()).or_default();
    }
    groups.entry(catchall.clone()).or_default();

    for profile in filtered {
        let target = explicit
            .iter()
            .find(|(_, res)| res.iter().any(|re| re.is_match(&profile)))
            .map(|(n, _)| (*n).clone())
            .unwrap_or_else(|| catchall.clone());
        groups.get_mut(&target).expect("group exists").push(profile);
    }

    let allowed: Option<HashSet<String>> = opts
        .show_groups
        .filter(|g| !g.is_empty())
        .map(|g| g.iter().map(|s| py::strip(s).to_owned()).collect());

    if !group_order.contains(&catchall) {
        group_order.push(catchall);
    }

    let mut grouped = Vec::new();
    for grp in &group_order {
        if let Some(a) = &allowed
            && !a.contains(grp)
        {
            continue;
        }
        let mut members = groups[grp].clone();
        members.sort();
        for p in members {
            grouped.push((p, grp.clone()));
        }
    }
    Ok(grouped)
}

/// `validate_profile_selection`.
/// The profile named on the command line (`awspick <profile>`): an exact
/// match, else the one profile that matches ignoring case. Numbers are
/// names here; selecting by number needs the table.
pub fn find_profile(name: &str, profiles: &[String]) -> Result<String, String> {
    if profiles.iter().any(|p| p == name) {
        return Ok(name.to_owned());
    }
    let lowered = py::lower(name);
    let matches: Vec<&String> = profiles
        .iter()
        .filter(|p| py::lower(p) == lowered)
        .collect();
    match matches.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(format!("Profile {} not found", py::repr_str(name))),
        many => Err(format!(
            "Profile {} matches more than one profile when case is ignored: {}",
            py::repr_str(name),
            many.iter()
                .map(|p| py::repr_str(p))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

pub fn validate_profile_selection(selection: &str, profiles: &[String]) -> Option<String> {
    if selection.is_empty() {
        out::error("Empty selection");
        return None;
    }
    if selection.bytes().all(|b| b.is_ascii_digit()) {
        let digits = selection.trim_start_matches('0');
        let index = if digits.len() > 19 {
            None
        } else {
            digits.parse::<u64>().ok().and_then(|n| n.checked_sub(1))
        };
        if let Some(i) = index
            && (i as u128) < profiles.len() as u128
        {
            return Some(profiles[i as usize].clone());
        }
        out::error(&format!(
            "Invalid profile number: {selection}. Valid range is 1-{}",
            profiles.len()
        ));
        return None;
    }
    if profiles.iter().any(|p| p == selection) {
        return Some(selection.to_owned());
    }
    let lowered = py::lower(selection);
    for p in profiles {
        if py::lower(p) == lowered {
            out::info(&format!(
                "Found case-insensitive match for '{selection}': '{p}'"
            ));
            return Some(p.clone());
        }
    }
    out::error(&format!(
        "Profile '{selection}' not found in available profiles"
    ));
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn sections_like_configparser() {
        let text = "[default]\n[profile a[b]]\n[profile  two]\n[ profile x ]\n[DEFAULT]\n";
        assert_eq!(
            parse_sections(text).unwrap(),
            s(&["default", "profile a[b]", "profile  two", " profile x "])
        );
    }

    #[test]
    fn duplicate_option_is_an_error() {
        let e = parse_sections("[profile a]\nregion=x\nRegion = y\n").unwrap_err();
        assert_eq!(
            e,
            ParseError::DuplicateOption {
                section: "profile a".into(),
                option: "region".into(),
                lineno: 3
            }
        );
    }

    #[test]
    fn parsing_error_hides_line_text() {
        let e = parse_sections("[profile a]\nregion=x\nSECRETKEYMATERIAL\n").unwrap_err();
        let msg = e.message(Path::new("/h/.aws/config"));
        assert!(!msg.contains("SECRET"));
        assert_eq!(
            msg,
            "Source contains parsing errors: PosixPath('/h/.aws/config')\n\t[line  3]"
        );
    }

    #[test]
    fn indented_header_after_option_is_a_continuation() {
        let text = "[profile a]\nregion = x\n  [profile b]\n";
        assert_eq!(parse_sections(text).unwrap(), s(&["profile a"]));
    }

    #[test]
    fn option_before_section() {
        assert_eq!(
            parse_sections("region=x\n").unwrap_err(),
            ParseError::MissingSectionHeader { lineno: 1 }
        );
    }

    #[test]
    fn default_rules() {
        let r = parse_group_rules(None);
        assert_eq!(r[0], ("prod".into(), s(&["prod"])));
        assert_eq!(r.len(), 4);
    }

    #[test]
    fn others_marker_and_wildcard() {
        let r = parse_group_rules(Some("others;tf=tf"));
        assert_eq!(
            r,
            vec![("others".into(), s(&["*"])), ("tf".into(), s(&["tf"]))]
        );
        let r = parse_group_rules(Some("main=*;tf=tf"));
        assert_eq!(r[0], ("main".into(), s(&["*"])));
    }

    #[test]
    fn grouping_uses_token_boundaries() {
        let profiles = s(&["acme-prod", "acme-preprod", "production", "dev_x"]);
        let opts = GroupOptions {
            group_rules: None,
            show_groups: None,
            include: None,
            exclude: None,
            regex: false,
            case_sensitive: false,
        };
        let g = get_grouped_profiles(&profiles, &opts).unwrap();
        assert_eq!(
            g,
            vec![
                ("acme-prod".into(), "prod".into()),
                ("dev_x".into(), "dev".into()),
                ("acme-preprod".into(), "preprod".into()),
                ("production".into(), "others".into()),
            ]
        );
    }

    #[test]
    fn selection_by_number_and_name() {
        let p = s(&["a", "B"]);
        assert_eq!(validate_profile_selection("2", &p), Some("B".into()));
        assert_eq!(validate_profile_selection("002", &p), Some("B".into()));
        assert_eq!(validate_profile_selection("b", &p), Some("B".into()));
        assert_eq!(validate_profile_selection("0", &p), None);
        assert_eq!(
            validate_profile_selection("99999999999999999999999", &p),
            None
        );
        assert_eq!(validate_profile_selection("zz", &p), None);
    }
    /// Differential test against CPython 3.13 configparser
    /// (tests/golden/configparser.json, written by scripts/gen_golden.py).
    #[test]
    fn matches_cpython_configparser() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/golden/configparser.json"
        );
        let raw = std::fs::read_to_string(path).unwrap();
        let cases: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();
        for c in cases {
            let text = crate::py::universal_newlines(c["text"].as_str().unwrap());
            let got = parse_sections(&text);
            let want = if let Some(sections) = c.get("sections") {
                Ok(sections
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>())
            } else {
                let n = |k: &str| c[k].as_u64().unwrap() as usize;
                let t = |k: &str| c[k].as_str().unwrap().to_owned();
                Err(match c["error"].as_str().unwrap() {
                    "MissingSectionHeader" => ParseError::MissingSectionHeader {
                        lineno: n("lineno"),
                    },
                    "DuplicateSection" => ParseError::DuplicateSection {
                        section: t("section"),
                        lineno: n("lineno"),
                    },
                    "DuplicateOption" => ParseError::DuplicateOption {
                        section: t("section"),
                        option: t("option"),
                        lineno: n("lineno"),
                    },
                    "Parsing" => ParseError::Parsing {
                        linenos: c["linenos"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_u64().unwrap() as usize)
                            .collect(),
                    },
                    other => panic!("unknown error kind {other}"),
                })
            };
            assert_eq!(got, want, "text: {:?}", c["text"]);
        }
    }

    #[test]
    fn profile_names_follow_botocore() {
        let n = |s: &str| profile_name(s);
        assert_eq!(n("default"), Some("default".into()));
        assert_eq!(n("profile default"), Some("default".into()));
        assert_eq!(n("profile  two"), Some("two".into()));
        assert_eq!(n("profile \"a b\""), Some("a b".into()));
        assert_eq!(n("profile 'it''s'"), Some("its".into()));
        assert_eq!(n("profile a[b]"), Some("a[b]".into()));
        assert_eq!(n("profilefoo bar"), Some("bar".into()));
        assert_eq!(n("profile a b"), None);
        assert_eq!(n("profile \"open"), None);
        assert_eq!(n("profileX"), None);
        assert_eq!(n(" profile x"), None);
        assert_eq!(n("sso-session s"), None);
    }

    #[test]
    fn shlex_matches_python() {
        assert_eq!(shlex_split("a\"b c\"d").unwrap(), vec!["ab cd"]);
        assert_eq!(shlex_split("\"a\\\"b\\x\"").unwrap(), vec!["a\"b\\x"]);
        assert_eq!(shlex_split("a\\ b").unwrap(), vec!["a b"]);
        assert_eq!(shlex_split("x \\"), None);
        assert_eq!(shlex_split("''").unwrap(), vec![""]);
    }
}
