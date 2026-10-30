//! Resolved, backend-independent execution plans for trusted pull workers.
//!
//! Validation checks scheduling structure and bounded metadata, not whether commands
//! are safe to execute. Shell names are worker capabilities, not a fixed Unix-only
//! allowlist. Callers must validate deserialized plans before scheduling them and
//! enforce input-size limits before deserialization as well.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// Maximum number of tasks in a resolved plan.
pub const MAX_TASKS: usize = 4_096;
/// Maximum number of command steps in one task.
pub const MAX_STEPS_PER_TASK: usize = 256;
/// Maximum number of command steps across the entire plan.
pub const MAX_TOTAL_STEPS: usize = 16_384;
/// Maximum number of dependency edges across the entire plan.
pub const MAX_DEPENDENCIES: usize = 65_536;
/// Maximum number of labels per task or environment entries per step.
pub const MAX_MAP_ENTRIES: usize = 256;
/// Maximum length in bytes of a task or dependency identifier.
pub const MAX_ID_BYTES: usize = 128;
/// Maximum length in bytes of any individual string, including command scripts.
pub const MAX_TEXT_BYTES: usize = 1_048_576;
/// Maximum combined string bytes, counting repeated dependency IDs, in a plan.
pub const MAX_PLAN_TEXT_BYTES: usize = 16_777_216;

/// An immutable source snapshot and its resolved task graph.
///
/// Fields remain public for adapters and dynamic plan generators. Construction and
/// deserialization do not validate the graph; use [`Self::validate`] explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// Worker-resolvable repository locator; never interpreted as a shell command.
    pub repository: String,
    /// Full immutable commit digest: exactly 40 or 64 ASCII hexadecimal digits.
    pub revision: String,
    /// Tasks keyed by their unique, case-sensitive identifiers.
    pub tasks: BTreeMap<String, Task>,
}

/// Dependency outcome required before scheduling a task.
/// Explicit run cancellation prevents both modes from starting new work.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskWhen {
    /// All dependencies must succeed.
    #[default]
    Success,
    /// All dependencies must be terminal, including failed, blocked or cancelled.
    Always,
}

/// A resolved schedulable unit with sequential command steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// Dependency outcome policy; omitted values preserve success-only scheduling.
    #[serde(default)]
    pub when: TaskWhen,
    /// Distinct task IDs that must settle according to `when` before this task runs.
    pub needs: Vec<String>,
    /// Commands executed in order on the same worker.
    pub steps: Vec<CommandStep>,
    /// Worker-selection requirements; interpretation belongs to the scheduler.
    pub labels: BTreeMap<String, String>,
    /// Positive number of CPU units requested from a worker.
    pub cpu: u32,
    /// Requested memory in MiB; zero specifies no minimum memory requirement.
    pub memory_mb: u64,
    /// Positive attempt limit, including the first execution.
    pub max_attempts: u32,
}

/// One command in a resolved task, with an explicit worker-supported shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandStep {
    /// Nonblank shell capability, for example `sh`, `pwsh`, or `cmd`.
    pub shell: String,
    /// Nonblank command script; newlines and platform-specific syntax are allowed.
    pub run: String,
    /// Environment overrides with portable ASCII variable names and NUL-free values.
    pub env: BTreeMap<String, String>,
    /// Positive wall-clock timeout in seconds.
    pub timeout_secs: u64,
}

impl Plan {
    /// Check that this plan is bounded and forms a valid execution DAG.
    ///
    /// Task IDs must match `[A-Za-z0-9_][A-Za-z0-9_.-]*` and be at most
    /// [`MAX_ID_BYTES`] bytes. Empty graphs are rejected. All string and collection
    /// limits are inclusive; see the module constants. Duplicate dependencies are
    /// rejected rather than silently normalized. Cycle detection is non-recursive.
    ///
    /// # Errors
    ///
    /// * The repository is blank or contains control characters, or the revision
    ///   is not a full 40- or 64-digit hexadecimal digest.
    /// * The graph is empty, IDs are unsafe, dependencies are missing or repeated,
    ///   or the graph cycles.
    /// * Tasks have no steps, or CPU, attempt limits, or step timeouts are zero.
    /// * Shells or scripts are blank, shells contain control characters, strings
    ///   contain NUL, environment names are invalid, label keys are blank, or a
    ///   documented size limit is exceeded.
    pub fn validate(&self) -> Result<(), String> {
        if self.repository.trim().is_empty() || self.repository.chars().any(char::is_control) {
            return Err("repository must be nonblank and contain no control characters".into());
        }
        if !matches!(self.revision.len(), 40 | 64)
            || !self.revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("revision must contain exactly 40 or 64 ASCII hexadecimal digits".into());
        }
        if self.tasks.is_empty() || self.tasks.len() > MAX_TASKS {
            return Err(format!("plan must contain 1..={MAX_TASKS} tasks"));
        }

        let mut text_bytes = 0;
        check_text(&self.repository, &mut text_bytes)?;
        check_text(&self.revision, &mut text_bytes)?;
        let mut dependencies = 0;
        let mut steps = 0;
        for (id, task) in &self.tasks {
            check_id(id)?;
            check_text(id, &mut text_bytes)?;
            dependencies += task.needs.len();
            if dependencies > MAX_DEPENDENCIES {
                return Err(format!("plan exceeds {MAX_DEPENDENCIES} dependency edges"));
            }
            steps += task.steps.len();
            if steps > MAX_TOTAL_STEPS {
                return Err(format!("plan exceeds {MAX_TOTAL_STEPS} command steps"));
            }
            if task.steps.is_empty() || task.steps.len() > MAX_STEPS_PER_TASK {
                return Err(format!(
                    "task {id:?} must contain 1..={MAX_STEPS_PER_TASK} steps"
                ));
            }
            if task.cpu == 0 || task.max_attempts == 0 {
                return Err(format!(
                    "task {id:?} must request positive CPU and attempts"
                ));
            }
            let mut seen = BTreeSet::new();
            for dependency in &task.needs {
                check_id(dependency)?;
                check_text(dependency, &mut text_bytes)?;
                if !self.tasks.contains_key(dependency) {
                    return Err(format!("task {id:?} needs missing task {dependency:?}"));
                }
                if !seen.insert(dependency) {
                    return Err(format!("task {id:?} repeats dependency {dependency:?}"));
                }
            }
            check_map(&task.labels, false, &mut text_bytes)?;
            for (index, step) in task.steps.iter().enumerate() {
                if step.shell.trim().is_empty()
                    || step.shell.chars().any(char::is_control)
                    || step.run.trim().is_empty()
                    || step.timeout_secs == 0
                {
                    return Err(format!(
                        "task {id:?} step {index} requires a nonblank shell/script and positive timeout; shell must contain no control characters"
                    ));
                }
                check_text(&step.shell, &mut text_bytes)?;
                check_text(&step.run, &mut text_bytes)?;
                check_map(&step.env, true, &mut text_bytes)?;
            }
        }
        self.validate_acyclic()
    }

    /// Atomically append tasks, permitting exact replays but never replacement.
    ///
    /// An existing ID is accepted only when its entire [`Task`] is equal, including
    /// dependency and step order. Thus retrying identical additions is idempotent;
    /// semantically similar but differently ordered tasks are not identical. New
    /// tasks may depend on existing tasks or other tasks in the same batch.
    /// Empty additions are a no-op only if the existing plan is valid.
    ///
    /// Both the existing plan and the complete candidate must validate. On every
    /// error, `self` is unchanged; successful additions never modify existing tasks,
    /// the repository, or the revision.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use gpipe::plan::{CommandStep, Plan, Task, TaskWhen};
    ///
    /// let build = Task {
    ///     when: TaskWhen::Success,
    ///     needs: vec![],
    ///     steps: vec![CommandStep {
    ///         shell: "sh".into(), run: "echo ready".into(),
    ///         env: BTreeMap::new(), timeout_secs: 60,
    ///     }],
    ///     labels: BTreeMap::new(), cpu: 1, memory_mb: 0, max_attempts: 1,
    /// };
    /// let mut plan = Plan {
    ///     repository: "https://example.invalid/repository.git".into(),
    ///     revision: "a".repeat(40),
    ///     tasks: BTreeMap::from([("build".into(), build.clone())]),
    /// };
    /// let mut test = build;
    /// test.needs.push("build".into());
    /// let additions = BTreeMap::from([("test".into(), test)]);
    /// plan.expand(additions.clone())?;
    /// let expanded = plan.clone();
    /// plan.expand(additions)?; // Exact replay is a no-op.
    /// assert_eq!(plan, expanded);
    /// # Ok::<(), String>(())
    /// ```
    ///
    /// # Errors
    ///
    /// * An existing task ID is supplied with a different definition.
    /// * The existing plan or expanded candidate fails [`Self::validate`], including
    ///   graph, resource, command, metadata, and size checks.
    pub fn expand(&mut self, additions: BTreeMap<String, Task>) -> Result<(), String> {
        self.validate()?;
        let mut task_count = self.tasks.len();
        for (id, task) in &additions {
            if let Some(existing) = self.tasks.get(id) {
                if existing != task {
                    return Err(format!("cannot replace existing task {id:?}"));
                }
            } else {
                task_count += 1;
                if task_count > MAX_TASKS {
                    return Err(format!("plan exceeds {MAX_TASKS} tasks"));
                }
            }
        }
        let mut candidate = self.clone();
        for (id, task) in additions {
            candidate.tasks.entry(id).or_insert(task);
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    fn validate_acyclic(&self) -> Result<(), String> {
        let mut remaining: BTreeMap<&str, usize> = self
            .tasks
            .iter()
            .map(|(id, task)| (id.as_str(), task.needs.len()))
            .collect();
        let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (id, task) in &self.tasks {
            for dependency in &task.needs {
                dependents.entry(dependency).or_default().push(id);
            }
        }
        let mut ready: BTreeSet<&str> = remaining
            .iter()
            .filter_map(|(&id, &count)| (count == 0).then_some(id))
            .collect();
        let mut visited = 0;
        while let Some(id) = ready.pop_first() {
            visited += 1;
            if let Some(children) = dependents.get(id) {
                for child in children {
                    if let Some(count) = remaining.get_mut(child) {
                        *count -= 1;
                        if *count == 0 {
                            ready.insert(child);
                        }
                    }
                }
            }
        }
        if visited != self.tasks.len() {
            return Err("task dependency graph contains a cycle".into());
        }
        Ok(())
    }
}

fn check_id(id: &str) -> Result<(), String> {
    let mut bytes = id.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if id.len() > MAX_ID_BYTES
        || !valid_start
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(format!(
            "unsafe task identifier (must match [A-Za-z0-9_][A-Za-z0-9_.-]*, at most {MAX_ID_BYTES} bytes)"
        ));
    }
    Ok(())
}

fn check_text(text: &str, total: &mut usize) -> Result<(), String> {
    if text.len() > MAX_TEXT_BYTES || text.contains('\0') {
        return Err(format!(
            "plan strings must be NUL-free and at most {MAX_TEXT_BYTES} bytes"
        ));
    }
    *total += text.len();
    if *total > MAX_PLAN_TEXT_BYTES {
        return Err(format!("plan exceeds {MAX_PLAN_TEXT_BYTES} string bytes"));
    }
    Ok(())
}

fn check_map(
    map: &BTreeMap<String, String>,
    environment: bool,
    text_bytes: &mut usize,
) -> Result<(), String> {
    if map.len() > MAX_MAP_ENTRIES {
        return Err(format!("metadata map exceeds {MAX_MAP_ENTRIES} entries"));
    }
    for (key, value) in map {
        if key.trim().is_empty() {
            return Err("metadata keys must not be blank".into());
        }
        if environment {
            let mut bytes = key.bytes();
            if !bytes
                .next()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
                || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            {
                return Err("environment names must match [A-Za-z_][A-Za-z0-9_]*".into());
            }
        }
        check_text(key, text_bytes)?;
        check_text(value, text_bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(needs: &[&str]) -> Task {
        Task {
            when: TaskWhen::Success,
            needs: needs.iter().map(|id| (*id).into()).collect(),
            steps: vec![CommandStep {
                shell: "sh".into(),
                run: "echo ready".into(),
                env: BTreeMap::new(),
                timeout_secs: 60,
            }],
            labels: BTreeMap::from([("os".into(), "linux".into())]),
            cpu: 1,
            memory_mb: 0,
            max_attempts: 1,
        }
    }

    fn plan() -> Plan {
        Plan {
            repository: "https://example.invalid/repository.git".into(),
            revision: "a".repeat(40),
            tasks: BTreeMap::from([("build".into(), task(&[]))]),
        }
    }

    #[test]
    fn task_when_defaults_to_success_and_round_trips_always() {
        let mut value = serde_json::to_value(task(&[])).unwrap();
        value.as_object_mut().unwrap().remove("when");
        let legacy: Task = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(legacy.when, TaskWhen::Success);
        value["when"] = serde_json::json!("ALWAYS");
        let always: Task = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(always.when, TaskWhen::Always);
        assert_eq!(serde_json::to_value(always).unwrap()["when"], "ALWAYS");
        value["when"] = serde_json::json!("sometimes");
        assert!(serde_json::from_value::<Task>(value).is_err());
    }

    #[test]
    fn accepts_full_digests_and_cross_platform_commands() {
        for revision in ["a".repeat(40), "F".repeat(64)] {
            for shell in ["sh", "pwsh", "cmd", "custom-shell"] {
                let mut plan = plan();
                plan.revision.clone_from(&revision);
                let step = &mut plan.tasks.get_mut("build").unwrap().steps[0];
                step.shell = shell.into();
                step.run = "echo first\necho second".into();
                step.env.insert("_PATH2".into(), String::new());
                assert_eq!(plan.validate(), Ok(()));
            }
        }
    }

    #[test]
    fn rejects_mutable_or_malformed_revisions() {
        for revision in [
            "",
            "main",
            "v1.0.0",
            &"a".repeat(39),
            &"a".repeat(41),
            &"a".repeat(63),
            &"a".repeat(65),
            &"g".repeat(40),
            &"é".repeat(20),
        ] {
            let mut plan = plan();
            plan.revision = revision.into();
            assert!(plan.validate().is_err(), "accepted {revision:?}");
        }
    }

    #[test]
    fn checks_repository_and_empty_graph() {
        for repository in ["", " \t", "repo\nother", "repo\0"] {
            let mut plan = plan();
            plan.repository = repository.into();
            assert!(plan.validate().is_err());
        }
        let mut plan = plan();
        plan.tasks.clear();
        assert!(plan.validate().is_err());
    }

    #[test]
    fn task_identifiers_are_safe_and_bounded() {
        for id in [
            "",
            ".",
            "..",
            "../escape",
            "a/b",
            "a\\b",
            "a b",
            "a\n",
            "é",
            "-flag",
            &"x".repeat(MAX_ID_BYTES + 1),
        ] {
            let mut plan = plan();
            plan.tasks = BTreeMap::from([(id.into(), task(&[]))]);
            assert!(plan.validate().is_err(), "accepted {id:?}");
        }
        for id in [
            "_task",
            "build.linux-x86_64",
            "0",
            &"x".repeat(MAX_ID_BYTES),
        ] {
            let mut plan = plan();
            plan.tasks = BTreeMap::from([(id.into(), task(&[]))]);
            assert_eq!(plan.validate(), Ok(()));
        }
    }

    #[test]
    fn rejects_missing_duplicate_and_unsafe_dependencies() {
        for needs in [vec!["missing"], vec!["build", "build"], vec!["../build"]] {
            let mut plan = plan();
            plan.tasks.insert("test".into(), task(&needs));
            assert!(plan.validate().is_err());
        }
    }

    #[test]
    fn detects_self_and_disconnected_cycles() {
        let mut plan = plan();
        plan.tasks.insert("self".into(), task(&["self"]));
        assert!(plan.validate().unwrap_err().contains("cycle"));
        plan.tasks.remove("self");
        plan.tasks.insert("a".into(), task(&["b"]));
        plan.tasks.insert("b".into(), task(&["a"]));
        assert!(plan.validate().unwrap_err().contains("cycle"));
    }

    #[test]
    fn accepts_diamonds_and_maximum_depth_without_recursion() {
        let mut plan = plan();
        plan.tasks.insert("left".into(), task(&["build"]));
        plan.tasks.insert("right".into(), task(&["build"]));
        plan.tasks.insert("join".into(), task(&["left", "right"]));
        assert_eq!(plan.validate(), Ok(()));
        plan.tasks.clear();
        for index in 0..MAX_TASKS {
            let needs = if index == 0 {
                vec![]
            } else {
                vec![format!("t{}", index - 1)]
            };
            let mut task = task(&[]);
            task.needs = needs;
            plan.tasks.insert(format!("t{index}"), task);
        }
        assert_eq!(plan.validate(), Ok(()));
        let original = plan.clone();
        assert!(
            plan.expand(BTreeMap::from([("extra".into(), task(&[]))]))
                .is_err()
        );
        assert_eq!(plan, original);
        plan.tasks.insert("extra".into(), task(&[]));
        assert!(plan.validate().is_err());
    }

    #[test]
    fn requires_steps_positive_cpu_attempts_and_timeouts() {
        let mut invalid = Vec::new();
        let mut empty = task(&[]);
        empty.steps.clear();
        invalid.push(empty);
        let mut zero_cpu = task(&[]);
        zero_cpu.cpu = 0;
        invalid.push(zero_cpu);
        let mut zero_attempts = task(&[]);
        zero_attempts.max_attempts = 0;
        invalid.push(zero_attempts);
        let mut zero_timeout = task(&[]);
        zero_timeout.steps[0].timeout_secs = 0;
        invalid.push(zero_timeout);
        for task in invalid {
            let mut plan = plan();
            plan.tasks.insert("build".into(), task);
            assert!(plan.validate().is_err());
        }
    }

    #[test]
    fn rejects_blank_commands_and_invalid_environment() {
        for value in ["", " \n", "nul\0script"] {
            let mut plan = plan();
            plan.tasks.get_mut("build").unwrap().steps[0].run = value.into();
            assert!(plan.validate().is_err());
        }
        for shell in ["", " ", "sh\n", "sh\0"] {
            let mut plan = plan();
            plan.tasks.get_mut("build").unwrap().steps[0].shell = shell.into();
            assert!(plan.validate().is_err());
        }
        for key in ["", "9NAME", "A=B", "A-B", "é"] {
            let mut plan = plan();
            plan.tasks.get_mut("build").unwrap().steps[0]
                .env
                .insert(key.into(), "value".into());
            assert!(plan.validate().is_err());
        }
        let mut plan = plan();
        plan.tasks.get_mut("build").unwrap().steps[0]
            .env
            .insert("NAME".into(), "bad\0value".into());
        assert!(plan.validate().is_err());
    }

    #[test]
    fn enforces_step_and_metadata_limits() {
        let mut plan = plan();
        let task = plan.tasks.get_mut("build").unwrap();
        task.steps = vec![task.steps[0].clone(); MAX_STEPS_PER_TASK];
        assert_eq!(plan.validate(), Ok(()));
        let task = plan.tasks.get_mut("build").unwrap();
        task.steps.push(task.steps[0].clone());
        assert!(plan.validate().is_err());
    }

    #[test]
    fn enforces_label_limits() {
        let mut plan = plan();
        plan.tasks.get_mut("build").unwrap().labels = (0..MAX_MAP_ENTRIES)
            .map(|index| (format!("label{index}"), String::new()))
            .collect();
        assert_eq!(plan.validate(), Ok(()));
        plan.tasks
            .get_mut("build")
            .unwrap()
            .labels
            .insert("extra".into(), String::new());
        assert!(plan.validate().is_err());
    }

    #[test]
    fn rejects_blank_label_keys_and_nul_values() {
        for labels in [
            BTreeMap::from([(" ".into(), "linux".into())]),
            BTreeMap::from([("os\0".into(), "linux".into())]),
            BTreeMap::from([("os".into(), "linux\0".into())]),
        ] {
            let mut plan = plan();
            plan.tasks.get_mut("build").unwrap().labels = labels;
            assert!(plan.validate().is_err());
        }
    }

    #[test]
    fn enforces_environment_entry_limits() {
        let mut plan = plan();
        plan.tasks.get_mut("build").unwrap().steps[0].env = (0..MAX_MAP_ENTRIES)
            .map(|index| (format!("VAR_{index}"), String::new()))
            .collect();
        assert_eq!(plan.validate(), Ok(()));
        plan.tasks.get_mut("build").unwrap().steps[0]
            .env
            .insert("EXTRA".into(), String::new());
        assert!(plan.validate().is_err());
    }

    #[test]
    fn enforces_total_step_limit() {
        let mut plan = plan();
        plan.tasks.clear();
        let mut large_task = task(&[]);
        large_task.steps = vec![large_task.steps[0].clone(); MAX_STEPS_PER_TASK];
        for index in 0..MAX_TOTAL_STEPS / MAX_STEPS_PER_TASK {
            plan.tasks.insert(format!("t{index}"), large_task.clone());
        }
        assert_eq!(plan.validate(), Ok(()));
        let original = plan.clone();
        assert!(
            plan.expand(BTreeMap::from([("extra".into(), task(&[]))]))
                .is_err()
        );
        assert_eq!(plan, original);
        plan.tasks.insert("extra".into(), task(&[]));
        assert!(plan.validate().unwrap_err().contains("command steps"));
    }

    #[test]
    fn enforces_total_dependency_limit() {
        let mut plan = plan();
        plan.tasks.clear();
        let roots: Vec<String> = (0..256).map(|index| format!("root{index}")).collect();
        for root in &roots {
            plan.tasks.insert(root.clone(), task(&[]));
        }
        for index in 0..MAX_DEPENDENCIES / roots.len() {
            let mut dependent = task(&[]);
            dependent.needs.clone_from(&roots);
            plan.tasks.insert(format!("dependent{index}"), dependent);
        }
        assert_eq!(plan.validate(), Ok(()));
        plan.tasks.insert("extra".into(), task(&["root0"]));
        assert!(plan.validate().unwrap_err().contains("dependency edges"));
    }

    #[test]
    fn enforces_individual_and_aggregate_text_bounds() {
        let mut plan = plan();
        plan.tasks.get_mut("build").unwrap().steps[0].run = "x".repeat(MAX_TEXT_BYTES);
        assert_eq!(plan.validate(), Ok(()));
        plan.tasks.get_mut("build").unwrap().steps[0].run.push('x');
        assert!(plan.validate().is_err());
        let task = plan.tasks.get_mut("build").unwrap();
        task.steps[0].run = "x".repeat(MAX_TEXT_BYTES);
        task.steps = vec![task.steps[0].clone(); MAX_PLAN_TEXT_BYTES / MAX_TEXT_BYTES];
        assert!(plan.validate().unwrap_err().contains("string bytes"));
    }

    #[test]
    fn expands_a_batch_with_forward_dependencies_and_exact_replays() {
        let mut plan = plan();
        let additions = BTreeMap::from([
            ("a-final".into(), task(&["z-test"])),
            ("z-test".into(), task(&["build"])),
            ("build".into(), task(&[])),
        ]);
        assert_eq!(plan.expand(additions.clone()), Ok(()));
        let expanded = plan.clone();
        assert_eq!(plan.expand(additions), Ok(()));
        assert_eq!(plan, expanded);
        assert_eq!(plan.expand(BTreeMap::new()), Ok(()));
        assert_eq!(plan, expanded);
    }

    #[test]
    fn failed_expansion_is_atomic_for_conflicts_and_invalid_batches() {
        let mut plan = plan();
        let original = plan.clone();
        let mut replacement = task(&[]);
        replacement.max_attempts = 2;
        for additions in [
            BTreeMap::from([("a-new".into(), task(&[])), ("build".into(), replacement)]),
            BTreeMap::from([("new".into(), task(&["missing"]))]),
            BTreeMap::from([("a".into(), task(&["b"])), ("b".into(), task(&["a"]))]),
            BTreeMap::from([("../bad".into(), task(&[]))]),
        ] {
            assert!(plan.expand(additions).is_err());
            assert_eq!(plan, original);
        }
    }

    #[test]
    fn dependency_order_is_part_of_replay_identity() {
        let mut plan = plan();
        plan.tasks.insert("other".into(), task(&[]));
        plan.tasks.insert("final".into(), task(&["build", "other"]));
        let original = plan.clone();
        assert!(
            plan.expand(BTreeMap::from([(
                "final".into(),
                task(&["other", "build"])
            ),]))
                .is_err()
        );
        assert_eq!(plan, original);
    }

    #[test]
    fn expansion_does_not_repair_invalid_existing_plans() {
        let mut plan = plan();
        plan.tasks
            .get_mut("build")
            .unwrap()
            .needs
            .push("missing".into());
        let original = plan.clone();
        assert!(
            plan.expand(BTreeMap::from([("missing".into(), task(&[]))]))
                .is_err()
        );
        assert!(plan.expand(BTreeMap::new()).is_err());
        assert_eq!(plan, original);
    }
}
