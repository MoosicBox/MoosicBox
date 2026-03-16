use anyhow::{Context, Result};
use regex::{Regex, RegexBuilder};
use std::path::{Path, PathBuf};
use time::{Duration as TimeDuration, OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "persistence-json",
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(feature = "persistence-json", serde(rename_all = "snake_case"))]
pub enum LogFilterKind {
    Include,
    Exclude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "persistence-json",
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(feature = "persistence-json", serde(rename_all = "snake_case"))]
pub enum LogFilterCaseMode {
    Sensitive,
    Insensitive,
}

#[derive(Debug, Clone)]
pub struct LogFilterRule {
    pub kind: LogFilterKind,
    pub pattern: String,
    pub case_mode: LogFilterCaseMode,
    pub enabled: bool,
    regex: std::result::Result<Regex, String>,
}

impl LogFilterRule {
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

    #[must_use]
    pub fn has_error(&self) -> bool {
        self.regex.is_err()
    }

    pub fn toggle_case_mode(&mut self) {
        self.case_mode = match self.case_mode {
            LogFilterCaseMode::Sensitive => LogFilterCaseMode::Insensitive,
            LogFilterCaseMode::Insensitive => LogFilterCaseMode::Sensitive,
        };
        self.regex = compile_filter_regex(&self.pattern, self.case_mode);
    }

    #[must_use]
    pub fn matches(&self, line: &str) -> bool {
        if !self.enabled {
            return false;
        }
        self.regex.as_ref().is_ok_and(|regex| regex.is_match(line))
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(
    feature = "persistence-json",
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct WatchFilterState {
    pub kind: LogFilterKind,
    pub pattern: String,
    pub case_mode: LogFilterCaseMode,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct WatchRunConfig {
    pub title: String,
    pub log_dir: PathBuf,
    pub log_file_prefix: String,
    pub lines: Option<usize>,
    pub since: Option<String>,
    pub profile: Option<String>,
    pub include: Vec<String>,
    pub include_i: Vec<String>,
    pub exclude: Vec<String>,
    pub exclude_i: Vec<String>,
    pub state_file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ProfileSummary {
    pub name: String,
    pub active: bool,
    pub filter_count: usize,
}

#[derive(Debug, Clone)]
pub struct ProfileDetails {
    pub name: String,
    pub active: bool,
    pub quick_filter: Option<String>,
    pub since: Option<String>,
    pub lines: Option<usize>,
    pub selected_filter_index: Option<usize>,
    pub filters: Vec<WatchFilterState>,
}

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

pub fn parse_since_cutoff(raw: &str) -> Result<OffsetDateTime> {
    let duration = parse_since_duration(raw)?;
    let now = OffsetDateTime::now_utc();
    Ok(now - duration)
}

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

#[must_use]
pub fn watch_filter_rule_to_state(rule: &LogFilterRule) -> WatchFilterState {
    WatchFilterState {
        kind: rule.kind,
        pattern: rule.pattern.clone(),
        case_mode: rule.case_mode,
        enabled: rule.enabled,
    }
}

#[must_use]
pub fn watch_filter_state_to_rule(state: WatchFilterState) -> LogFilterRule {
    let mut rule = LogFilterRule::new(state.kind, state.pattern, state.case_mode);
    rule.enabled = state.enabled;
    rule
}

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
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
    use std::collections::VecDeque;
    use std::io::{Read, Seek, Write};
    use std::time::Duration;

    const WATCH_BUFFER_LIMIT: usize = 20_000;
    const WATCH_STATUS_HEIGHT: u16 = 4;
    const WATCH_FILTER_HEIGHT: u16 = 5;
    const WATCH_INFO_HEIGHT: u16 = 3;

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
                .map(|line| ListItem::new(line.clone()))
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
pub use tui::run_watch;
