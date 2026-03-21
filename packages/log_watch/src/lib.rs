//! Shared filtering, profile, and time utilities for log watching.
//!
//! This crate exposes reusable logic used by the log watch command, including:
//! * Regex-based include/exclude filtering
//! * Relative `--since` parsing
//! * Optional profile persistence helpers

use anyhow::{Context, Result};
use regex::{Regex, RegexBuilder};
use std::path::{Path, PathBuf};
use time::{Duration as TimeDuration, OffsetDateTime, format_description::well_known::Rfc3339};

/// Include/exclude mode for a log filter rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "persistence-json",
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(feature = "persistence-json", serde(rename_all = "snake_case"))]
pub enum LogFilterKind {
    /// Keep lines that match this rule.
    Include,
    /// Remove lines that match this rule.
    Exclude,
}

/// Case sensitivity behavior for a log filter rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "persistence-json",
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(feature = "persistence-json", serde(rename_all = "snake_case"))]
pub enum LogFilterCaseMode {
    /// Match using case-sensitive regex behavior.
    Sensitive,
    /// Match using case-insensitive regex behavior.
    Insensitive,
}

/// A single include/exclude regex rule used by log watching.
#[derive(Debug, Clone)]
pub struct LogFilterRule {
    /// Whether this is an include or exclude rule.
    pub kind: LogFilterKind,
    /// The raw regex pattern entered by the user.
    pub pattern: String,
    /// How case matching is applied to [`Self::pattern`].
    pub case_mode: LogFilterCaseMode,
    /// Whether this rule currently participates in filtering.
    pub enabled: bool,
    regex: std::result::Result<Regex, String>,
}

impl LogFilterRule {
    /// Creates a filter rule and precompiles its regex pattern.
    #[must_use]
    pub fn new(kind: LogFilterKind, pattern: String, case_mode: LogFilterCaseMode) -> Self {
        let regex = compile_filter_regex(&pattern, case_mode);
        Self {
            kind,
            pattern,
            case_mode,
            enabled: true,
            regex,
        }
    }

    /// Returns whether the rule currently contains a regex compilation error.
    #[must_use]
    pub fn has_error(&self) -> bool {
        self.regex.is_err()
    }

    /// Toggles case sensitivity and recompiles the regex pattern.
    pub fn toggle_case_mode(&mut self) {
        self.case_mode = match self.case_mode {
            LogFilterCaseMode::Sensitive => LogFilterCaseMode::Insensitive,
            LogFilterCaseMode::Insensitive => LogFilterCaseMode::Sensitive,
        };
        self.regex = compile_filter_regex(&self.pattern, self.case_mode);
    }

    /// Returns whether this enabled rule matches the provided log line.
    #[must_use]
    pub fn matches(&self, line: &str) -> bool {
        if !self.enabled {
            return false;
        }
        self.regex.as_ref().is_ok_and(|regex| regex.is_match(line))
    }
}

/// Serializable representation of a filter rule used in persisted watch state.
#[derive(Debug, Clone)]
#[cfg_attr(
    feature = "persistence-json",
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct WatchFilterState {
    /// Whether this is an include or exclude rule.
    pub kind: LogFilterKind,
    /// Raw regex pattern.
    pub pattern: String,
    /// Case matching mode.
    pub case_mode: LogFilterCaseMode,
    /// Whether this persisted rule is enabled.
    pub enabled: bool,
}

/// Runtime configuration for the interactive log watch UI.
#[derive(Debug, Clone)]
pub struct WatchRunConfig {
    /// Panel title shown in the UI.
    pub title: String,
    /// Directory containing rolling log files.
    pub log_dir: PathBuf,
    /// Prefix used to select candidate log files.
    pub log_file_prefix: String,
    /// Optional startup line limit for initial history.
    pub lines: Option<usize>,
    /// Optional relative time filter (for example, `10m`).
    pub since: Option<String>,
    /// Optional profile name used for persistence.
    pub profile: Option<String>,
    /// Case-sensitive include regex seeds.
    pub include: Vec<String>,
    /// Case-insensitive include regex seeds.
    pub include_i: Vec<String>,
    /// Case-sensitive exclude regex seeds.
    pub exclude: Vec<String>,
    /// Case-insensitive exclude regex seeds.
    pub exclude_i: Vec<String>,
    /// Optional JSON state file path.
    pub state_file: Option<PathBuf>,
}

/// Lightweight profile information returned by profile listing.
#[derive(Debug, Clone)]
pub struct ProfileSummary {
    /// Profile name.
    pub name: String,
    /// Whether this is the active profile.
    pub active: bool,
    /// Number of saved filters in this profile.
    pub filter_count: usize,
}

/// Full profile details returned by profile inspection.
#[derive(Debug, Clone)]
pub struct ProfileDetails {
    /// Profile name.
    pub name: String,
    /// Whether this is the active profile.
    pub active: bool,
    /// Optional quick substring filter.
    pub quick_filter: Option<String>,
    /// Optional `--since` value.
    pub since: Option<String>,
    /// Optional startup line count.
    pub lines: Option<usize>,
    /// Selected filter index in the UI.
    pub selected_filter_index: Option<usize>,
    /// Persisted include/exclude filter rules.
    pub filters: Vec<WatchFilterState>,
}

/// Returns the newest file in `log_dir` whose file name starts with `file_prefix`.
///
/// If no matching file is found, this returns `log_dir.join(file_prefix)`.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// let path = moosicbox_log_watch::active_log_file_path(Path::new("/var/log"), "app");
/// assert!(path.starts_with("/var/log"));
/// ```
#[must_use]
pub fn active_log_file_path(log_dir: &Path, file_prefix: &str) -> PathBuf {
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;

    if let Ok(entries) = std::fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !file_name.starts_with(file_prefix) {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            match &newest {
                Some((latest_modified, _)) if modified <= *latest_modified => {}
                _ => newest = Some((modified, path)),
            }
        }
    }

    newest
        .map(|(_, path)| path)
        .unwrap_or_else(|| log_dir.join(file_prefix))
}

/// Parses a relative `--since` value and returns an absolute UTC cutoff time.
///
/// # Errors
///
/// * Returns an error when `raw` is empty, malformed, negative, or uses an unsupported unit.
pub fn parse_since_cutoff(raw: &str) -> Result<OffsetDateTime> {
    let duration = parse_since_duration(raw)?;
    let now = OffsetDateTime::now_utc();
    Ok(now - duration)
}

/// Parses a relative duration like `30s`, `10m`, `2h`, or `1d`.
///
/// # Errors
///
/// * Returns an error when `raw` is empty.
/// * Returns an error when `raw` does not start with an integer value.
/// * Returns an error when the parsed value is negative.
/// * Returns an error when the unit is not one of `s`, `m`, `h`, `d`.
///
/// # Examples
///
/// ```
/// use time::Duration;
///
/// let duration = moosicbox_log_watch::parse_since_duration("10m").unwrap();
/// assert_eq!(duration, Duration::minutes(10));
/// ```
pub fn parse_since_duration(raw: &str) -> Result<TimeDuration> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("--since must be a non-empty duration like 30s, 10m, 2h, or 1d");
    }

    let split_at = trimmed
        .find(|char: char| !char.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let (value_part, unit_part) = trimmed.split_at(split_at);
    if value_part.is_empty() {
        anyhow::bail!("--since must start with a number");
    }

    let amount = value_part
        .parse::<i64>()
        .with_context(|| format!("invalid --since value '{raw}'"))?;
    if amount < 0 {
        anyhow::bail!("--since must be non-negative");
    }

    let duration = match unit_part {
        "" | "s" => TimeDuration::seconds(amount),
        "m" => TimeDuration::minutes(amount),
        "h" => TimeDuration::hours(amount),
        "d" => TimeDuration::days(amount),
        _ => {
            anyhow::bail!(
                "invalid --since unit '{unit_part}' (use s, m, h, d; example: 30s, 10m, 2h, 1d)"
            )
        }
    };
    Ok(duration)
}

/// Returns whether a log line is on or after the optional timestamp cutoff.
///
/// The function expects the line to start with an RFC 3339 timestamp.
#[must_use]
pub fn line_matches_since(line: &str, cutoff: Option<OffsetDateTime>) -> bool {
    let Some(cutoff) = cutoff else {
        return true;
    };
    let Some(timestamp) = line.split_whitespace().next() else {
        return false;
    };
    let Ok(parsed) = OffsetDateTime::parse(timestamp, &Rfc3339) else {
        return false;
    };
    parsed >= cutoff
}

/// Compiles a regex pattern for a filter rule.
///
/// # Errors
///
/// * Returns the regex parser error string when `pattern` is invalid.
pub fn compile_filter_regex(
    pattern: &str,
    case_mode: LogFilterCaseMode,
) -> std::result::Result<Regex, String> {
    let mut builder = RegexBuilder::new(pattern);
    builder.unicode(false);
    if matches!(case_mode, LogFilterCaseMode::Insensitive) {
        builder.case_insensitive(true);
    }
    builder.build().map_err(|error| error.to_string())
}

/// Returns whether a line should be visible after quick filter and rule filtering.
///
/// # Examples
///
/// ```
/// use moosicbox_log_watch::{
///     line_visible_in_watch, LogFilterCaseMode, LogFilterKind, LogFilterRule,
/// };
///
/// let filters = [LogFilterRule::new(
///     LogFilterKind::Include,
///     "error".to_string(),
///     LogFilterCaseMode::Insensitive,
/// )];
///
/// assert!(line_visible_in_watch("ERROR: failed", &filters, None));
/// assert!(!line_visible_in_watch("INFO: ok", &filters, None));
/// ```
#[must_use]
pub fn line_visible_in_watch(
    line: &str,
    filters: &[LogFilterRule],
    quick_filter: Option<&str>,
) -> bool {
    if let Some(quick) = quick_filter
        && !quick.is_empty()
        && !line.contains(quick)
    {
        return false;
    }

    let include_filters = filters
        .iter()
        .filter(|rule| rule.enabled && matches!(rule.kind, LogFilterKind::Include))
        .collect::<Vec<_>>();
    if !include_filters.is_empty() && !include_filters.iter().any(|rule| rule.matches(line)) {
        return false;
    }

    !filters.iter().any(|rule| {
        rule.enabled && matches!(rule.kind, LogFilterKind::Exclude) && rule.matches(line)
    })
}

/// Converts a runtime filter rule to a serializable persisted state value.
#[must_use]
pub fn watch_filter_rule_to_state(rule: &LogFilterRule) -> WatchFilterState {
    WatchFilterState {
        kind: rule.kind,
        pattern: rule.pattern.clone(),
        case_mode: rule.case_mode,
        enabled: rule.enabled,
    }
}

/// Converts a persisted filter state value to a runtime filter rule.
#[must_use]
pub fn watch_filter_state_to_rule(state: WatchFilterState) -> LogFilterRule {
    let mut rule = LogFilterRule::new(state.kind, state.pattern, state.case_mode);
    rule.enabled = state.enabled;
    rule
}

/// Normalizes and validates an optional profile name.
///
/// Uses `default` when `profile` is `None`.
///
/// # Errors
///
/// * Returns an error when the resulting profile name is empty.
/// * Returns an error when the resulting profile name is longer than 64 characters.
/// * Returns an error when the resulting profile name contains non-ASCII alphanumeric characters other than `-` and `_`.
///
/// # Examples
///
/// ```
/// assert_eq!(moosicbox_log_watch::normalize_profile_name(None).unwrap(), "default");
/// assert!(moosicbox_log_watch::normalize_profile_name(Some("bad name")).is_err());
/// ```
pub fn normalize_profile_name(profile: Option<&str>) -> Result<String> {
    let value = profile.unwrap_or("default").trim();
    if value.is_empty() {
        anyhow::bail!("--profile cannot be empty");
    }
    if value.len() > 64 {
        anyhow::bail!("--profile must be 64 characters or fewer");
    }
    if !value
        .chars()
        .all(|entry| entry.is_ascii_alphanumeric() || entry == '-' || entry == '_')
    {
        anyhow::bail!("--profile may contain only ASCII letters, numbers, '-', and '_'");
    }
    Ok(value.to_string())
}

#[cfg(feature = "persistence-json")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct WatchStateFile {
    version: u32,
    active_profile: Option<String>,
    #[serde(default)]
    profiles: std::collections::BTreeMap<String, WatchProfileState>,
}

#[cfg(feature = "persistence-json")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct WatchProfileState {
    #[serde(default)]
    filters: Vec<WatchFilterState>,
    quick_filter: Option<String>,
    since: Option<String>,
    lines: Option<usize>,
    selected_filter_index: Option<usize>,
}

#[cfg(feature = "persistence-json")]
fn read_state_file(state_file: &Path) -> Result<WatchStateFile> {
    if !state_file.exists() {
        return Ok(WatchStateFile {
            version: 1,
            ..WatchStateFile::default()
        });
    }
    let content = std::fs::read_to_string(state_file)
        .with_context(|| format!("failed reading watch state file {}", state_file.display()))?;
    let mut state: WatchStateFile = serde_json::from_str(&content)
        .with_context(|| format!("failed parsing watch state file {}", state_file.display()))?;
    if state.version == 0 {
        state.version = 1;
    }
    Ok(state)
}

#[cfg(feature = "persistence-json")]
fn write_state_file(state_file: &Path, state: &WatchStateFile) -> Result<()> {
    if let Some(parent) = state_file.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed creating state directory {}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(state).context("failed encoding watch state")?;
    let temp_path = state_file.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temp_path, bytes).with_context(|| {
        format!(
            "failed writing temporary state file {}",
            temp_path.display()
        )
    })?;
    std::fs::rename(&temp_path, state_file)
        .with_context(|| format!("failed finalizing state file {}", state_file.display()))?;
    Ok(())
}

#[cfg(feature = "persistence-json")]
/// Lists saved profiles from the JSON state file.
///
/// The returned list always includes `default`.
///
/// # Errors
///
/// * Returns an error when the state file cannot be read.
/// * Returns an error when the state file cannot be parsed as JSON.
pub fn profiles_list(state_file: &Path) -> Result<Vec<ProfileSummary>> {
    let state = read_state_file(state_file)?;
    let active_profile = state.active_profile.as_deref().unwrap_or("default");
    let mut names = state.profiles.keys().cloned().collect::<Vec<_>>();
    if !names.iter().any(|name| name == "default") {
        names.push("default".to_string());
    }
    names.sort();
    Ok(names
        .into_iter()
        .map(|name| ProfileSummary {
            filter_count: state
                .profiles
                .get(&name)
                .map_or(0, |profile| profile.filters.len()),
            active: name == active_profile,
            name,
        })
        .collect())
}

#[cfg(feature = "persistence-json")]
/// Returns full details for a specific profile.
///
/// Missing profiles resolve to an empty default profile payload.
///
/// # Errors
///
/// * Returns an error when `profile` fails validation.
/// * Returns an error when the state file cannot be read.
/// * Returns an error when the state file cannot be parsed as JSON.
pub fn profile_show(state_file: &Path, profile: Option<&str>) -> Result<ProfileDetails> {
    let profile_name = normalize_profile_name(profile)?;
    let state = read_state_file(state_file)?;
    let profile_state = state
        .profiles
        .get(&profile_name)
        .cloned()
        .unwrap_or_default();
    Ok(ProfileDetails {
        name: profile_name.clone(),
        active: state.active_profile.as_deref() == Some(profile_name.as_str()),
        quick_filter: profile_state.quick_filter,
        since: profile_state.since,
        lines: profile_state.lines,
        selected_filter_index: profile_state.selected_filter_index,
        filters: profile_state.filters,
    })
}

#[cfg(feature = "persistence-json")]
/// Deletes a saved profile from the JSON state file.
///
/// # Errors
///
/// * Returns an error when `profile` fails validation.
/// * Returns an error when attempting to delete the reserved `default` profile.
/// * Returns an error when the profile is not found.
/// * Returns an error when the state file cannot be read, parsed, or written.
pub fn profile_delete(state_file: &Path, profile: &str) -> Result<()> {
    let profile_name = normalize_profile_name(Some(profile))?;
    if profile_name == "default" {
        anyhow::bail!("cannot delete reserved profile 'default'");
    }
    let mut state = read_state_file(state_file)?;
    if state.profiles.remove(&profile_name).is_none() {
        anyhow::bail!("profile '{profile_name}' not found");
    }
    if state.active_profile.as_deref() == Some(profile_name.as_str()) {
        state.active_profile = Some("default".to_string());
    }
    write_state_file(state_file, &state)
}

#[cfg(feature = "persistence-json")]
/// Renames a saved profile in the JSON state file.
///
/// # Errors
///
/// * Returns an error when either profile name fails validation.
/// * Returns an error when source and destination names are identical.
/// * Returns an error when destination profile already exists.
/// * Returns an error when source profile is not found.
/// * Returns an error when the state file cannot be read, parsed, or written.
pub fn profile_rename(state_file: &Path, from: &str, to: &str) -> Result<()> {
    let from_name = normalize_profile_name(Some(from))?;
    let to_name = normalize_profile_name(Some(to))?;
    if from_name == to_name {
        anyhow::bail!("source and destination profile names are the same");
    }
    let mut state = read_state_file(state_file)?;
    if state.profiles.contains_key(&to_name) {
        anyhow::bail!("profile '{to_name}' already exists");
    }
    let profile = state
        .profiles
        .remove(&from_name)
        .ok_or_else(|| anyhow::anyhow!("profile '{from_name}' not found"))?;
    state.profiles.insert(to_name.clone(), profile);
    if state.active_profile.as_deref() == Some(from_name.as_str()) {
        state.active_profile = Some(to_name);
    }
    write_state_file(state_file, &state)
}

#[cfg(feature = "tui")]
mod tui {
    //! Terminal user interface for live log watching.

    use super::*;
    use crossterm::cursor::{Hide, Show};
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use crossterm::execute;
    use crossterm::terminal;
    use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
    use ratatui::Terminal;
    use ratatui::backend::CrosstermBackend;
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
    use std::collections::VecDeque;
    use std::io::{Read, Seek, Write};
    use std::time::Duration;

    const WATCH_BUFFER_LIMIT: usize = 20_000;
    const WATCH_STATUS_HEIGHT: u16 = 4;
    const WATCH_FILTER_HEIGHT: u16 = 5;
    const WATCH_INFO_HEIGHT: u16 = 3;

    #[derive(Debug, Clone, Copy, Default)]
    struct AnsiStyleState {
        fg: Option<Color>,
        bg: Option<Color>,
        bold: bool,
        underline: bool,
    }

    impl AnsiStyleState {
        fn to_style(self) -> Style {
            let mut style = Style::default();
            if let Some(fg) = self.fg {
                style = style.fg(fg);
            }
            if let Some(bg) = self.bg {
                style = style.bg(bg);
            }
            if self.bold {
                style = style.add_modifier(Modifier::BOLD);
            }
            if self.underline {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            style
        }

        fn apply_sgr_codes(&mut self, codes: &[u16]) {
            if codes.is_empty() {
                *self = Self::default();
                return;
            }

            let mut index = 0;
            while index < codes.len() {
                match codes[index] {
                    0 => *self = Self::default(),
                    1 => self.bold = true,
                    4 => self.underline = true,
                    22 => self.bold = false,
                    24 => self.underline = false,
                    30..=37 => self.fg = Some(basic_ansi_color(codes[index] - 30, false)),
                    90..=97 => self.fg = Some(basic_ansi_color(codes[index] - 90, true)),
                    39 => self.fg = None,
                    40..=47 => self.bg = Some(basic_ansi_color(codes[index] - 40, false)),
                    100..=107 => self.bg = Some(basic_ansi_color(codes[index] - 100, true)),
                    49 => self.bg = None,
                    38 => {
                        if let Some((color, consumed)) =
                            parse_extended_ansi_color(&codes[index + 1..])
                        {
                            self.fg = Some(color);
                            index += consumed;
                        }
                    }
                    48 => {
                        if let Some((color, consumed)) =
                            parse_extended_ansi_color(&codes[index + 1..])
                        {
                            self.bg = Some(color);
                            index += consumed;
                        }
                    }
                    _ => {}
                }
                index += 1;
            }
        }
    }

    fn basic_ansi_color(code: u16, bright: bool) -> Color {
        match (code, bright) {
            (0, false) => Color::Black,
            (1, false) => Color::Red,
            (2, false) => Color::Green,
            (3, false) => Color::Yellow,
            (4, false) => Color::Blue,
            (5, false) => Color::Magenta,
            (6, false) => Color::Cyan,
            (7, false) => Color::Gray,
            (0, true) => Color::DarkGray,
            (1, true) => Color::LightRed,
            (2, true) => Color::LightGreen,
            (3, true) => Color::LightYellow,
            (4, true) => Color::LightBlue,
            (5, true) => Color::LightMagenta,
            (6, true) => Color::LightCyan,
            (7, true) => Color::White,
            _ => Color::Reset,
        }
    }

    fn parse_extended_ansi_color(codes: &[u16]) -> Option<(Color, usize)> {
        if codes.len() >= 2 && codes[0] == 5 {
            let index = u8::try_from(codes[1]).ok()?;
            return Some((Color::Indexed(index), 2));
        }
        if codes.len() >= 4 && codes[0] == 2 {
            let red = u8::try_from(codes[1]).ok()?;
            let green = u8::try_from(codes[2]).ok()?;
            let blue = u8::try_from(codes[3]).ok()?;
            return Some((Color::Rgb(red, green, blue), 4));
        }
        None
    }

    fn parse_sgr_codes(param_block: &str) -> Vec<u16> {
        if param_block.is_empty() {
            return vec![0];
        }

        param_block
            .split(';')
            .filter_map(|entry| {
                if entry.is_empty() {
                    Some(0)
                } else {
                    entry.parse::<u16>().ok()
                }
            })
            .collect()
    }

    fn decode_escaped_ansi(line: &str) -> String {
        line.replace("\\x1b", "\u{001b}")
            .replace("\\x1B", "\u{001b}")
            .replace("\\u001b", "\u{001b}")
            .replace("\\u001B", "\u{001b}")
    }

    fn ansi_to_line(line: &str) -> Line<'static> {
        let decoded = decode_escaped_ansi(line);
        let bytes = decoded.as_bytes();
        let mut spans = Vec::new();
        let mut style_state = AnsiStyleState::default();
        let mut segment_start = 0usize;
        let mut index = 0usize;

        while index < bytes.len() {
            if bytes[index] != 0x1b {
                index += 1;
                continue;
            }

            if segment_start < index {
                spans.push(Span::styled(
                    decoded[segment_start..index].to_string(),
                    style_state.to_style(),
                ));
            }

            if index + 1 >= bytes.len() || bytes[index + 1] != b'[' {
                index += 1;
                continue;
            }

            let mut final_index = index + 2;
            while final_index < bytes.len() && !(0x40..=0x7e).contains(&bytes[final_index]) {
                final_index += 1;
            }

            if final_index >= bytes.len() {
                break;
            }

            if bytes[final_index] == b'm' {
                let params = &decoded[index + 2..final_index];
                let codes = parse_sgr_codes(params);
                style_state.apply_sgr_codes(&codes);
            }

            index = final_index + 1;
            segment_start = index;
        }

        if segment_start < decoded.len() {
            spans.push(Span::styled(
                decoded[segment_start..].to_string(),
                style_state.to_style(),
            ));
        }

        if spans.is_empty() {
            spans.push(Span::raw(String::new()));
        }

        Line::from(spans)
    }

    struct UiGuard;

    impl UiGuard {
        fn activate() -> Result<Self> {
            enable_raw_mode().context("failed enabling raw mode for watch")?;
            let mut stdout = std::io::stdout();
            execute!(stdout, EnterAlternateScreen, Hide)
                .context("failed entering alternate screen")?;
            Ok(Self)
        }
    }

    impl Drop for UiGuard {
        fn drop(&mut self) {
            let mut stdout = std::io::stdout();
            let _ = execute!(stdout, Show, LeaveAlternateScreen);
            let _ = disable_raw_mode();
        }
    }

    /// Runs the interactive terminal log watch UI.
    ///
    /// # Errors
    ///
    /// * Returns an error when profile configuration is invalid.
    /// * Returns an error when log files cannot be opened, read, or tailed.
    /// * Returns an error when terminal raw mode or alternate screen setup fails.
    /// * Returns an error when rendering or keyboard event handling fails.
    /// * Returns an error when persistence is enabled and state cannot be read or written.
    pub fn run_watch(config: WatchRunConfig) -> Result<()> {
        let profile_name = normalize_profile_name(config.profile.as_deref())?;

        #[cfg(feature = "persistence-json")]
        let mut persisted_profile = match config.state_file.as_deref() {
            Some(state_file) => profile_show(state_file, Some(&profile_name))?,
            None => ProfileDetails {
                name: profile_name.clone(),
                active: true,
                quick_filter: None,
                since: None,
                lines: None,
                selected_filter_index: None,
                filters: Vec::new(),
            },
        };

        #[cfg(not(feature = "persistence-json"))]
        let mut persisted_profile = ProfileDetails {
            name: profile_name.clone(),
            active: true,
            quick_filter: None,
            since: None,
            lines: None,
            selected_filter_index: None,
            filters: Vec::new(),
        };

        let effective_lines = config.lines.or(persisted_profile.lines).unwrap_or(200);
        let effective_since = config
            .since
            .clone()
            .or_else(|| persisted_profile.since.clone());
        let since_cutoff = match effective_since.as_deref() {
            Some(value) => Some(parse_since_cutoff(value)?),
            None => None,
        };

        let mut filters = persisted_profile
            .filters
            .iter()
            .cloned()
            .map(watch_filter_state_to_rule)
            .collect::<Vec<_>>();
        filters.extend(seed_filters(
            &config.include,
            &config.include_i,
            &config.exclude,
            &config.exclude_i,
        ));

        let mut selected_filter = persisted_profile.selected_filter_index.unwrap_or_default();
        if selected_filter >= filters.len() {
            selected_filter = filters.len().saturating_sub(1);
        }
        let mut status_message = String::new();
        let mut quick_filter = persisted_profile.quick_filter.take();
        let mut paused = false;
        let mut auto_follow = true;
        let mut log_cursor = usize::MAX;

        let mut active_path = active_log_file_path(&config.log_dir, &config.log_file_prefix);
        let mut entries = VecDeque::new();
        let mut pending_fragment = String::new();
        let mut read_offset = 0_u64;

        if active_path.exists() {
            preload_entries(
                &active_path,
                &mut entries,
                effective_lines,
                since_cutoff,
                WATCH_BUFFER_LIMIT,
            )?;
        }

        let mut log_file = open_log_file(&active_path)?;
        if let Some(file) = log_file.as_mut() {
            read_offset = file
                .metadata()
                .with_context(|| format!("failed reading metadata for {}", active_path.display()))?
                .len();
        }

        let _guard = UiGuard::activate()?;
        let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))
            .context("failed initializing TUI")?;

        let save_state = |filters: &[LogFilterRule],
                          quick_filter: Option<&str>,
                          selected_filter: usize|
         -> Result<()> {
            #[cfg(feature = "persistence-json")]
            {
                if let Some(state_file) = config.state_file.as_deref() {
                    save_profile_state(
                        state_file,
                        &profile_name,
                        filters,
                        quick_filter,
                        effective_since.as_deref(),
                        effective_lines,
                        selected_filter,
                    )?;
                }
            }
            Ok(())
        };

        loop {
            let newest_path = active_log_file_path(&config.log_dir, &config.log_file_prefix);
            if newest_path != active_path {
                active_path = newest_path;
                log_file = open_log_file(&active_path)?;
                read_offset = 0;
                pending_fragment.clear();
                status_message = format!("switched to {}", active_path.display());
            }

            if !paused {
                if log_file.is_none() {
                    log_file = open_log_file(&active_path)?;
                    if log_file.is_some() {
                        status_message = format!("opened {}", active_path.display());
                    }
                }

                if let Some(file) = log_file.as_mut()
                    && let Some(new_lines) =
                        read_log_delta(file, &active_path, &mut read_offset, &mut pending_fragment)?
                {
                    for line in new_lines {
                        if line_matches_since(&line, since_cutoff) {
                            entries.push_back(line);
                        }
                    }
                    while entries.len() > WATCH_BUFFER_LIMIT {
                        let _ = entries.pop_front();
                    }
                    if auto_follow {
                        log_cursor = usize::MAX;
                    }
                }
            }

            render(
                &mut terminal,
                &config.title,
                &active_path,
                &entries,
                &filters,
                selected_filter,
                quick_filter.as_deref(),
                paused,
                &profile_name,
                log_cursor,
                auto_follow,
                &status_message,
            )?;

            let visible_count = entries
                .iter()
                .filter(|line| line_visible_in_watch(line, &filters, quick_filter.as_deref()))
                .count();
            if visible_count == 0 {
                log_cursor = 0;
            } else {
                if auto_follow || log_cursor == usize::MAX {
                    log_cursor = visible_count.saturating_sub(1);
                }
                log_cursor = log_cursor.min(visible_count.saturating_sub(1));
            }

            if !event::poll(Duration::from_millis(120)).context("failed polling watch input")? {
                continue;
            }
            let Event::Key(key) = event::read().context("failed reading watch input")? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }

            let viewport = viewport_height();
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('p') => {
                    paused = !paused;
                    status_message = if paused {
                        "paused ingest".to_string()
                    } else {
                        "resumed ingest".to_string()
                    };
                }
                KeyCode::Char('a') => {
                    if let Some(pattern) = prompt_line("Add include regex: ")? {
                        filters.push(LogFilterRule::new(
                            LogFilterKind::Include,
                            pattern,
                            LogFilterCaseMode::Sensitive,
                        ));
                        selected_filter = filters.len().saturating_sub(1);
                        if let Err(error) =
                            save_state(&filters, quick_filter.as_deref(), selected_filter)
                        {
                            status_message = format!("failed saving watch state: {error:#}");
                        }
                    }
                }
                KeyCode::Char('x') => {
                    if let Some(pattern) = prompt_line("Add exclude regex: ")? {
                        filters.push(LogFilterRule::new(
                            LogFilterKind::Exclude,
                            pattern,
                            LogFilterCaseMode::Sensitive,
                        ));
                        selected_filter = filters.len().saturating_sub(1);
                        if let Err(error) =
                            save_state(&filters, quick_filter.as_deref(), selected_filter)
                        {
                            status_message = format!("failed saving watch state: {error:#}");
                        }
                    }
                }
                KeyCode::Char('/') => {
                    quick_filter = prompt_line("Quick substring filter (empty clears): ")?;
                    if let Err(error) =
                        save_state(&filters, quick_filter.as_deref(), selected_filter)
                    {
                        status_message = format!("failed saving watch state: {error:#}");
                    }
                }
                KeyCode::Char('c') => {
                    filters.clear();
                    selected_filter = 0;
                    quick_filter = None;
                    status_message = "cleared filters".to_string();
                    if let Err(error) =
                        save_state(&filters, quick_filter.as_deref(), selected_filter)
                    {
                        status_message = format!("failed saving watch state: {error:#}");
                    }
                }
                KeyCode::Char('t') => {
                    if let Some(filter) = filters.get_mut(selected_filter) {
                        filter.enabled = !filter.enabled;
                        if let Err(error) =
                            save_state(&filters, quick_filter.as_deref(), selected_filter)
                        {
                            status_message = format!("failed saving watch state: {error:#}");
                        }
                    }
                }
                KeyCode::Char('i') => {
                    if let Some(filter) = filters.get_mut(selected_filter) {
                        filter.toggle_case_mode();
                        if let Err(error) =
                            save_state(&filters, quick_filter.as_deref(), selected_filter)
                        {
                            status_message = format!("failed saving watch state: {error:#}");
                        }
                    }
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if visible_count > 0 {
                        auto_follow = false;
                        log_cursor = log_cursor
                            .saturating_add((viewport / 2).max(1))
                            .min(visible_count.saturating_sub(1));
                    }
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    auto_follow = false;
                    log_cursor = log_cursor.saturating_sub((viewport / 2).max(1));
                }
                KeyCode::Char('d') => {
                    if selected_filter < filters.len() {
                        let _ = filters.remove(selected_filter);
                        if selected_filter >= filters.len() {
                            selected_filter = filters.len().saturating_sub(1);
                        }
                        if let Err(error) =
                            save_state(&filters, quick_filter.as_deref(), selected_filter)
                        {
                            status_message = format!("failed saving watch state: {error:#}");
                        }
                    }
                }
                KeyCode::Char('j') => {
                    if visible_count > 0 {
                        auto_follow = false;
                        log_cursor = log_cursor
                            .saturating_add(1)
                            .min(visible_count.saturating_sub(1));
                    }
                }
                KeyCode::Char('k') => {
                    auto_follow = false;
                    log_cursor = log_cursor.saturating_sub(1);
                }
                KeyCode::Char('g') => {
                    auto_follow = false;
                    log_cursor = 0;
                }
                KeyCode::Char('G') => {
                    auto_follow = true;
                    if visible_count > 0 {
                        log_cursor = visible_count.saturating_sub(1);
                    }
                }
                KeyCode::PageDown => {
                    if visible_count > 0 {
                        auto_follow = false;
                        log_cursor = log_cursor
                            .saturating_add(viewport.max(1))
                            .min(visible_count.saturating_sub(1));
                    }
                }
                KeyCode::PageUp => {
                    auto_follow = false;
                    log_cursor = log_cursor.saturating_sub(viewport.max(1));
                }
                KeyCode::Up => {
                    selected_filter = selected_filter.saturating_sub(1);
                }
                KeyCode::Down => {
                    if selected_filter + 1 < filters.len() {
                        selected_filter += 1;
                    }
                }
                _ => {}
            }
        }

        save_state(&filters, quick_filter.as_deref(), selected_filter)?;
        Ok(())
    }

    fn seed_filters(
        include: &[String],
        include_i: &[String],
        exclude: &[String],
        exclude_i: &[String],
    ) -> Vec<LogFilterRule> {
        let mut filters = Vec::new();
        filters.extend(include.iter().cloned().map(|pattern| {
            LogFilterRule::new(
                LogFilterKind::Include,
                pattern,
                LogFilterCaseMode::Sensitive,
            )
        }));
        filters.extend(include_i.iter().cloned().map(|pattern| {
            LogFilterRule::new(
                LogFilterKind::Include,
                pattern,
                LogFilterCaseMode::Insensitive,
            )
        }));
        filters.extend(exclude.iter().cloned().map(|pattern| {
            LogFilterRule::new(
                LogFilterKind::Exclude,
                pattern,
                LogFilterCaseMode::Sensitive,
            )
        }));
        filters.extend(exclude_i.iter().cloned().map(|pattern| {
            LogFilterRule::new(
                LogFilterKind::Exclude,
                pattern,
                LogFilterCaseMode::Insensitive,
            )
        }));
        filters
    }

    #[cfg(feature = "persistence-json")]
    fn save_profile_state(
        state_file: &Path,
        profile_name: &str,
        filters: &[LogFilterRule],
        quick_filter: Option<&str>,
        since: Option<&str>,
        lines: usize,
        selected_filter: usize,
    ) -> Result<()> {
        let mut state = read_state_file(state_file)?;
        state.version = 1;
        state.active_profile = Some(profile_name.to_string());
        state.profiles.insert(
            profile_name.to_string(),
            WatchProfileState {
                filters: filters.iter().map(watch_filter_rule_to_state).collect(),
                quick_filter: quick_filter
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string),
                since: since
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string),
                lines: Some(lines.max(1)),
                selected_filter_index: Some(selected_filter),
            },
        );
        write_state_file(state_file, &state)
    }

    fn open_log_file(path: &Path) -> Result<Option<std::fs::File>> {
        if !path.exists() {
            return Ok(None);
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(path)
            .with_context(|| format!("failed opening log file {}", path.display()))?;
        Ok(Some(file))
    }

    fn preload_entries(
        path: &Path,
        entries: &mut VecDeque<String>,
        lines: usize,
        since_cutoff: Option<OffsetDateTime>,
        max_entries: usize,
    ) -> Result<()> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed reading log file {}", path.display()))?;
        for line in content.lines() {
            if line_matches_since(line, since_cutoff) {
                entries.push_back(line.to_string());
            }
        }
        while entries.len() > max_entries {
            let _ = entries.pop_front();
        }
        if entries.len() > lines {
            let drop = entries.len().saturating_sub(lines);
            for _ in 0..drop {
                let _ = entries.pop_front();
            }
        }
        Ok(())
    }

    fn read_log_delta(
        file: &mut std::fs::File,
        path: &Path,
        read_offset: &mut u64,
        pending_fragment: &mut String,
    ) -> Result<Option<Vec<String>>> {
        let metadata = file
            .metadata()
            .with_context(|| format!("failed reading metadata for {}", path.display()))?;
        let file_len = metadata.len();
        if file_len < *read_offset {
            *read_offset = 0;
        }
        if file_len == *read_offset {
            return Ok(None);
        }

        file.seek(std::io::SeekFrom::Start(*read_offset))
            .with_context(|| format!("failed seeking {}", path.display()))?;
        let mut chunk = String::new();
        file.read_to_string(&mut chunk)
            .with_context(|| format!("failed reading appended logs from {}", path.display()))?;
        *read_offset = file_len;

        pending_fragment.push_str(&chunk);
        let mut complete = Vec::new();
        let ends_with_newline = pending_fragment.ends_with('\n');
        for segment in pending_fragment.split('\n') {
            complete.push(segment.to_string());
        }
        if !ends_with_newline {
            let last = complete.pop().unwrap_or_default();
            *pending_fragment = last;
        } else {
            pending_fragment.clear();
        }
        if let Some(last) = complete.last()
            && last.is_empty()
        {
            let _ = complete.pop();
        }
        Ok(Some(complete))
    }

    fn viewport_height() -> usize {
        let (_, rows) = terminal::size().unwrap_or((120, 40));
        rows.saturating_sub(WATCH_STATUS_HEIGHT + WATCH_FILTER_HEIGHT + WATCH_INFO_HEIGHT) as usize
    }

    #[allow(clippy::too_many_arguments)]
    fn render(
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
        title: &str,
        active_path: &Path,
        entries: &VecDeque<String>,
        filters: &[LogFilterRule],
        selected_filter: usize,
        quick_filter: Option<&str>,
        paused: bool,
        profile_name: &str,
        log_cursor: usize,
        auto_follow: bool,
        status_message: &str,
    ) -> Result<()> {
        let mut visible_lines = entries
            .iter()
            .filter(|line| line_visible_in_watch(line, filters, quick_filter))
            .cloned()
            .collect::<Vec<_>>();
        if visible_lines.is_empty() {
            visible_lines.push("(no visible log lines)".to_string());
        }

        let cursor = log_cursor.min(visible_lines.len().saturating_sub(1));
        let viewport_height = viewport_height().max(1);
        let start = cursor.saturating_sub(viewport_height.saturating_sub(1));
        let end = (start + viewport_height).min(visible_lines.len());
        let visible_slice = &visible_lines[start..end];

        terminal.draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(WATCH_STATUS_HEIGHT),
                    Constraint::Min(5),
                    Constraint::Length(WATCH_FILTER_HEIGHT),
                    Constraint::Length(WATCH_INFO_HEIGHT),
                ])
                .split(frame.area());

            let mode = if paused { "PAUSED" } else { "LIVE" };
            let follow = if auto_follow { "follow" } else { "manual" };
            let header_text = vec![
                Line::from(vec![
                    Span::styled(title, Style::default().fg(Color::Cyan)),
                    Span::raw(format!(
                        "  [{mode}] [{follow}] profile={} total={} visible={} file={}",
                        profile_name,
                        entries.len(),
                        visible_lines.len(),
                        active_path.display()
                    )),
                ]),
                Line::from(
                    "keys: q quit | p pause | j/k move | g/G top/bottom | ctrl-u/d half-page | pgup/pgdn page | a/x add | t toggle | i case | d delete | c clear | / search",
                ),
            ];
            let header = Paragraph::new(header_text)
                .block(Block::default().borders(Borders::ALL).title("Status"));
            frame.render_widget(header, chunks[0]);

            let log_items = visible_slice
                .iter()
                .map(|line| ListItem::new(ansi_to_line(line)))
                .collect::<Vec<_>>();
            let logs_block =
                List::new(log_items).block(Block::default().borders(Borders::ALL).title("Logs"));
            frame.render_widget(logs_block, chunks[1]);

            let filter_lines = if filters.is_empty() {
                vec![Line::from("(none)")]
            } else {
                filters
                    .iter()
                    .enumerate()
                    .map(|(index, filter)| {
                        let marker = if index == selected_filter { '>' } else { ' ' };
                        let kind = match filter.kind {
                            LogFilterKind::Include => "+",
                            LogFilterKind::Exclude => "-",
                        };
                        let enabled = if filter.enabled { "on" } else { "off" };
                        let case = match filter.case_mode {
                            LogFilterCaseMode::Sensitive => "CS",
                            LogFilterCaseMode::Insensitive => "CI",
                        };
                        let error = if filter.has_error() {
                            " (regex error)"
                        } else {
                            ""
                        };
                        Line::from(format!(
                            "{marker} {kind} [{enabled}|{case}] /{}{error}",
                            filter.pattern
                        ))
                    })
                    .collect()
            };
            let filter_panel = Paragraph::new(filter_lines)
                .block(Block::default().borders(Borders::ALL).title("Filters"));
            frame.render_widget(filter_panel, chunks[2]);

            let footer_text = if status_message.is_empty() {
                match quick_filter {
                    Some(value) if !value.is_empty() => {
                        format!("quick filter: {value} | cursor: {}/{}", cursor + 1, visible_lines.len())
                    }
                    _ => format!("cursor: {}/{}", cursor + 1, visible_lines.len()),
                }
            } else {
                status_message.to_string()
            };
            let footer =
                Paragraph::new(footer_text).block(Block::default().borders(Borders::ALL).title("Info"));
            frame.render_widget(footer, chunks[3]);
        })?;

        Ok(())
    }

    fn prompt_line(prompt: &str) -> Result<Option<String>> {
        disable_raw_mode().context("failed disabling raw mode for prompt")?;
        {
            let mut stdout = std::io::stdout();
            execute!(stdout, Show).context("failed showing cursor for prompt")?;
            print!("\r\n{prompt}");
            stdout.flush().context("failed flushing prompt")?;
        }

        let mut input = String::new();
        let read_result = std::io::stdin().read_line(&mut input);

        {
            let mut stdout = std::io::stdout();
            execute!(stdout, Hide).context("failed hiding cursor after prompt")?;
        }
        enable_raw_mode().context("failed re-enabling raw mode after prompt")?;
        read_result.context("failed reading prompt input")?;

        let trimmed = input.trim().to_string();
        if trimmed.is_empty() {
            return Ok(None);
        }
        Ok(Some(trimmed))
    }
}

#[cfg(feature = "tui")]
/// Re-export of the interactive watch entrypoint.
pub use tui::run_watch;
