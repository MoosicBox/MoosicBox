use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::{env, fs, str};

use clippier_md::{
    Config, FormatterEngine, HeadingIndentationMode, ListIndentationMode, ListStyle, ProseWrapMode,
    format_markdown,
};
use serde_json::{Value, json};

const PRETTIER_VERSION: &str = "3.8.1";
const PRETTIER_PARSER: &str = "markdown";
const COMMONMARK_REVISION: &str = "31c0ca2d294ea60ab4438004da410e2e590a46f2";
const COMMONMARK_EXAMPLE_COUNT: usize = 655;
const ORACLE_SCHEMA_VERSION: u64 = 1;
const REPORT_SCHEMA_VERSION: u64 = 1;

#[test]
fn prettier_parity_commonmark_gfm_fixtures() {
    assert_prettier_version();
    verify_commonmark_checkout();

    let selection = Selection::from_env().unwrap_or_else(|error| panic!("{error}"));
    let cases = collect_parity_cases();
    let selected = cases
        .into_iter()
        .filter(|case| selection.matches(case))
        .collect::<Vec<_>>();

    assert!(
        !selected.is_empty(),
        "Parity filters selected no cases: {selection:?}"
    );

    let oracle_mode = OracleMode::from_env().unwrap_or_else(|error| panic!("{error}"));
    let config = parity_config();
    let started = std::time::Instant::now();
    let mut reports = Vec::with_capacity(selected.len());

    for case in selected {
        let expected = oracle_output(&case, oracle_mode);
        let actual = format_markdown(&case.input, &config);
        let second = format_markdown(&actual, &config);
        let report = CaseReport::new(case, expected, actual, second);
        if selection.matches_report(&report) {
            reports.push(report);
        }
    }
    assert!(
        !reports.is_empty(),
        "Parity mismatch filter selected no evaluated cases: {selection:?}"
    );

    let aggregate = AggregateReport::new(
        reports,
        selection,
        oracle_mode,
        started.elapsed().as_millis(),
    );
    let report_path = write_report(&aggregate);
    let summary = aggregate.human_summary(&report_path);

    if aggregate.has_failures() {
        panic!("{summary}");
    }

    eprintln!("{summary}");
}

fn parity_config() -> Config {
    Config {
        engine: FormatterEngine::Ast,
        prose_wrap: ProseWrapMode::Always,
        heading_indentation: HeadingIndentationMode::Normalize,
        list_indentation: ListIndentationMode::Normalize,
        list_style: ListStyle::Dash,
        ..Config::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaseKind {
    Commonmark { id: usize },
    Fixture,
}

#[derive(Debug, Clone)]
struct ParityCase {
    name: String,
    kind: CaseKind,
    section: String,
    subsection: String,
    subsystem: String,
    virtual_path: PathBuf,
    input: String,
}

fn collect_parity_cases() -> Vec<ParityCase> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixtures_base = manifest_dir.join("tests/parity/fixtures");
    let mut cases = Vec::new();

    for category in ["commonmark", "gfm"] {
        for dir in collect_fixture_dirs(&fixtures_base.join(category)) {
            let (path, input) = read_fixture_file(&dir, "input").expect("missing input fixture");
            let relative = dir
                .strip_prefix(&fixtures_base)
                .expect("fixture must be below fixture root")
                .to_string_lossy()
                .replace('\\', "/");
            let subsection = relative
                .split_once('/')
                .map_or(relative.as_str(), |(_, value)| value)
                .to_string();
            cases.push(ParityCase {
                name: format!("fixture:{relative}"),
                kind: CaseKind::Fixture,
                section: format!("Curated {category}"),
                subsection: subsection.clone(),
                subsystem: classify_subsystem(&subsection).to_string(),
                virtual_path: path,
                input,
            });
        }
    }

    let spec_path = commonmark_spec_path();
    let spec = fs::read_to_string(&spec_path).unwrap_or_else(|error| {
        panic!(
            "CommonMark corpus is required at '{}': {error}. Run: git submodule update --init --recursive -- packages/clippier/md/tests/vendor/commonmark-spec",
            spec_path.display()
        )
    });
    let examples = parse_commonmark_examples(&spec);
    assert_eq!(
        examples.len(),
        COMMONMARK_EXAMPLE_COUNT,
        "Expected exactly {COMMONMARK_EXAMPLE_COUNT} examples in CommonMark revision {COMMONMARK_REVISION}, parsed {}",
        examples.len()
    );

    cases.extend(examples.into_iter().map(|example| {
        let section_label = if example.subsection.is_empty() {
            example.section.clone()
        } else {
            format!("{} / {}", example.section, example.subsection)
        };
        ParityCase {
            name: format!("commonmark-spec#{}", example.id),
            kind: CaseKind::Commonmark { id: example.id },
            section: example.section,
            subsection: example.subsection,
            subsystem: classify_subsystem(&section_label).to_string(),
            virtual_path: PathBuf::from("commonmark-spec.md"),
            input: example.markdown.replace('→', "\t"),
        }
    }));
    cases
}

fn commonmark_spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/vendor/commonmark-spec/spec.txt")
}

fn verify_commonmark_checkout() {
    let spec_path = commonmark_spec_path();
    let checkout = spec_path
        .parent()
        .expect("CommonMark spec path must have a parent");
    assert!(
        checkout.join("spec.txt").is_file(),
        "CommonMark corpus is missing at '{}'. Run: git submodule update --init --recursive -- packages/clippier/md/tests/vendor/commonmark-spec",
        checkout.display()
    );

    let output = Command::new("git")
        .args(["-C"])
        .arg(checkout)
        .args(["rev-parse", "HEAD"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|error| panic!("Failed to inspect CommonMark submodule revision: {error}"));
    assert!(
        output.status.success(),
        "Failed to inspect CommonMark submodule revision: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let revision = str::from_utf8(&output.stdout)
        .expect("CommonMark revision is not UTF-8")
        .trim();
    assert_eq!(
        revision, COMMONMARK_REVISION,
        "CommonMark submodule is at {revision}, expected {COMMONMARK_REVISION}. Run: git submodule update --init --recursive -- packages/clippier/md/tests/vendor/commonmark-spec"
    );
}

#[derive(Debug, Default, Clone)]
struct Selection {
    id: Option<usize>,
    range: Option<(usize, usize)>,
    section: Option<String>,
    fixture: Option<String>,
    category: Option<String>,
    mismatch: Option<String>,
}

impl Selection {
    fn from_env() -> Result<Self, String> {
        let id = optional_env("CLIPPIER_MD_PARITY_ID")
            .map(|value| {
                value.parse::<usize>().map_err(|_| {
                    format!("CLIPPIER_MD_PARITY_ID must be a positive integer, found {value:?}")
                })
            })
            .transpose()?;
        let range = optional_env("CLIPPIER_MD_PARITY_RANGE")
            .map(|value| parse_range(&value))
            .transpose()?;

        Ok(Self {
            id,
            range,
            section: optional_env("CLIPPIER_MD_PARITY_SECTION"),
            fixture: optional_env("CLIPPIER_MD_PARITY_FIXTURE"),
            category: optional_env("CLIPPIER_MD_PARITY_CATEGORY"),
            mismatch: optional_env("CLIPPIER_MD_PARITY_MISMATCH"),
        })
    }

    fn matches(&self, case: &ParityCase) -> bool {
        let example_id = match case.kind {
            CaseKind::Commonmark { id } => Some(id),
            CaseKind::Fixture => None,
        };
        if self.id.is_some_and(|id| example_id != Some(id)) {
            return false;
        }
        if self
            .range
            .is_some_and(|(start, end)| example_id.is_none_or(|id| id < start || id > end))
        {
            return false;
        }
        if let Some(section) = &self.section
            && (!contains_ignore_ascii_case(&case.section, section)
                && !contains_ignore_ascii_case(&case.subsection, section))
        {
            return false;
        }
        if let Some(fixture) = &self.fixture
            && (case.kind != CaseKind::Fixture || !contains_ignore_ascii_case(&case.name, fixture))
        {
            return false;
        }
        if let Some(category) = &self.category
            && !contains_ignore_ascii_case(&case.subsystem, category)
        {
            return false;
        }
        true
    }

    fn matches_report(&self, report: &CaseReport) -> bool {
        self.mismatch.as_ref().is_none_or(|mismatch| {
            let mismatch = mismatch.to_ascii_lowercase();
            match mismatch.as_str() {
                "parity" => !report.parity,
                "idempotence" => !report.idempotent,
                "passing" | "pass" => {
                    (report.parity || report.deliberate_compatibility_divergence)
                        && report.idempotent
                }
                _ => report
                    .mismatch_shape
                    .as_ref()
                    .is_some_and(|shape| contains_ignore_ascii_case(shape, &mismatch)),
            }
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "range": self.range.map(|(start, end)| format!("{start}-{end}")),
            "section": self.section,
            "fixture": self.fixture,
            "category": self.category,
            "mismatch": self.mismatch,
        })
    }
}

fn optional_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn parse_range(value: &str) -> Result<(usize, usize), String> {
    let (start, end) = value.split_once('-').ok_or_else(|| {
        format!("CLIPPIER_MD_PARITY_RANGE must use inclusive START-END syntax, found {value:?}")
    })?;
    let start = start
        .parse::<usize>()
        .map_err(|_| format!("Invalid parity range start in {value:?}"))?;
    let end = end
        .parse::<usize>()
        .map_err(|_| format!("Invalid parity range end in {value:?}"))?;
    if start == 0 || start > end {
        return Err(format!(
            "Parity range must be positive and ascending, found {value:?}"
        ));
    }
    Ok((start, end))
}

fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OracleMode {
    Live,
    Cache,
    Refresh,
    Verify,
}

impl OracleMode {
    fn from_env() -> Result<Self, String> {
        match optional_env("CLIPPIER_MD_PARITY_ORACLE")
            .unwrap_or_else(|| {
                if oracle_cache_dir().is_dir() {
                    "cache".to_string()
                } else {
                    "live".to_string()
                }
            })
            .to_ascii_lowercase()
            .as_str()
        {
            "live" => Ok(Self::Live),
            "cache" => Ok(Self::Cache),
            "refresh" => Ok(Self::Refresh),
            "verify" => Ok(Self::Verify),
            value => Err(format!(
                "CLIPPIER_MD_PARITY_ORACLE must be live, cache, refresh, or verify; found {value:?}"
            )),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Cache => "cache",
            Self::Refresh => "refresh",
            Self::Verify => "verify",
        }
    }
}

fn oracle_output(case: &ParityCase, mode: OracleMode) -> String {
    match mode {
        OracleMode::Live => run_prettier(&case.virtual_path, &case.input),
        OracleMode::Cache => read_cached_oracle(case),
        OracleMode::Refresh => {
            let output = run_prettier(&case.virtual_path, &case.input);
            write_cached_oracle(case, &output);
            output
        }
        OracleMode::Verify => {
            let cached = read_cached_oracle(case);
            let live = run_prettier(&case.virtual_path, &case.input);
            assert_eq!(
                live, cached,
                "Cached Prettier oracle drifted for '{}'. Regenerate with CLIPPIER_MD_PARITY_ORACLE=refresh",
                case.name
            );
            cached
        }
    }
}

fn oracle_cache_dir() -> PathBuf {
    if let Some(path) = optional_env("CLIPPIER_MD_PARITY_CACHE_DIR") {
        let path = PathBuf::from(path);
        return if path.is_absolute() {
            path
        } else {
            workspace_root().join(path)
        };
    }
    workspace_root().join("target/clippier-md-parity/oracle-v1")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("clippier_md must be located under packages/clippier/md")
        .to_path_buf()
}

fn oracle_cache_path(case: &ParityCase) -> PathBuf {
    oracle_cache_dir().join(format!("{:016x}.json", stable_hash(case.name.as_bytes())))
}

fn oracle_key(case: &ParityCase) -> String {
    let mut bytes = Vec::new();
    for value in [
        PRETTIER_VERSION,
        PRETTIER_PARSER,
        "--parser=markdown",
        COMMONMARK_REVISION,
        &case.virtual_path.to_string_lossy(),
        &case.input,
    ] {
        bytes.extend_from_slice(value.as_bytes());
        bytes.push(0);
    }
    format!("{:016x}", stable_hash(&bytes))
}

const fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        index += 1;
    }
    hash
}

fn write_cached_oracle(case: &ParityCase, output: &str) {
    let path = oracle_cache_path(case);
    fs::create_dir_all(path.parent().expect("cache file must have a parent"))
        .unwrap_or_else(|error| panic!("Failed to create oracle cache directory: {error}"));
    let value = json!({
        "schema_version": ORACLE_SCHEMA_VERSION,
        "key": oracle_key(case),
        "case": case.name,
        "prettier_version": PRETTIER_VERSION,
        "parser": PRETTIER_PARSER,
        "options": ["--parser", "markdown"],
        "commonmark_revision": COMMONMARK_REVISION,
        "virtual_path": case.virtual_path,
        "input_hash": format!("{:016x}", stable_hash(case.input.as_bytes())),
        "expected": output,
    });
    let encoded = serde_json::to_string_pretty(&value).expect("oracle cache must serialize");
    fs::write(&path, format!("{encoded}\n")).unwrap_or_else(|error| {
        panic!("Failed to write oracle cache '{}': {error}", path.display())
    });
}

fn read_cached_oracle(case: &ParityCase) -> String {
    let path = oracle_cache_path(case);
    let encoded = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "Oracle cache is absent for '{}' at '{}': {error}. Generate it with CLIPPIER_MD_PARITY_ORACLE=refresh",
            case.name,
            path.display()
        )
    });
    let value: Value = serde_json::from_str(&encoded).unwrap_or_else(|error| {
        panic!(
            "Oracle cache is malformed for '{}' at '{}': {error}. Regenerate it with CLIPPIER_MD_PARITY_ORACLE=refresh",
            case.name,
            path.display()
        )
    });
    assert_eq!(
        value.get("schema_version").and_then(Value::as_u64),
        Some(ORACLE_SCHEMA_VERSION),
        "Oracle cache schema is stale for '{}' at '{}'; regenerate it",
        case.name,
        path.display()
    );
    assert_eq!(
        value.get("key").and_then(Value::as_str),
        Some(oracle_key(case).as_str()),
        "Oracle cache key is stale for '{}' at '{}'; regenerate it with CLIPPIER_MD_PARITY_ORACLE=refresh",
        case.name,
        path.display()
    );
    value
        .get("expected")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            panic!(
                "Oracle cache has no string expected output for '{}' at '{}'; regenerate it",
                case.name,
                path.display()
            )
        })
        .to_string()
}

#[derive(Debug)]
struct CaseReport {
    case: ParityCase,
    expected: String,
    actual: String,
    second: String,
    parity: bool,
    deliberate_compatibility_divergence: bool,
    idempotent: bool,
    difference: Option<Difference>,
    mismatch_shape: Option<String>,
}

impl CaseReport {
    fn new(case: ParityCase, expected: String, actual: String, second: String) -> Self {
        let parity = expected == actual;
        let deliberate_compatibility_divergence = match case.kind {
            CaseKind::Commonmark { id: 440 } => {
                expected == "foo _\\__\n" && actual == "foo *\\_*\n"
            }
            CaseKind::Commonmark { id: 451 } => {
                expected == "foo \\_\\_\\_\n" && actual == "foo *\\_*\n"
            }
            CaseKind::Commonmark { .. } | CaseKind::Fixture => false,
        };
        let idempotent = actual == second;
        let difference = (!parity).then(|| first_difference(&expected, &actual));
        let mismatch_shape = (!parity).then(|| classify_mismatch_shape(&expected, &actual));
        Self {
            case,
            expected,
            actual,
            second,
            parity,
            deliberate_compatibility_divergence,
            idempotent,
            difference,
            mismatch_shape,
        }
    }

    fn to_json(&self) -> Value {
        let example_id = match self.case.kind {
            CaseKind::Commonmark { id } => Some(id),
            CaseKind::Fixture => None,
        };
        json!({
            "name": self.case.name,
            "example_id": example_id,
            "section": self.case.section,
            "subsection": self.case.subsection,
            "subsystem": self.case.subsystem,
            "mismatch_shape": self.mismatch_shape,
            "input": self.case.input,
            "expected": self.expected,
            "actual": self.actual,
            "second_pass": self.second,
            "parity": self.parity,
            "deliberate_compatibility_divergence": self.deliberate_compatibility_divergence,
            "idempotent": self.idempotent,
            "difference": self.difference.as_ref().map(Difference::to_json),
        })
    }
}

#[derive(Debug)]
struct Difference {
    byte: usize,
    line: usize,
    column: usize,
    expected_line: Option<String>,
    actual_line: Option<String>,
}

impl Difference {
    fn to_json(&self) -> Value {
        json!({
            "byte": self.byte,
            "line": self.line,
            "column": self.column,
            "expected_line": self.expected_line,
            "actual_line": self.actual_line,
        })
    }
}

fn first_difference(expected: &str, actual: &str) -> Difference {
    let byte = expected
        .bytes()
        .zip(actual.bytes())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| expected.len().min(actual.len()));
    let prefix = &expected[..byte.min(expected.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.chars().count() + 1, |(_, tail)| {
            tail.chars().count() + 1
        });
    let expected_line = expected.lines().nth(line - 1).map(str::to_string);
    let actual_line = actual.lines().nth(line - 1).map(str::to_string);
    Difference {
        byte,
        line,
        column,
        expected_line,
        actual_line,
    }
}

fn classify_mismatch_shape(expected: &str, actual: &str) -> String {
    if expected.trim_end_matches(['\r', '\n']) == actual.trim_end_matches(['\r', '\n']) {
        return "terminal-newline".to_string();
    }
    if collapse_whitespace(expected) == collapse_whitespace(actual) {
        return "line-break-or-whitespace".to_string();
    }
    if expected
        .lines()
        .map(str::trim_start)
        .eq(actual.lines().map(str::trim_start))
    {
        return "indentation".to_string();
    }
    let expected_words = lexical_content(expected);
    let actual_words = lexical_content(actual);
    if expected_words == actual_words {
        return "delimiter-or-escape".to_string();
    }
    if expected_words.contains(&actual_words) {
        return "content-loss".to_string();
    }
    if actual_words.contains(&expected_words) {
        return "content-addition".to_string();
    }
    "content-change".to_string()
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn lexical_content(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn classify_subsystem(section: &str) -> &'static str {
    let section = section.to_ascii_lowercase();
    if section.contains("list") || section.contains("block quote") {
        "block-container"
    } else if section.contains("html") {
        "html"
    } else if section.contains("code span") || section.contains("code block") {
        "code"
    } else if section.contains("link") || section.contains("image") {
        "link-image"
    } else if section.contains("emphasis") {
        "emphasis"
    } else if section.contains("break") || section.contains("paragraph") {
        "line-layout"
    } else if section.contains("tab") || section.contains("space") {
        "whitespace"
    } else if section.contains("escape") || section.contains("character reference") {
        "escaping"
    } else if section.contains("heading") || section.contains("thematic") {
        "block-leaf"
    } else if section.contains("table") {
        "gfm-table"
    } else {
        "other"
    }
}

#[derive(Debug)]
struct AggregateReport {
    reports: Vec<CaseReport>,
    selection: Selection,
    oracle_mode: OracleMode,
    runtime_ms: u128,
}

impl AggregateReport {
    fn new(
        reports: Vec<CaseReport>,
        selection: Selection,
        oracle_mode: OracleMode,
        runtime_ms: u128,
    ) -> Self {
        Self {
            reports,
            selection,
            oracle_mode,
            runtime_ms,
        }
    }

    fn has_failures(&self) -> bool {
        self.reports.iter().any(|report| {
            (!report.parity && !report.deliberate_compatibility_divergence) || !report.idempotent
        })
    }

    fn counts(&self) -> (usize, usize, usize, usize, usize) {
        let total = self.reports.len();
        let passing = self.reports.iter().filter(|report| report.parity).count();
        let parity_failing = total - passing;
        let deliberate_divergences = self
            .reports
            .iter()
            .filter(|report| report.deliberate_compatibility_divergence)
            .count();
        let idempotence_failing = self
            .reports
            .iter()
            .filter(|report| !report.idempotent)
            .count();
        (
            total,
            passing,
            parity_failing,
            deliberate_divergences,
            idempotence_failing,
        )
    }

    fn to_json(&self) -> Value {
        let (total, passing, parity_failing, deliberate_divergences, idempotence_failing) =
            self.counts();
        json!({
            "schema_version": REPORT_SCHEMA_VERSION,
            "oracle": {
                "prettier_version": PRETTIER_VERSION,
                "parser": PRETTIER_PARSER,
                "commonmark_revision": COMMONMARK_REVISION,
                "mode": self.oracle_mode.as_str(),
            },
            "selection": self.selection.to_json(),
            "runtime_ms": self.runtime_ms,
            "counts": {
                "selected": total,
                "parity_passing": passing,
                "parity_failing": parity_failing,
                "deliberate_compatibility_divergences": deliberate_divergences,
                "idempotence_failing": idempotence_failing,
            },
            "cases": self.reports.iter().map(CaseReport::to_json).collect::<Vec<_>>(),
        })
    }

    fn human_summary(&self, report_path: &Path) -> String {
        let (total, passing, parity_failing, deliberate_divergences, idempotence_failing) =
            self.counts();
        let mut output = format!(
            "Strict Prettier parity: {passing}/{total} passing, {parity_failing} divergence(s) ({deliberate_divergences} deliberate), {idempotence_failing} idempotence failure(s); oracle={}; runtime={}ms\nMachine report: {}",
            self.oracle_mode.as_str(),
            self.runtime_ms,
            report_path.display()
        );
        let mut groups = BTreeMap::<String, (usize, usize)>::new();
        for report in &self.reports {
            if (!report.parity && !report.deliberate_compatibility_divergence) || !report.idempotent
            {
                let key = format!(
                    "{} [{}]",
                    if report.case.subsection.is_empty() {
                        &report.case.section
                    } else {
                        &report.case.subsection
                    },
                    report.case.subsystem
                );
                let entry = groups.entry(key).or_default();
                if !report.parity {
                    entry.0 += 1;
                }
                if !report.idempotent {
                    entry.1 += 1;
                }
            }
        }
        if !groups.is_empty() {
            output.push_str("\nFailures by section/subsystem:");
            for (group, (parity, idempotence)) in groups {
                write!(
                    output,
                    "\n- {group}: parity={parity}, idempotence={idempotence}"
                )
                .expect("writing to a String cannot fail");
            }
        }
        if total <= 10 {
            for report in self.reports.iter().filter(|report| {
                (!report.parity && !report.deliberate_compatibility_divergence)
                    || !report.idempotent
            }) {
                write!(output, "\n\n{}", focused_failure(report))
                    .expect("writing to a String cannot fail");
            }
        }
        output
    }
}

fn focused_failure(report: &CaseReport) -> String {
    let difference = report.difference.as_ref().map_or_else(
        || "parity matched".to_string(),
        |difference| {
            format!(
                "first difference at byte {}, line {}, column {}",
                difference.byte, difference.line, difference.column
            )
        },
    );
    format!(
        "{}: parity={}, idempotent={}, {difference}\n--- input ---\n{}\n--- expected ---\n{}\n--- actual ---\n{}",
        report.case.name,
        report.parity,
        report.idempotent,
        report.case.input,
        report.expected,
        report.actual
    )
}

fn report_path() -> PathBuf {
    optional_env("CLIPPIER_MD_PARITY_REPORT").map_or_else(
        || workspace_root().join("target/clippier-md-parity/latest-report.json"),
        |path| {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                workspace_root().join(path)
            }
        },
    )
}

fn write_report(report: &AggregateReport) -> PathBuf {
    let path = report_path();
    fs::create_dir_all(path.parent().expect("report path must have a parent"))
        .unwrap_or_else(|error| panic!("Failed to create report directory: {error}"));
    let encoded = serde_json::to_string_pretty(&report.to_json()).expect("report must serialize");
    fs::write(&path, format!("{encoded}\n"))
        .unwrap_or_else(|error| panic!("Failed to write report '{}': {error}", path.display()));
    path
}

#[derive(Debug)]
struct CommonmarkExample {
    id: usize,
    section: String,
    subsection: String,
    markdown: String,
}

fn parse_commonmark_examples(spec: &str) -> Vec<CommonmarkExample> {
    let lines = spec.split_inclusive('\n').collect::<Vec<_>>();
    let mut examples = Vec::new();
    let mut headings = BTreeMap::<usize, String>::new();
    let mut ordinary_fence: Option<(char, usize)> = None;
    let mut index = 0usize;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_end_matches(['\r', '\n']);

        if ordinary_fence.is_none()
            && let Some(fence) = example_fence(trimmed)
        {
            let mut markdown = String::new();
            index += 1;
            let mut found_separator = false;
            let mut found_close = false;
            while index < lines.len() {
                let body_line = lines[index];
                let body = body_line.trim_end_matches(['\r', '\n']);
                if body == fence {
                    found_close = true;
                    break;
                }
                if !found_separator && body == "." {
                    found_separator = true;
                } else if !found_separator {
                    markdown.push_str(body_line);
                }
                index += 1;
            }
            assert!(
                found_separator,
                "CommonMark example is missing its '.' separator"
            );
            assert!(
                found_close,
                "CommonMark example is missing its closing fence"
            );
            let id = examples.len() + 1;
            examples.push(CommonmarkExample {
                id,
                section: headings.get(&1).cloned().unwrap_or_default(),
                subsection: headings
                    .iter()
                    .filter(|(level, _)| **level > 1)
                    .map(|(_, heading)| heading.as_str())
                    .next_back()
                    .unwrap_or_else(|| headings.get(&1).map_or("Uncategorized", String::as_str))
                    .to_string(),
                markdown,
            });
            index += 1;
            continue;
        }

        if let Some((marker, minimum_length)) = ordinary_fence {
            if is_closing_fence(trimmed, marker, minimum_length) {
                ordinary_fence = None;
            }
            index += 1;
            continue;
        }

        if let Some((marker, length)) = opening_fence(trimmed) {
            ordinary_fence = Some((marker, length));
            index += 1;
            continue;
        }

        if let Some((level, heading)) = parse_heading(trimmed) {
            headings.retain(|existing_level, _| *existing_level < level);
            headings.insert(level, heading);
        }
        index += 1;
    }

    examples
}

fn example_fence(line: &str) -> Option<&str> {
    let fence = line.strip_suffix(" example")?;
    (fence.len() >= 3 && fence.bytes().all(|byte| byte == b'`')).then_some(fence)
}

fn opening_fence(line: &str) -> Option<(char, usize)> {
    let line = line.strip_prefix("   ").unwrap_or(line);
    let marker = line.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let length = line.chars().take_while(|value| *value == marker).count();
    (length >= 3).then_some((marker, length))
}

fn is_closing_fence(line: &str, marker: char, minimum_length: usize) -> bool {
    let line = line.trim_start_matches(' ');
    let length = line.chars().take_while(|value| *value == marker).count();
    length >= minimum_length && line[length..].trim().is_empty()
}

fn parse_heading(line: &str) -> Option<(usize, String)> {
    let line = line.strip_prefix("   ").unwrap_or(line);
    let level = line.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&level) || line.as_bytes().get(level) != Some(&b' ') {
        return None;
    }
    let heading = line[level + 1..]
        .trim()
        .trim_end_matches('#')
        .trim()
        .to_string();
    Some((level, heading))
}

fn collect_fixture_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_fixture_dirs_at_path(root, &mut out);
    out.sort();
    out
}

fn collect_fixture_dirs_at_path(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut has_input = false;
    let mut directories = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            directories.push(path);
        } else if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.starts_with("input."))
        {
            has_input = true;
        }
    }

    if has_input {
        out.push(root.to_path_buf());
    }
    directories.sort();
    for directory in directories {
        collect_fixture_dirs_at_path(&directory, out);
    }
}

fn assert_prettier_version() {
    let runner = prettier_runner();
    static CHECK: OnceLock<()> = OnceLock::new();
    CHECK.get_or_init(|| {
        let output = run_prettier_command(runner, &["--version"], None)
            .expect("Failed to execute prettier version check command");
        assert!(
            output.status.success(),
            "`{}` prettier version check failed with status {:?}: {}",
            runner.display,
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        let version = str::from_utf8(&output.stdout)
            .expect("Prettier version output is not valid UTF-8")
            .trim();
        assert_eq!(
            version, PRETTIER_VERSION,
            "Expected prettier {PRETTIER_VERSION} for parity tests, found {version}"
        );
    });
}

fn run_prettier(input_path: &Path, input: &str) -> String {
    let runner = prettier_runner();
    let path = input_path.to_string_lossy().to_string();
    let config_path = workspace_root().join(".prettierrc.json");
    let config_path = config_path
        .to_str()
        .expect("Prettier config path must be UTF-8");
    let ignore_path =
        workspace_root().join("target/clippier-md-parity/nonexistent-prettier-ignore");
    let ignore_path = ignore_path
        .to_str()
        .expect("Prettier ignore path must be UTF-8");
    let output = run_prettier_command(
        runner,
        &[
            "--config",
            config_path,
            "--ignore-path",
            ignore_path,
            "--parser",
            PRETTIER_PARSER,
            "--stdin-filepath",
            &path,
        ],
        Some(input),
    )
    .expect("Failed to execute prettier process for parity test");
    assert!(
        output.status.success(),
        "Prettier formatting failed for {input_path:?} via {}: {}",
        runner.display,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("Prettier output is not valid UTF-8")
}

#[derive(Debug)]
struct PrettierRunner {
    program: &'static str,
    base_args: Vec<&'static str>,
    display: &'static str,
}

fn prettier_runner() -> &'static PrettierRunner {
    static RUNNER: OnceLock<PrettierRunner> = OnceLock::new();
    RUNNER.get_or_init(resolve_prettier_runner)
}

fn resolve_prettier_runner() -> PrettierRunner {
    let candidates = [
        PrettierRunner {
            program: "bunx",
            base_args: vec!["prettier@3.8.1"],
            display: "bunx prettier@3.8.1",
        },
        PrettierRunner {
            program: "pnpm",
            base_args: vec!["dlx", "prettier@3.8.1"],
            display: "pnpm dlx prettier@3.8.1",
        },
        PrettierRunner {
            program: "npx",
            base_args: vec!["--yes", "prettier@3.8.1"],
            display: "npx --yes prettier@3.8.1",
        },
    ];
    candidates
        .into_iter()
        .find(|runner| command_exists(runner.program))
        .unwrap_or_else(|| {
            panic!(
                "No prettier runner available. Install one of: bunx, pnpm, npx (required for parity tests)."
            )
        })
}

fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn run_prettier_command(
    runner: &PrettierRunner,
    args: &[&str],
    stdin: Option<&str>,
) -> std::io::Result<std::process::Output> {
    let mut command = Command::new(runner.program);
    command
        .args(&runner.base_args)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn()?;
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .expect("Failed to open stdin for prettier command")
            .write_all(input.as_bytes())?;
    }
    child.wait_with_output()
}

fn read_fixture_file(dir: &Path, stem: &str) -> Option<(PathBuf, String)> {
    for extension in ["md", "markdown"] {
        let path = dir.join(format!("{stem}.{extension}"));
        if path.exists() {
            let content = fs::read_to_string(&path).ok()?;
            return Some((path, content));
        }
    }
    None
}

#[test]
fn frontmatter_is_preserved_byte_for_byte() {
    let fixtures_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/parity/fixtures/frontmatter");
    let config = Config {
        engine: FormatterEngine::Ast,
        prose_wrap: ProseWrapMode::Preserve,
        heading_indentation: HeadingIndentationMode::Normalize,
        list_indentation: ListIndentationMode::Preserve,
        ..Config::default()
    };

    for dir in collect_fixture_dirs(&fixtures_root) {
        let input = read_fixture_file(&dir, "input").expect("missing input fixture");
        let output = format_markdown(&input.1, &config);
        let (frontmatter_input, _) = split_frontmatter(&input.1).unwrap_or_else(|| {
            panic!(
                "Frontmatter fixture '{}' has no recognized frontmatter",
                dir.display()
            )
        });
        let (frontmatter_output, _) = split_frontmatter(&output).unwrap_or_else(|| {
            panic!(
                "Formatted frontmatter fixture '{}' has no recognized frontmatter",
                dir.display()
            )
        });
        assert_eq!(
            frontmatter_output,
            frontmatter_input,
            "Frontmatter changed for fixture '{}'",
            dir.display()
        );
    }
}

#[test]
fn non_commonmark_regression_fixtures_are_stable() {
    let fixtures_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/parity/fixtures/regressions");
    let config = Config {
        engine: FormatterEngine::Ast,
        prose_wrap: ProseWrapMode::Preserve,
        heading_indentation: HeadingIndentationMode::Preserve,
        list_indentation: ListIndentationMode::Preserve,
        ..Config::default()
    };

    let fixture_dirs = collect_fixture_dirs(&fixtures_root);
    assert!(!fixture_dirs.is_empty(), "No regression fixtures found");
    for dir in fixture_dirs {
        let input = read_fixture_file(&dir, "input").expect("missing input fixture");
        let output = format_markdown(&input.1, &config);
        let second = format_markdown(&output, &config);
        assert_eq!(
            second,
            output,
            "Regression fixture '{}' should be idempotent",
            dir.display()
        );
    }
}

fn split_frontmatter(input: &str) -> Option<(&str, &str)> {
    let first_newline = input.find('\n')?;
    let first_line = &input[..=first_newline];
    let delimiter = match first_line.trim_end_matches(['\r', '\n']) {
        "---" => "---",
        "+++" => "+++",
        _ => return None,
    };
    let mut offset = first_newline + 1;
    loop {
        let remaining = &input[offset..];
        if remaining.is_empty() {
            return None;
        }
        if let Some(next_newline) = remaining.find('\n') {
            let line_end = offset + next_newline + 1;
            let line = &input[offset..line_end];
            if line.trim_end_matches(['\r', '\n']) == delimiter {
                return Some(input.split_at(line_end));
            }
            offset = line_end;
        } else {
            return (remaining.trim_end_matches(['\r', '\n']) == delimiter)
                .then(|| input.split_at(input.len()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pinned_commonmark_corpus_with_sections_and_exact_count() {
        let spec = fs::read_to_string(commonmark_spec_path()).expect("spec must be available");
        let examples = parse_commonmark_examples(&spec);
        assert_eq!(examples.len(), COMMONMARK_EXAMPLE_COUNT);
        assert_eq!(examples.first().map(|example| example.id), Some(1));
        assert_eq!(
            examples.last().map(|example| example.id),
            Some(COMMONMARK_EXAMPLE_COUNT)
        );
        assert!(examples.iter().all(|example| !example.section.is_empty()));
        assert!(
            examples
                .iter()
                .all(|example| !example.subsection.is_empty())
        );
    }

    #[test]
    fn parser_preserves_empty_input_tabs_and_terminal_newlines() {
        let spec = concat!(
            "# Section\n## Subsection\n",
            "```` example\n.\n<p></p>\n````\n",
            "```` example\na→b\n.\n<p></p>\n````\n",
            "```` example\nno-final-newline\n.\n<p></p>\n````\n",
        );
        let examples = parse_commonmark_examples(spec);
        assert_eq!(examples.len(), 3);
        assert_eq!(examples[0].markdown, "");
        assert_eq!(examples[1].markdown, "a→b\n");
        assert_eq!(examples[1].markdown.replace('→', "\t"), "a\tb\n");
        assert_eq!(examples[2].markdown, "no-final-newline\n");
    }

    #[test]
    fn selection_filters_ids_ranges_sections_fixtures_and_categories() {
        let commonmark = ParityCase {
            name: "commonmark-spec#12".to_string(),
            kind: CaseKind::Commonmark { id: 12 },
            section: "Leaf blocks".to_string(),
            subsection: "ATX headings".to_string(),
            subsystem: "block-leaf".to_string(),
            virtual_path: PathBuf::from("commonmark-spec.md"),
            input: String::new(),
        };
        assert!(
            Selection {
                id: Some(12),
                range: Some((10, 20)),
                section: Some("heading".to_string()),
                category: Some("block".to_string()),
                ..Selection::default()
            }
            .matches(&commonmark)
        );
        assert!(
            !Selection {
                range: Some((13, 20)),
                ..Selection::default()
            }
            .matches(&commonmark)
        );

        let fixture = ParityCase {
            name: "fixture:gfm/table".to_string(),
            kind: CaseKind::Fixture,
            section: "Curated gfm".to_string(),
            subsection: "table".to_string(),
            subsystem: "gfm-table".to_string(),
            virtual_path: PathBuf::from("input.md"),
            input: String::new(),
        };
        assert!(
            Selection {
                fixture: Some("gfm/table".to_string()),
                ..Selection::default()
            }
            .matches(&fixture)
        );

        let report = CaseReport::new(
            commonmark,
            "a\n".to_string(),
            "b\n".to_string(),
            "b\n".to_string(),
        );
        assert!(
            Selection {
                mismatch: Some("parity".to_string()),
                ..Selection::default()
            }
            .matches_report(&report)
        );
        assert!(
            Selection {
                mismatch: report.mismatch_shape.clone(),
                ..Selection::default()
            }
            .matches_report(&report)
        );
    }

    #[test]
    fn pinned_prettier_reproduces_commonmark_example_440_non_idempotence() {
        let path = Path::new("commonmark-spec.md");
        let first = run_prettier(path, "foo *_*\n");
        let second = run_prettier(path, &first);
        assert_eq!(first, "foo _\\__\n");
        assert_eq!(second, "foo \\_\\_\\_\n");
        assert_ne!(second, first);
    }

    #[test]
    fn report_aggregation_accounts_for_parity_and_idempotence_independently() {
        let case = ParityCase {
            name: "commonmark-spec#1".to_string(),
            kind: CaseKind::Commonmark { id: 1 },
            section: "Preliminaries".to_string(),
            subsection: "Tabs".to_string(),
            subsystem: "whitespace".to_string(),
            virtual_path: PathBuf::from("commonmark-spec.md"),
            input: "a\n".to_string(),
        };
        let reports = vec![
            CaseReport::new(
                case.clone(),
                "a\n".to_string(),
                "a\n".to_string(),
                "a\n".to_string(),
            ),
            CaseReport::new(
                case,
                "a\n".to_string(),
                "b\n".to_string(),
                "c\n".to_string(),
            ),
        ];
        let aggregate = AggregateReport::new(reports, Selection::default(), OracleMode::Cache, 123);
        assert_eq!(aggregate.counts(), (2, 1, 1, 0, 1));
        let json = aggregate.to_json();
        assert_eq!(json["counts"]["selected"], 2);
        assert_eq!(json["cases"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn oracle_key_changes_with_case_input() {
        let mut case = ParityCase {
            name: "commonmark-spec#1".to_string(),
            kind: CaseKind::Commonmark { id: 1 },
            section: String::new(),
            subsection: String::new(),
            subsystem: String::new(),
            virtual_path: PathBuf::from("commonmark-spec.md"),
            input: "a\n".to_string(),
        };
        let first = oracle_key(&case);
        case.input = "b\n".to_string();
        assert_ne!(oracle_key(&case), first);
    }
}
