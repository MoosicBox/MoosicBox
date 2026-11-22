//! Simulation configuration and result types.
//!
//! This module defines the core data structures used to configure simulation runs
//! and report their outcomes, including [`SimConfig`], [`SimProperties`],
//! [`SimRunProperties`], and [`SimResult`].

use std::{sync::LazyLock, time::Duration};

use switchy::random::{rng, simulator::seed};

use crate::{RUNS, formatting::TimeFormat as _};

/// Configuration for a simulation run.
///
/// Controls various aspects of the simulation environment including randomness,
/// network properties, and timing.
///
/// The network settings become the run's defaults: [`Self::min_message_latency`] and
/// [`Self::max_message_latency`] the latency of the simulated network's links without a policy
/// of their own, and [`Self::tcp_capacity`] the backlog of simulated TCP listeners. Faults
/// beyond those are injected through [`crate::fault`].
#[derive(Debug, Clone, Copy)]
pub struct SimConfig {
    /// Random seed for reproducible simulations.
    pub seed: u64,
    /// How many connections a simulated TCP listener queues before refusing more.
    pub tcp_capacity: u64,
    /// Whether to randomize the order of actor execution.
    pub enable_random_order: bool,
    /// The least latency of a message (or TCP segment) over the simulated network.
    pub min_message_latency: Duration,
    /// The most latency of a message (or TCP segment) over the simulated network. Each
    /// message's latency is drawn uniformly between the two.
    pub max_message_latency: Duration,
    /// How long the simulation should run (`Duration::MAX` for unlimited).
    pub duration: Duration,
    /// Duration of each simulation tick.
    pub tick_duration: Duration,
    /// Offset from Unix epoch for simulated time.
    #[cfg(feature = "time")]
    pub epoch_offset: u64,
    /// Time multiplier for simulation steps.
    #[cfg(feature = "time")]
    pub step_multiplier: u64,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl SimConfig {
    /// Creates a new `SimConfig` with default values.
    ///
    /// Returns a configuration with reasonable defaults for testing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            seed: 0,
            tcp_capacity: 64,
            enable_random_order: false,
            min_message_latency: Duration::from_millis(0),
            max_message_latency: Duration::from_millis(0),
            duration: Duration::MAX,
            tick_duration: Duration::from_millis(1),
            #[cfg(feature = "time")]
            epoch_offset: 0,
            #[cfg(feature = "time")]
            step_multiplier: 1,
        }
    }

    /// Creates a new `SimConfig` with randomized values.
    ///
    /// Uses the current RNG to generate configuration values suitable for
    /// testing. The `SIMULATOR_DURATION` environment variable can be used
    /// to override the duration.
    ///
    /// # Panics
    ///
    /// * If `SIMULATOR_DURATION` is set but cannot be parsed as a supported
    ///   duration format.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use simvar_harness::SimConfig;
    ///
    /// let config = SimConfig::from_rng();
    /// assert!(config.max_message_latency >= config.min_message_latency);
    /// ```
    #[must_use]
    pub fn from_rng() -> Self {
        static DURATION: LazyLock<Duration> = LazyLock::new(|| {
            std::env::var("SIMULATOR_DURATION")
                .ok()
                .map_or(Duration::MAX, |x| {
                    #[allow(clippy::option_if_let_else)]
                    if let Some(x) = x.strip_suffix("µs") {
                        Duration::from_micros(x.parse::<u64>().unwrap())
                    } else if let Some(x) = x.strip_suffix("ns") {
                        Duration::from_nanos(x.parse::<u64>().unwrap())
                    } else if let Some(x) = x.strip_suffix("ms") {
                        Duration::from_millis(x.parse::<u64>().unwrap())
                    } else if let Some(x) = x.strip_suffix("s") {
                        Duration::from_secs(x.parse::<u64>().unwrap())
                    } else {
                        Duration::from_millis(x.parse::<u64>().unwrap())
                    }
                })
        });

        let mut config = Self::new();
        config.seed = seed();

        // A few milliseconds: enough to reorder concurrent work, not so much that applications
        // written without latency in mind time out. Simulations wanting more set it.
        let min_message_latency = rng().gen_range(0..=5_u64);
        let max_message_latency = rng().gen_range(min_message_latency..=min_message_latency + 20);

        let config = config
            .tcp_capacity(64)
            .enable_random_order(true)
            .min_message_latency(Duration::from_millis(min_message_latency))
            .max_message_latency(Duration::from_millis(max_message_latency))
            .duration(*DURATION);

        #[cfg(feature = "time")]
        {
            config.epoch_offset = switchy::time::simulator::epoch_offset();
            config.step_multiplier = switchy::time::simulator::step_multiplier();
        }

        #[cfg(feature = "time")]
        let config = config.tick_duration(Duration::from_millis(
            switchy::time::simulator::step_multiplier(),
        ));

        *config
    }

    /// Sets the TCP capacity and returns a mutable reference to self.
    #[must_use]
    pub const fn tcp_capacity(&mut self, tcp_capacity: u64) -> &mut Self {
        self.tcp_capacity = tcp_capacity;
        self
    }

    /// Sets whether to enable random actor execution order and returns a mutable reference to self.
    #[must_use]
    pub const fn enable_random_order(&mut self, enable_random_order: bool) -> &mut Self {
        self.enable_random_order = enable_random_order;
        self
    }

    /// Sets the minimum message latency and returns a mutable reference to self.
    #[must_use]
    pub const fn min_message_latency(&mut self, min_message_latency: Duration) -> &mut Self {
        self.min_message_latency = min_message_latency;
        self
    }

    /// Sets the maximum message latency and returns a mutable reference to self.
    #[must_use]
    pub const fn max_message_latency(&mut self, max_message_latency: Duration) -> &mut Self {
        self.max_message_latency = max_message_latency;
        self
    }

    /// Sets the simulation duration and returns a mutable reference to self.
    #[must_use]
    pub const fn duration(&mut self, duration: Duration) -> &mut Self {
        self.duration = duration;
        self
    }

    /// Sets the tick duration and returns a mutable reference to self.
    #[must_use]
    pub const fn tick_duration(&mut self, tick_duration: Duration) -> &mut Self {
        self.tick_duration = tick_duration;
        self
    }

    /// Make this configuration's network settings the current run's defaults: the latency of
    /// the simulated network's links and the backlog of simulated TCP listeners.
    pub(crate) fn apply(&self) {
        switchy::network::set_default_policy(
            switchy::network::LinkPolicy::reliable()
                .with_latency(self.min_message_latency, self.max_message_latency),
        );
        switchy::tcp::simulator::set_listener_backlog(
            usize::try_from(self.tcp_capacity).unwrap_or(usize::MAX),
        );
    }
}

/// Properties describing a simulation run.
///
/// Contains the configuration and metadata about a specific simulation run.
#[derive(Debug, Clone)]
pub struct SimProperties {
    /// Configuration used for this simulation run.
    pub config: SimConfig,
    /// Run number (1-indexed).
    pub run_number: u64,
    /// Worker thread ID, if running in parallel mode.
    pub thread_id: Option<u64>,
    /// Additional custom properties from the bootstrap.
    pub extra: Vec<(String, String)>,
}

/// Runtime metrics from a simulation run.
///
/// Captures timing and step count information after a simulation completes.
#[derive(Debug, Clone, Default)]
pub struct SimRunProperties {
    /// Number of simulation steps executed.
    pub steps: u64,
    /// Real-world time elapsed in milliseconds.
    pub real_time_millis: u128,
    /// Simulated time elapsed in milliseconds.
    pub sim_time_millis: u128,
    /// The run's interleaving coverage, when its thread scheduler recorded it
    /// (`SIMULATOR_INTERLEAVING_COVERAGE=1`, or a configuration with
    /// `interleaving_coverage` set): see
    /// [`switchy::thread::simulator::Coverage`]. A bootstrap reads it in
    /// [`crate::SimBootstrap::on_result`].
    #[cfg(feature = "thread")]
    pub coverage: Option<switchy::thread::simulator::Coverage>,
}

/// Result of a simulation run.
///
/// Indicates whether the simulation succeeded or failed, along with properties
/// and runtime metrics.
#[derive(Debug)]
pub enum SimResult {
    /// Simulation completed successfully.
    Success {
        /// Properties of the simulation run.
        props: SimProperties,
        /// Runtime metrics from the run.
        run: SimRunProperties,
    },
    /// Simulation failed with an error or panic.
    Fail {
        /// Properties of the simulation run.
        props: SimProperties,
        /// Runtime metrics from the run.
        run: SimRunProperties,
        /// Error message, if the failure was due to a returned error.
        error: Option<String>,
        /// Panic message, if the failure was due to a panic.
        panic: Option<String>,
    },
}

impl SimResult {
    /// Returns the simulation properties.
    #[must_use]
    pub const fn props(&self) -> &SimProperties {
        match self {
            Self::Success { props, .. } | Self::Fail { props, .. } => props,
        }
    }

    /// Returns the simulation configuration.
    #[must_use]
    pub const fn config(&self) -> &SimConfig {
        &self.props().config
    }

    /// Returns the runtime properties.
    #[must_use]
    pub const fn run(&self) -> &SimRunProperties {
        match self {
            Self::Success { run, .. } | Self::Fail { run, .. } => run,
        }
    }

    /// Returns `true` if the simulation succeeded.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }
}

impl std::fmt::Display for SimResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let props = self.props();
        let config = &props.config;
        let run = self.run();

        let run_from_seed = if *RUNS == Some(1) && switchy::random::simulator::contains_fixed_seed()
        {
            String::new()
        } else {
            #[cfg(feature = "time")]
            let failed_epoch_offset = Some(config.epoch_offset);
            #[cfg(not(feature = "time"))]
            let failed_epoch_offset = None;

            let run_env: Vec<(&str, &str)> = props
                .extra
                .iter()
                .filter(|(name, _)| is_environment_name(name))
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect();
            let cmd = get_run_command_with(
                &[
                    "SIMULATOR_SEED",
                    "SIMULATOR_RUNS",
                    "SIMULATOR_DURATION",
                    "SIMULATOR_MAX_PARALLEL",
                ],
                config.seed,
                failed_epoch_offset,
                &run_env,
            );
            format!("\n\nTo run again with this seed: `{cmd}`")
        };
        let run_from_start = if !switchy::random::simulator::contains_fixed_seed()
            && RUNS.is_none_or(|runs| runs > 1)
        {
            let cmd = get_run_command(
                &["SIMULATOR_SEED"],
                switchy::random::simulator::initial_seed(),
                None,
            );
            format!("\nTo run entire simulation again from the first run: `{cmd}`")
        } else {
            String::new()
        };

        let (error, panic) = match self {
            Self::Success { .. } => (String::new(), String::new()),
            Self::Fail { error, panic, .. } => (
                error
                    .as_ref()
                    .map_or_else(String::new, |x| format!("\n\nError:\n{x}")),
                panic
                    .as_ref()
                    .map_or_else(String::new, |x| format!("\n\nPanic:\n{x}")),
            ),
        };

        #[allow(clippy::cast_precision_loss)]
        f.write_fmt(format_args!(
            "\
            =========================== FINISH ===========================\n\
            Server simulator finished\n\n\
            {run_info}\n\
            steps={steps}\n\
            real_time_elapsed={real_time}\n\
            simulated_time_elapsed={simulated_time} ({simulated_time_x:.2}x)\n\n\
            successful={successful}\
            {error}{panic}{run_from_seed}{run_from_start}\n\
            ==============================================================\
            ",
            successful = self.is_success(),
            run_info = run_info(props),
            steps = run.steps,
            real_time = run.real_time_millis.into_formatted(),
            simulated_time = run.sim_time_millis.into_formatted(),
            simulated_time_x = run.sim_time_millis as f64 / run.real_time_millis as f64,
        ))
    }
}

/// Formats simulation properties as a human-readable string.
///
/// Used for logging and displaying simulation configuration details.
#[must_use]
pub fn run_info(props: &SimProperties) -> String {
    use std::fmt::Write as _;

    let config = &props.config;

    let mut extra_top = String::new();
    if let Some(thread_id) = props.thread_id {
        write!(extra_top, "\nthread_id={thread_id}").unwrap();
    }
    #[cfg(feature = "time")]
    write!(extra_top, "\nepoch_offset={}", config.epoch_offset).unwrap();
    #[cfg(feature = "time")]
    write!(extra_top, "\nstep_multiplier={}", config.step_multiplier).unwrap();

    let mut extra_str = String::new();
    for (k, v) in &props.extra {
        write!(extra_str, "\n{k}={v}").unwrap();
    }

    let duration = if config.duration == Duration::MAX {
        "forever".to_string()
    } else {
        config.duration.as_millis().to_string()
    };

    let run_number = props.run_number;
    let runs = match *RUNS {
        Some(runs) if runs > 1 => format!("{run_number}/{runs}"),
        Some(runs) => runs.to_string(),
        None => format!("{run_number}/forever"),
    };

    format!(
        "\
        seed={seed}\n\
        run={runs}{extra_top}\n\
        tick_duration={tick_duration}\n\
        tcp_capacity={tcp_capacity}\n\
        enable_random_order={enable_random_order}\n\
        min_message_latency={min_message_latency}\n\
        max_message_latency={max_message_latency}\n\
        duration={duration}{extra_str}\
        ",
        seed = config.seed,
        tick_duration = config.tick_duration.as_millis(),
        tcp_capacity = config.tcp_capacity,
        enable_random_order = config.enable_random_order,
        min_message_latency = config.min_message_latency.as_millis(),
        max_message_latency = config.max_message_latency.as_millis(),
    )
}

fn get_cargoified_args() -> Vec<String> {
    let mut args = std::env::args().collect::<Vec<_>>();

    let Some(cmd) = args.first() else {
        return args;
    };

    let mut components = cmd.split('/');

    if matches!(components.next(), Some("target")) {
        let Some(profile) = components.next() else {
            return args;
        };
        let profile = profile.to_string();

        let Some(binary_name) = components.next() else {
            return args;
        };
        let binary_name = binary_name.to_string();

        args.remove(0);
        args.insert(0, binary_name);
        args.insert(0, "-p".to_string());

        if profile == "release" {
            args.insert(0, "--release".to_string());
        } else if profile != "debug" {
            args.insert(0, profile);
            args.insert(0, "--profile".to_string());
        }

        args.insert(0, "run".to_string());
        args.insert(0, "cargo".to_string());
    }

    args
}

pub fn get_run_command(skip_env: &[&str], seed: u64, epoch_offset: Option<u64>) -> String {
    get_run_command_with(skip_env, seed, epoch_offset, &[])
}

/// Whether a [`crate::SimBootstrap::props`] name is an environment variable the run needs
/// (upper-case letters, digits and underscores, starting with a letter): such properties go in
/// the run's replay command, as a run chosen by more than its seed (a guided fleet's input, say)
/// needs them to replay.
#[must_use]
pub fn is_environment_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// [`get_run_command`], with `run_env` (this run's own environment, taking precedence over the
/// process's) in the command too.
#[must_use]
pub fn get_run_command_with(
    skip_env: &[&str],
    seed: u64,
    epoch_offset: Option<u64>,
    run_env: &[(&str, &str)],
) -> String {
    let args = get_cargoified_args();
    let quoted_args = args
        .iter()
        .map(|x| shell_words::quote(x.as_str()))
        .collect::<Vec<_>>();
    let cmd = quoted_args.join(" ");

    let mut env_vars = String::new();

    for (name, value) in std::env::vars() {
        use std::fmt::Write as _;

        if !name.starts_with("SIMULATOR_") && name != "RUST_LOG" {
            continue;
        }
        if skip_env.iter().any(|x| *x == name) || run_env.iter().any(|(x, _)| *x == name) {
            continue;
        }

        write!(env_vars, "{name}={} ", shell_words::quote(value.as_str())).unwrap();
    }
    for (name, value) in run_env {
        use std::fmt::Write as _;
        write!(env_vars, "{name}={} ", shell_words::quote(value)).unwrap();
    }

    let mut prefix = format!("SIMULATOR_SEED={seed} ");
    if let Some(epoch_offset) = epoch_offset {
        use std::fmt::Write as _;
        write!(prefix, "SIMULATOR_EPOCH_OFFSET={epoch_offset} ").unwrap();
    }

    format!("{prefix}{env_vars}{cmd}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "time")]
    struct EnvGuard {
        name: &'static str,
        value: Option<String>,
    }

    #[cfg(feature = "time")]
    impl EnvGuard {
        fn new(name: &'static str) -> Self {
            Self {
                name,
                value: std::env::var(name).ok(),
            }
        }

        fn remove(&self) {
            unsafe {
                std::env::remove_var(self.name);
            }
        }
    }

    #[cfg(feature = "time")]
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.value {
                Some(value) => unsafe {
                    std::env::set_var(self.name, value);
                },
                None => unsafe {
                    std::env::remove_var(self.name);
                },
            }
        }
    }

    #[test_log::test]
    fn test_simconfig_default() {
        let config = SimConfig::default();
        assert_eq!(config.seed, 0);
        assert_eq!(config.tcp_capacity, 64);
        assert!(!config.enable_random_order);
        assert_eq!(config.min_message_latency, Duration::from_millis(0));
        assert_eq!(config.max_message_latency, Duration::ZERO);
        assert_eq!(config.duration, Duration::MAX);
        assert_eq!(config.tick_duration, Duration::from_millis(1));
    }

    #[test_log::test]
    fn test_simconfig_new() {
        let config = SimConfig::new();
        assert_eq!(config.seed, 0);
        assert_eq!(config.min_message_latency, Duration::ZERO);
        assert_eq!(config.max_message_latency, Duration::ZERO);
    }

    #[test_log::test]
    fn test_simconfig_builder_methods() {
        let mut config = SimConfig::new();

        let _ = config
            .tcp_capacity(128)
            .enable_random_order(true)
            .min_message_latency(Duration::from_millis(10))
            .max_message_latency(Duration::from_secs(2))
            .duration(Duration::from_mins(1))
            .tick_duration(Duration::from_millis(5));

        assert_eq!(config.tcp_capacity, 128);
        assert!(config.enable_random_order);
        assert_eq!(config.min_message_latency, Duration::from_millis(10));
        assert_eq!(config.max_message_latency, Duration::from_secs(2));
        assert_eq!(config.duration, Duration::from_mins(1));
        assert_eq!(config.tick_duration, Duration::from_millis(5));
    }

    #[test_log::test]
    fn test_simconfig_builder_method_chaining() {
        let mut config = SimConfig::new();
        let _ = config.tcp_capacity(100).enable_random_order(true);

        assert_eq!(config.tcp_capacity, 100);
        assert!(config.enable_random_order);
    }

    #[test_log::test]
    fn test_simresult_is_success() {
        let props = SimProperties {
            config: SimConfig::new(),
            run_number: 1,
            thread_id: None,
            extra: vec![],
        };

        let run = SimRunProperties {
            steps: 100,
            real_time_millis: 1000,
            sim_time_millis: 5000,
            #[cfg(feature = "thread")]
            coverage: None,
        };

        let success = SimResult::Success {
            props: props.clone(),
            run: run.clone(),
        };
        assert!(success.is_success());

        let fail = SimResult::Fail {
            props,
            run,
            error: Some("test error".to_string()),
            panic: None,
        };
        assert!(!fail.is_success());
    }

    #[test_log::test]
    fn test_simresult_props() {
        let props = SimProperties {
            config: SimConfig::new(),
            run_number: 42,
            thread_id: Some(3),
            extra: vec![("key".to_string(), "value".to_string())],
        };

        let run = SimRunProperties {
            steps: 100,
            real_time_millis: 1000,
            sim_time_millis: 5000,
            #[cfg(feature = "thread")]
            coverage: None,
        };

        let result = SimResult::Success { props, run };

        let result_props = result.props();
        assert_eq!(result_props.run_number, 42);
        assert_eq!(result_props.thread_id, Some(3));
        assert_eq!(result_props.extra.len(), 1);
    }

    #[test_log::test]
    fn test_simresult_config() {
        let mut config = SimConfig::new();
        let _ = config.tcp_capacity(256);

        let props = SimProperties {
            config,
            run_number: 1,
            thread_id: None,
            extra: vec![],
        };

        let run = SimRunProperties {
            steps: 100,
            real_time_millis: 1000,
            sim_time_millis: 5000,
            #[cfg(feature = "thread")]
            coverage: None,
        };

        let result = SimResult::Success { props, run };

        assert_eq!(result.config().tcp_capacity, 256);
    }

    #[test_log::test]
    fn test_simresult_run() {
        let props = SimProperties {
            config: SimConfig::new(),
            run_number: 1,
            thread_id: None,
            extra: vec![],
        };

        let run = SimRunProperties {
            steps: 12345,
            real_time_millis: 9876,
            sim_time_millis: 54321,
            #[cfg(feature = "thread")]
            coverage: None,
        };

        let result = SimResult::Success { props, run };

        let result_run = result.run();
        assert_eq!(result_run.steps, 12345);
        assert_eq!(result_run.real_time_millis, 9876);
        assert_eq!(result_run.sim_time_millis, 54321);
    }

    #[test_log::test]
    fn test_get_cargoified_args_with_target_path() {
        // Note: This test depends on the actual command line arguments,
        // so we're just checking that it doesn't panic
        let args = get_cargoified_args();
        assert_ne!(args.len(), 0);
    }

    #[test_log::test]
    fn test_simresult_props_for_fail_variant() {
        let props = SimProperties {
            config: SimConfig::new(),
            run_number: 10,
            thread_id: Some(5),
            extra: vec![("debug".to_string(), "true".to_string())],
        };

        let run = SimRunProperties {
            steps: 500,
            real_time_millis: 2000,
            sim_time_millis: 10000,
            #[cfg(feature = "thread")]
            coverage: None,
        };

        let fail = SimResult::Fail {
            props,
            run,
            error: Some("test error".to_string()),
            panic: Some("test panic".to_string()),
        };

        let result_props = fail.props();
        assert_eq!(result_props.run_number, 10);
        assert_eq!(result_props.thread_id, Some(5));
        assert_eq!(result_props.extra.len(), 1);
    }

    #[test_log::test]
    fn test_simresult_run_for_fail_variant() {
        let props = SimProperties {
            config: SimConfig::new(),
            run_number: 1,
            thread_id: None,
            extra: vec![],
        };

        let run = SimRunProperties {
            steps: 999,
            real_time_millis: 5555,
            sim_time_millis: 8888,
            #[cfg(feature = "thread")]
            coverage: None,
        };

        let fail = SimResult::Fail {
            props,
            run,
            error: Some("error".to_string()),
            panic: None,
        };

        let result_run = fail.run();
        assert_eq!(result_run.steps, 999);
        assert_eq!(result_run.real_time_millis, 5555);
        assert_eq!(result_run.sim_time_millis, 8888);
    }

    #[test_log::test]
    fn test_simresult_config_for_fail_variant() {
        let mut config = SimConfig::new();
        let _ = config.tcp_capacity(512);

        let props = SimProperties {
            config,
            run_number: 1,
            thread_id: None,
            extra: vec![],
        };

        let run = SimRunProperties {
            steps: 100,
            real_time_millis: 1000,
            sim_time_millis: 5000,
            #[cfg(feature = "thread")]
            coverage: None,
        };

        let fail = SimResult::Fail {
            props,
            run,
            error: None,
            panic: Some("panic message".to_string()),
        };

        assert_eq!(fail.config().tcp_capacity, 512);
    }

    #[test_log::test]
    fn test_run_info_contains_config_values() {
        let mut config = SimConfig::new();
        let _ = config.tcp_capacity(128).enable_random_order(true);

        let props = SimProperties {
            config,
            run_number: 1,
            thread_id: None,
            extra: vec![],
        };

        let info = run_info(&props);

        assert!(info.contains("seed=0"));
        assert!(info.contains("tcp_capacity=128"));
        assert!(info.contains("enable_random_order=true"));
    }

    #[test_log::test]
    fn test_run_info_includes_thread_id_when_present() {
        let props = SimProperties {
            config: SimConfig::new(),
            run_number: 1,
            thread_id: Some(42),
            extra: vec![],
        };

        let info = run_info(&props);

        assert!(info.contains("thread_id=42"));
    }

    #[test_log::test]
    fn test_run_info_excludes_thread_id_when_none() {
        let props = SimProperties {
            config: SimConfig::new(),
            run_number: 1,
            thread_id: None,
            extra: vec![],
        };

        let info = run_info(&props);

        assert!(!info.contains("thread_id="));
    }

    #[test_log::test]
    fn test_run_info_includes_extra_properties() {
        let props = SimProperties {
            config: SimConfig::new(),
            run_number: 1,
            thread_id: None,
            extra: vec![
                ("custom_key".to_string(), "custom_value".to_string()),
                ("another_key".to_string(), "another_value".to_string()),
            ],
        };

        let info = run_info(&props);

        assert!(info.contains("custom_key=custom_value"));
        assert!(info.contains("another_key=another_value"));
    }

    #[test_log::test]
    fn test_run_info_duration_forever_when_max() {
        let props = SimProperties {
            config: SimConfig::new(), // duration defaults to Duration::MAX
            run_number: 1,
            thread_id: None,
            extra: vec![],
        };

        let info = run_info(&props);

        assert!(info.contains("duration=forever"));
    }

    #[test_log::test]
    fn test_run_info_duration_value_when_finite() {
        let mut config = SimConfig::new();
        let _ = config.duration(Duration::from_mins(2));

        let props = SimProperties {
            config,
            run_number: 1,
            thread_id: None,
            extra: vec![],
        };

        let info = run_info(&props);

        // 120 seconds = 120000 milliseconds
        assert!(info.contains("duration=120000"));
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn test_fail_output_includes_seed_and_epoch_offset_rerun_command() {
        let seed_guard = EnvGuard::new("SIMULATOR_SEED");
        let epoch_guard = EnvGuard::new("SIMULATOR_EPOCH_OFFSET");
        seed_guard.remove();
        epoch_guard.remove();

        let mut config = SimConfig::new();
        config.seed = 4242;
        config.epoch_offset = 1_700_000_000_000;

        let props = SimProperties {
            config,
            run_number: 1,
            thread_id: None,
            extra: vec![],
        };

        let run = SimRunProperties {
            steps: 10,
            real_time_millis: 100,
            sim_time_millis: 200,
            #[cfg(feature = "thread")]
            coverage: None,
        };

        let result = SimResult::Fail {
            props,
            run,
            error: Some("failure".to_string()),
            panic: None,
        };

        let output = result.to_string();
        assert!(output.contains("To run again with this seed:"));
        assert!(output.contains("SIMULATOR_SEED=4242"));
        assert!(output.contains("SIMULATOR_EPOCH_OFFSET=1700000000000"));
    }
}

#[cfg(test)]
mod run_environment_tests {
    use super::*;

    #[test]
    fn environment_names_are_upper_case_words() {
        assert!(is_environment_name("PANE_RUNTIME_SIM_PROFILE"));
        assert!(is_environment_name("A1"));
        assert!(!is_environment_name("profile"));
        assert!(!is_environment_name("_X"));
        assert!(!is_environment_name("1X"));
    }

    #[test]
    fn a_run_environment_goes_in_the_command() {
        let command = get_run_command_with(&[], 7, Some(9), &[("MY_PROFILE", "a=1,b=2 c")]);
        assert!(
            command.starts_with("SIMULATOR_SEED=7 SIMULATOR_EPOCH_OFFSET=9 "),
            "{command}"
        );
        assert!(command.contains("MY_PROFILE='a=1,b=2 c'"), "{command}");
    }
}
