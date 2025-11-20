use std::{sync::LazyLock, time::Duration};

use switchy::random::{rng, simulator::seed};

use crate::{RUNS, formatting::TimeFormat as _};

/// Configuration for a simulation run.
///
/// Controls various aspects of the simulation environment including randomness,
/// failure rates, network properties, and timing.
#[derive(Debug, Clone, Copy)]
pub struct SimConfig {
    /// Random seed for reproducible simulations.
    pub seed: u64,
    /// Probability (0.0 to 1.0) that a component will fail.
    pub fail_rate: f64,
    /// Probability (0.0 to 1.0) that a failed component will be repaired.
    pub repair_rate: f64,
    /// Maximum number of TCP messages in flight.
    pub tcp_capacity: u64,
    /// Maximum number of UDP messages in flight.
    pub udp_capacity: u64,
    /// Whether to randomize the order of actor execution.
    pub enable_random_order: bool,
    /// Minimum simulated network latency.
    pub min_message_latency: Duration,
    /// Maximum simulated network latency.
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
            fail_rate: 0.0,
            repair_rate: 1.0,
            tcp_capacity: 64,
            udp_capacity: 64,
            enable_random_order: false,
            min_message_latency: Duration::from_millis(0),
            max_message_latency: Duration::from_millis(1000),
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

        let min_message_latency = rng().gen_range_dist(0..=1000, 1.0);

        let config = config
            .fail_rate(0.0)
            .repair_rate(1.0)
            .tcp_capacity(64)
            .udp_capacity(64)
            .enable_random_order(true)
            .min_message_latency(Duration::from_millis(min_message_latency))
            .max_message_latency(Duration::from_millis(
                rng().gen_range(min_message_latency..2000),
            ))
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

    /// Sets the failure rate (0.0 to 1.0) and returns a mutable reference to self.
    #[must_use]
    pub const fn fail_rate(&mut self, fail_rate: f64) -> &mut Self {
        self.fail_rate = fail_rate;
        self
    }

    /// Sets the repair rate (0.0 to 1.0) and returns a mutable reference to self.
    #[must_use]
    pub const fn repair_rate(&mut self, repair_rate: f64) -> &mut Self {
        self.repair_rate = repair_rate;
        self
    }

    /// Sets the TCP capacity and returns a mutable reference to self.
    #[must_use]
    pub const fn tcp_capacity(&mut self, tcp_capacity: u64) -> &mut Self {
        self.tcp_capacity = tcp_capacity;
        self
    }

    /// Sets the UDP capacity and returns a mutable reference to self.
    #[must_use]
    pub const fn udp_capacity(&mut self, udp_capacity: u64) -> &mut Self {
        self.udp_capacity = udp_capacity;
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
}

/// Properties describing a simulation run.
///
/// Contains the configuration and metadata about a specific simulation run.
#[derive(Debug)]
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
#[derive(Debug)]
pub struct SimRunProperties {
    /// Number of simulation steps executed.
    pub steps: u64,
    /// Real-world time elapsed in milliseconds.
    pub real_time_millis: u128,
    /// Simulated time elapsed in milliseconds.
    pub sim_time_millis: u128,
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

        let run_from_seed = if *RUNS == 1 && switchy::random::simulator::contains_fixed_seed() {
            String::new()
        } else {
            let cmd = get_run_command(
                &[
                    "SIMULATOR_SEED",
                    "SIMULATOR_RUNS",
                    "SIMULATOR_DURATION",
                    "SIMULATOR_MAX_PARALLEL",
                ],
                config.seed,
            );
            format!("\n\nTo run again with this seed: `{cmd}`")
        };
        let run_from_start = if !switchy::random::simulator::contains_fixed_seed() && *RUNS > 1 {
            let cmd = get_run_command(
                &["SIMULATOR_SEED"],
                switchy::random::simulator::initial_seed(),
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
    let runs = *RUNS;
    let runs = if runs > 1 {
        format!("{run_number}/{runs}")
    } else {
        runs.to_string()
    };

    format!(
        "\
        seed={seed}\n\
        run={runs}{extra_top}\n\
        tick_duration={tick_duration}\n\
        fail_rate={fail_rate}\n\
        repair_rate={repair_rate}\n\
        tcp_capacity={tcp_capacity}\n\
        udp_capacity={udp_capacity}\n\
        enable_random_order={enable_random_order}\n\
        min_message_latency={min_message_latency}\n\
        max_message_latency={max_message_latency}\n\
        duration={duration}{extra_str}\
        ",
        seed = config.seed,
        tick_duration = config.tick_duration.as_millis(),
        fail_rate = config.fail_rate,
        repair_rate = config.repair_rate,
        tcp_capacity = config.tcp_capacity,
        udp_capacity = config.udp_capacity,
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

fn get_run_command(skip_env: &[&str], seed: u64) -> String {
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
        if skip_env.iter().any(|x| *x == name) {
            continue;
        }

        write!(env_vars, "{name}={} ", shell_words::quote(value.as_str())).unwrap();
    }

    format!("SIMULATOR_SEED={seed} {env_vars}{cmd}")
}
