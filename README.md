# MoosicBox

**A self-hosted music platform and a workspace of reusable Rust audio, UI, and infrastructure libraries.**

MoosicBox serves a local music library, integrates music services, and provides clients for browsing and playback. Its supporting libraries include audio encoding/resampling, HyperChad UI rendering, Switchy infrastructure abstractions, and Simvar deterministic simulation testing.

![MoosicBox](https://github.com/MoosicBox/Files/blob/master/animation.gif?raw=true)

> **Active development.** Features and clients are experimental; this is not yet a generally supported production music service. Platform artifacts and feature availability vary by release.

[Downloads](https://moosicbox.com/download) · [Website](https://moosicbox.com) · [Issues](https://github.com/MoosicBox/MoosicBox/issues)

## Music platform

- Local library indexing, search, and streaming.
- Tidal, Qobuz, and YouTube integration code; availability depends on provider APIs, account access, and build features.
- Feature-gated AAC, FLAC, MP3, and Opus encoding, decoding, and resampling infrastructure.
- Playback sessions, audio zones, and multiple output devices.
- Web, Tauri desktop, and HyperChad-based client implementations. Android work is experimental; do not infer mobile support from desktop artifacts.
- SQLite and PostgreSQL server configurations.

Service integrations may contact third parties. Self-hosting does not imply offline availability for streaming-service content or that every configuration keeps all data on one device.

## Try a client

Use the [download page](https://moosicbox.com/download) and select the platform and release variant deliberately. Client-only and bundled variants are not interchangeable. The presence of an artifact does not certify its compatibility with every OS version or current source revision.

## Run a local server

### Prerequisites

Install Git, stable Rust, and a native compiler toolchain. The default server includes native audio dependencies; Rust alone is insufficient on a clean machine.

- **macOS:** Xcode Command Line Tools, `pkg-config`, Autoconf, Automake, and Libtool for codec source builds; an installed Opus library can be discovered through pkg-config.
- **Linux:** compiler/build tools, pkg-config, codec development libraries/build tools, and ALSA development headers for the default audio-output feature.
- Optional image backends and other features add their own dependencies. See [server documentation](packages/server/README.md), package `clippier.toml` files, and the Nix development environment.

```sh
git clone https://github.com/MoosicBox/MoosicBox.git
cd MoosicBox
BIND_ADDR=127.0.0.1 PORT=8000 cargo run --locked -p moosicbox_server
```

Then open:

- `http://127.0.0.1:8000/health` to check the server response;
- `http://127.0.0.1:8000/admin` for the feature-gated administration interface.

The server root is not the listening application's web UI. Connect a separate MoosicBox client to the server for browsing/playback. The web application's source lives in [`app-website`](app-website).

**Keep this evaluation local.** The server otherwise defaults to `0.0.0.0`, and static-token authentication is an optional feature rather than a default protection. Review authentication, TLS/reverse-proxy configuration, network exposure, provider credentials, and backups before remote deployment. Do not expose an evaluation server directly to the internet.

## Reusable infrastructure

| Area                          | Starting point                                                                                         |
| ----------------------------- | ------------------------------------------------------------------------------------------------------ |
| Audio                         | [Encoder](packages/audio_encoder), [decoder](packages/audio_decoder), [resampling](packages/resampler) |
| UI                            | [HyperChad](packages/hyperchad)                                                                        |
| Real/simulated infrastructure | [Switchy](packages/switchy)                                                                            |
| Deterministic simulation      | [Simvar](packages/simvar), [server simulator](packages/server/simulator)                               |
| Databases                     | [Switchy database](packages/switchy/database), [schema](packages/switchy/schema)                       |
| CI tooling                    | [Clippier](packages/clippier)                                                                          |

Switchy provides feature-selected real and simulated infrastructure; Simvar orchestrates seeded workloads, controlled time/scheduling, and restartable hosts. These are reusable libraries, not guarantees that every application or external dependency is deterministic.

The workspace contains over 100 library packages. Obtain current membership from `cargo metadata --no-deps`; directories outside the workspace and examples should not be counted as independent reusable libraries.

### Database boundaries

The music server exposes SQLite/PostgreSQL configurations. The reusable database layer additionally implements MySQL and DuckDB backends. Library support does not imply those backends are supported end-to-end by the music server or its migrations.

## Development

```sh
cargo fmt
cargo test --locked -p simvar_harness --lib
cargo clippy --all-targets
```

Choose tests and features for the changed package; a workspace-wide all-features build can pull in multiple native, platform, and simulator backends. Follow [AGENTS.md](AGENTS.md) for repository conventions. CI uses package feature matrices; inspect the affected package's manifest and `clippier.toml` rather than assuming default features cover every implementation.

For bugs, include the revision/release, platform, client/server variants, build features, and a minimal non-sensitive reproduction. Never post streaming credentials, tokens, or private music-library metadata. Report security-sensitive issues to [bradensteffaniak@gmail.com](mailto:bradensteffaniak@gmail.com).

## Workspace reference

The [package catalog](packages/README.md) is a navigation aid. Package manifests and implementations remain authoritative for features, maturity, and installation.

## License

[Mozilla Public License 2.0](LICENSE). Dependencies and bundled third-party assets retain their own licenses.
