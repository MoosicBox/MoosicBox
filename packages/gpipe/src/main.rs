#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Native gPipe coordinator, worker and management client.
use clap::{Parser, Subcommand};
use gpipe::{
    plan::{Plan, Task},
    protocol::{Envelope, PROTOCOL_VERSION, Request, Response, ServerConfig, WorkerConfig},
};
use serde::de::DeserializeOwned;
use std::{collections::BTreeMap, io::Read, path::PathBuf, time::Duration};

#[derive(Parser)]
#[command(version, about = "Native, durable pipeline orchestration")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Plan, deploy, submit or verify a source-bound native pipeline.
    Pipeline {
        #[command(subcommand)]
        command: PipelineCommand,
    },
    /// Run the coordinator (loopback HTTP; use a TLS proxy for remote access).
    Serve {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        triggers: Option<PathBuf>,
    },
    /// Run a native worker. --once processes at most one assignment.
    Worker {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        once: bool,
    },
    /// Lower an official Clippier jobs manifest using explicit per-case recipes.
    ImportClippier {
        jobs: PathBuf,
        recipes: PathBuf,
        #[arg(long)]
        repository: String,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        seed: Option<u64>,
    },
    /// Generate conservative native Rust validation recipes from Clippier cases.
    RustRecipes {
        jobs: PathBuf,
        #[arg(long)]
        seed: Option<u64>,
    },
    /// Verify exact Clippier case receipts, rejecting missing or changed cases.
    VerifyClippier {
        jobs: PathBuf,
        receipts: PathBuf,
        #[arg(long)]
        seed: Option<u64>,
    },
    /// Prepare an exact `MoosicBox` suite using the shared Clippier native executor.
    PrepareMoosicbox {
        jobs: PathBuf,
        profile: PathBuf,
        #[arg(long)]
        repository: String,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        seed: u64,
        #[arg(long)]
        clippier: PathBuf,
    },
    /// Verify run/revision/attempt-bound native suite evidence.
    VerifyMoosicbox {
        prepared: PathBuf,
        inspection: PathBuf,
    },
    /// Download and verify authoritative completed-attempt CI evidence.
    VerifyCiRun {
        prepared: PathBuf,
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GPIPE_ADMIN_TOKEN")]
        token_env: String,
        run_id: String,
    },
    /// Validate a resolved JSON execution plan without executing it.
    Validate { plan: PathBuf },
    /// Submit a plan with a stable idempotency key.
    Submit {
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GPIPE_ADMIN_TOKEN")]
        token_env: String,
        #[arg(long)]
        key: String,
        #[arg(long)]
        dynamic: bool,
        plan: PathBuf,
    },
    /// Inspect run state and recorded terminal evidence.
    Inspect {
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GPIPE_ADMIN_TOKEN")]
        token_env: String,
        run_id: String,
    },
    /// List recent runs.
    List {
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GPIPE_ADMIN_TOKEN")]
        token_env: String,
    },
    /// Cancel pending work and signal active process trees.
    Cancel {
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GPIPE_ADMIN_TOKEN")]
        token_env: String,
        run_id: String,
    },
    /// Approve an exact source revision for execution.
    Approve {
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GPIPE_ADMIN_TOKEN")]
        token_env: String,
        repository: String,
        revision: String,
    },
    /// Atomically add task definitions to an open dynamic run.
    Expand {
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GPIPE_ADMIN_TOKEN")]
        token_env: String,
        run_id: String,
        tasks: PathBuf,
    },
    /// Close dynamic expansion and allow successful finalization.
    Seal {
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GPIPE_ADMIN_TOKEN")]
        token_env: String,
        run_id: String,
    },
    /// Send a versioned operation from JSON (API escape hatch, same authorization).
    Rpc {
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GPIPE_ADMIN_TOKEN")]
        token_env: String,
        request: PathBuf,
    },
}

fn read_json<T: DeserializeOwned>(path: &PathBuf) -> Result<T, String> {
    let file = std::fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("JSON file exceeds 2 MiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn rpc(url: &str, token_env: &str, request: Request) -> Result<(), String> {
    let mut endpoint = reqwest::Url::parse(url).map_err(|error| error.to_string())?;
    let loopback = endpoint.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if (endpoint.scheme() != "https" && !(endpoint.scheme() == "http" && loopback))
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err("coordinator requires HTTPS (HTTP allowed only on loopback), without URL credentials/query/fragment".into());
    }
    endpoint.set_path("/v1/rpc");
    let secret = std::env::var(token_env)
        .map_err(|_| format!("missing token environment variable {token_env}"))?;
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .post(endpoint)
        .bearer_auth(secret)
        .json(&Envelope {
            version: PROTOCOL_VERSION,
            request,
        })
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    response
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("response exceeds 4 MiB; narrow inspection scope".into());
    }
    let response: Response = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if let Response::Error { message } = &response {
        return Err(message.clone());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&response).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn import_clippier(
    jobs: &PathBuf,
    recipes: &PathBuf,
    repository: String,
    revision: String,
    seed: Option<u64>,
) -> Result<(), String> {
    let jobs: serde_json::Value = read_json(jobs)?;
    let matrix = gpipe::clippier::MatrixPlan::from_jobs_json(&jobs.to_string(), seed)?;
    let lowered = matrix.lower(repository, revision, read_json(recipes)?)?;
    println!(
        "{}",
        serde_json::to_string_pretty(lowered.plan()).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn verify_clippier(jobs: &PathBuf, receipts: &PathBuf, seed: Option<u64>) -> Result<(), String> {
    let jobs: serde_json::Value = read_json(jobs)?;
    let matrix = gpipe::clippier::MatrixPlan::from_jobs_json(&jobs.to_string(), seed)?;
    let receipts: Vec<gpipe::clippier::Receipt> = read_json(receipts)?;
    matrix.verify_receipts(&receipts)?;
    println!("verified");
    Ok(())
}

#[allow(clippy::too_many_lines)] // Explicit one-arm-per-command CLI dispatch.
fn run() -> Result<(), String> {
    match Cli::parse().command {
        Command::Pipeline { command } => pipeline(command),
        Command::Serve { config, triggers } => {
            let config: ServerConfig = read_json(&config)?;
            let triggers = triggers.as_ref().map(read_json).transpose()?;
            actix_web::rt::System::new()
                .block_on(gpipe::server::serve_with_triggers(config, triggers))
        }
        Command::Worker { config, once } => {
            let config: WorkerConfig = read_json(&config)?;
            gpipe::worker::run(&config, once)
        }
        Command::ImportClippier {
            jobs,
            recipes,
            repository,
            revision,
            seed,
        } => import_clippier(&jobs, &recipes, repository, revision, seed),
        Command::RustRecipes { jobs, seed } => {
            let jobs: serde_json::Value = read_json(&jobs)?;
            let matrix = gpipe::clippier::MatrixPlan::from_jobs_json(&jobs.to_string(), seed)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&matrix.rust_validation_recipes()?)
                    .map_err(|error| error.to_string())?
            );
            Ok(())
        }
        Command::VerifyClippier {
            jobs,
            receipts,
            seed,
        } => verify_clippier(&jobs, &receipts, seed),
        Command::PrepareMoosicbox {
            jobs,
            profile,
            repository,
            revision,
            seed,
            clippier,
        } => {
            let prepared = gpipe::moosicbox::prepare(
                &read_json(&jobs)?,
                &repository,
                &revision,
                seed,
                &clippier,
                &read_json(&profile)?,
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&prepared).map_err(|error| error.to_string())?
            );
            Ok(())
        }
        Command::VerifyMoosicbox {
            prepared,
            inspection,
        } => {
            let verified =
                gpipe::moosicbox::verify(&read_json(&prepared)?, &read_json(&inspection)?)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&verified).map_err(|error| error.to_string())?
            );
            Ok(())
        }
        Command::VerifyCiRun {
            prepared,
            url,
            token_env,
            run_id,
        } => {
            let client = gpipe::operator::OperatorClient::new(&url, &token_env)?;
            let verified = client.verify_ci(&read_json(&prepared)?, &run_id)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&verified).map_err(|error| error.to_string())?
            );
            Ok(())
        }
        Command::Validate { plan } => {
            let plan: Plan = read_json(&plan)?;
            plan.validate()?;
            println!("valid");
            Ok(())
        }
        Command::Submit {
            url,
            token_env,
            key,
            dynamic,
            plan,
        } => rpc(
            &url,
            &token_env,
            Request::Submit {
                key,
                plan: read_json(&plan)?,
                dynamic,
            },
        ),
        Command::Inspect {
            url,
            token_env,
            run_id,
        } => rpc(&url, &token_env, Request::Inspect { run_id }),
        Command::List { url, token_env } => rpc(&url, &token_env, Request::List),
        Command::Cancel {
            url,
            token_env,
            run_id,
        } => rpc(&url, &token_env, Request::Cancel { run_id }),
        Command::Approve {
            url,
            token_env,
            repository,
            revision,
        } => rpc(
            &url,
            &token_env,
            Request::Approve {
                repository,
                revision,
            },
        ),
        Command::Expand {
            url,
            token_env,
            run_id,
            tasks,
        } => {
            let tasks: BTreeMap<String, Task> = read_json(&tasks)?;
            rpc(&url, &token_env, Request::Expand { run_id, tasks })
        }
        Command::Seal {
            url,
            token_env,
            run_id,
        } => rpc(&url, &token_env, Request::Seal { run_id }),
        Command::Rpc {
            url,
            token_env,
            request,
        } => rpc(&url, &token_env, read_json(&request)?),
    }
}

#[derive(Subcommand)]
enum PipelineCommand {
    /// Resolve a definition and configuration from an exact git revision.
    Plan {
        definition: PathBuf,
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        base_revision: String,
        #[arg(long)]
        clippier: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Deploy a loopback coordinator and one sequential compatible native worker.
    Run {
        prepared: PathBuf,
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        state: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Submit the exact prepared plan to an explicitly selected coordinator.
    Submit {
        prepared: PathBuf,
        #[arg(long)]
        url: String,
        #[arg(long)]
        token_env: String,
        #[arg(long)]
        key: String,
    },
    /// Retain attempt evidence and require authoritative successful CI verification.
    Verify {
        prepared: PathBuf,
        #[arg(long)]
        url: String,
        #[arg(long)]
        token_env: String,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        output: PathBuf,
    },
    #[command(name = "__host", hide = true)]
    Host {
        prepared: PathBuf,
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        state: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

fn pipeline(command: PipelineCommand) -> Result<(), String> {
    use gpipe::{operator::OperatorClient, pipeline};
    match command {
        PipelineCommand::Plan {
            definition,
            workspace,
            revision,
            base_revision,
            clippier,
            output,
        } => {
            pipeline::MoosicBoxClippierAdapter::plan(
                &definition,
                &workspace,
                &revision,
                &base_revision,
                &clippier,
                &output,
            )?;
            println!("{}", output.join("prepared.json").display());
            Ok(())
        }
        PipelineCommand::Run {
            prepared,
            workspace,
            state,
            output,
        } => pipeline::run(&prepared, &workspace, &state, &output),
        PipelineCommand::Submit {
            prepared,
            url,
            token_env,
            key,
        } => {
            let prepared = pipeline::load(&prepared)?;
            let client = OperatorClient::new(&url, &token_env)?;
            let run_id = pipeline::submit(&prepared, &client, &key)?;
            println!("{}", serde_json::json!({"run_id": run_id}));
            Ok(())
        }
        PipelineCommand::Verify {
            prepared,
            url,
            token_env,
            run_id,
            output,
        } => {
            let prepared = pipeline::load(&prepared)?;
            pipeline::verify(
                &prepared,
                &OperatorClient::new(&url, &token_env)?,
                &run_id,
                &output,
            )
        }
        PipelineCommand::Host {
            prepared,
            workspace,
            state,
            output,
        } => pipeline::host(&prepared, &workspace, &state, &output),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("gpipe: {error}");
        std::process::exit(1);
    }
}
