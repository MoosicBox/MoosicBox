# Cache Artifact 🚀

A super cool reusable GitHub Action that intelligently caches build artifacts based on directory checksums or git repository HEAD SHA. It runs custom commands only when changes are detected and efficiently manages artifacts to speed up your CI/CD pipelines.

## ✨ Features

- **🔍 Smart Change Detection**: Checksums directories or tracks git repository HEAD SHA with precision
- **📦 Dual Source Support**: Works with local directories or remote git repositories
- **⚡ Intelligent Caching**: Uses both GitHub Actions cache and artifacts for optimal performance
- **🎯 Conditional Execution**: Runs commands only when input changes are detected
- **🔧 Executable Setup**: Automatically makes output files executable and verifies them
- **📂 Path Expansion**: Handles tilde (~) and other shell expansions in paths
- **🚀 Artifact Management**: Automatically uploads/downloads artifacts with proper naming
- **🏷️ Flexible Naming**: Custom artifact names with sensible defaults
- **📊 Detailed Logging**: Beautiful output with emojis and comprehensive reporting

## 📋 Usage

### Basic Directory Example

```yaml
- name: Build with Cache Artifact
  id: build
  uses: ./cache-artifact # For local usage
  # uses: your-org/repo-name/cache-artifact@v1  # For published action
  with:
      directory: ./src
      command: cargo build --release
      output-path: ./target/release/my-app
```

### Repository Example

```yaml
- name: Install Tool from Repository
  id: install-tool
  uses: ./cache-artifact
  with:
      repo: https://github.com/owner/repo
      command: cargo install --git https://github.com/owner/repo tool-name
      output-path: ~/.cargo/bin/tool-name
      make-executable: true
      verify-command: --version
```

### Advanced Example

```yaml
- name: Build Rust Project
  id: rust-build
  uses: ./cache-artifact
  with:
      directory: ./packages/server
      command: |
          cargo build --release --bin server
          strip ./target/release/server
      output-path: ./target/release/server
      artifact-name: server-binary
      cache-key-prefix: my-project
      working-directory: ./
      shell: bash
      make-executable: true
      verify-command: --version

- name: Use the built binary
  run: |
      echo "Cache hit: ${{ steps.rust-build.outputs.cache-hit }}"
      echo "Checksum: ${{ steps.rust-build.outputs.checksum }}"
      echo "Artifact: ${{ steps.rust-build.outputs.artifact-name }}"
      ./target/release/server --version
```

## 🔧 Inputs

| Input               | Description                                                   | Required | Default               |
| ------------------- | ------------------------------------------------------------- | -------- | --------------------- |
| `directory`         | Directory to checksum for change detection                    | ❌\*     | -                     |
| `repo`              | Git repository URL to checksum for change detection           | ❌\*     | -                     |
| `command`           | Custom command to run when changes are detected               | ✅       | -                     |
| `output-path`       | Path to the output file/artifact created by the command       | ✅       | -                     |
| `artifact-name`     | Name for the artifact                                         | ❌       | `{source-name}-cache` |
| `cache-key-prefix`  | Prefix for cache keys                                         | ❌       | `{repository-name}`   |
| `working-directory` | Working directory to run the command in                       | ❌       | `.`                   |
| `shell`             | Shell to use for running commands                             | ❌       | `bash`                |
| `make-executable`   | Whether to make the output file executable                    | ❌       | `false`               |
| `verify-command`    | Command to run to verify the output works (e.g., "--version") | ❌       | -                     |

\*Either `directory` or `repo` must be provided (mutually exclusive)

## 📤 Outputs

| Output          | Description                                  |
| --------------- | -------------------------------------------- |
| `cache-hit`     | Whether the cache was hit (`true`/`false`)   |
| `checksum`      | The computed checksum of the directory/repo  |
| `artifact-name` | The name of the uploaded/downloaded artifact |

## 🎯 How It Works

### Directory Mode

1. **Checksum Calculation**: Computes a SHA256 hash of all files in the specified directory
2. **Cache Check**: Looks for existing cache entries using the checksum
3. **Artifact Download**: If cache misses, tries to download existing artifacts
4. **Command Execution**: Runs your custom command only if no cache/artifact exists
5. **Setup & Verification**: Makes output executable and verifies it works (if configured)
6. **Caching & Upload**: Saves results to both cache and artifacts for future use

### Repository Mode

1. **HEAD SHA Retrieval**: Uses `git ls-remote` to get the current HEAD SHA (efficient, no cloning)
2. **Cache Check**: Looks for existing cache entries using the HEAD SHA
3. **Artifact Download**: If cache misses, tries to download existing artifacts
4. **Command Execution**: Runs your custom command only if no cache/artifact exists
5. **Setup & Verification**: Makes output executable and verifies it works (if configured)
6. **Caching & Upload**: Saves results to both cache and artifacts for future use

## 🌟 Use Cases

### Rust Binary from Local Source

```yaml
- name: Build Rust Binary
  uses: ./cache-artifact
  with:
      directory: ./src
      command: cargo build --release
      output-path: ./target/release/my-app
      make-executable: true
      verify-command: --version
```

### Tool Installation from Git Repository

```yaml
- name: Install cargo-machete
  uses: ./cache-artifact
  with:
      repo: https://github.com/bstrie/cargo-machete
      command: cargo install --git https://github.com/bstrie/cargo-machete cargo-machete
      output-path: ~/.cargo/bin/cargo-machete
      make-executable: true
      verify-command: --version
```

### Docker Image Building

```yaml
- name: Build Docker Image
  uses: ./cache-artifact
  with:
      directory: ./docker-context
      command: |
          docker build -t my-app:latest .
          docker save my-app:latest > my-app.tar
      output-path: ./my-app.tar
```

### Frontend Build

```yaml
- name: Build Frontend
  uses: ./cache-artifact
  with:
      directory: ./frontend/src
      command: |
          npm ci
          npm run build
          tar -czf dist.tar.gz dist/
      output-path: ./dist.tar.gz
```

### Generated Code from Repository

```yaml
- name: Generate Code from Proto Repository
  uses: ./cache-artifact
  with:
      repo: https://github.com/company/proto-definitions
      command: |
          git clone https://github.com/company/proto-definitions temp-proto
          protoc --go_out=. --go_opt=paths=source_relative temp-proto/*.proto
          tar -czf generated.tar.gz *.pb.go
          rm -rf temp-proto
      output-path: ./generated.tar.gz
```

### Custom Binary Installation

```yaml
- name: Install Custom Tool
  uses: ./cache-artifact
  with:
      repo: https://github.com/owner/custom-tool
      command: |
          git clone https://github.com/owner/custom-tool
          cd custom-tool
          make install PREFIX=$HOME/.local
      output-path: ~/.local/bin/custom-tool
      make-executable: true
      verify-command: --help
```

## 🔄 Caching Strategy

The action uses a dual-layer caching approach:

1. **GitHub Actions Cache**: Fast access for the current workflow run
2. **Artifacts**: Persistent storage across workflow runs and branches

This ensures maximum performance while maintaining reliability across different scenarios.

## 🎨 Output Examples

### Directory Cache Hit

```
🎯 Cache HIT! Using cached version of ./target/release/server
📊 Summary:
  - Source: Directory ./packages/server
  - Checksum: a1b2c3d4e5f6...
  - Cache Key: my-repo-server-cache-a1b2c3d4e5f6...
  - Artifact: server-cache-a1b2c3d4e5f6...
  - Output: ./target/release/server
```

### Repository Cache Miss + Build

```
🚀 Running custom command: cargo install --git https://github.com/owner/repo tool
📂 Working directory: ./
✅ Command completed successfully, output created: /home/runner/.cargo/bin/tool
🔧 Making output file executable...
🧪 Verifying output file...
tool 1.0.0
✅ Output file verification successful
🔨 No existing cache or artifact found - built new version
📊 Summary:
  - Source: Repository https://github.com/owner/repo
  - Checksum: f6e5d4c3b2a1...
  - Cache Key: my-repo-repo-cache-f6e5d4c3b2a1...
  - Artifact: repo-cache-f6e5d4c3b2a1...
  - Output: /home/runner/.cargo/bin/tool
```

## 🚀 Performance Benefits

- **⚡ Skip Redundant Builds**: Only rebuild when source code changes
- **📦 Efficient Repository Tracking**: Uses git HEAD SHA without cloning
- **🔄 Cross-Workflow Caching**: Share artifacts between different workflow runs
- **📂 Path Expansion**: Handles complex paths with tilde expansion
- **🎯 Precise Detection**: SHA256 checksums and git SHA ensure accurate change detection

## 🛠️ Advanced Configuration

### Custom Shell with Verification

```yaml
- name: Build with PowerShell
  uses: ./cache-artifact
  with:
      directory: ./src
      command: |
          Write-Host "Building with PowerShell"
          dotnet build --configuration Release
      output-path: ./bin/Release/app.exe
      shell: pwsh
      make-executable: true
      verify-command: --version
```

### Repository with Custom Working Directory

```yaml
- name: Install from Repository
  uses: ./cache-artifact
  with:
      repo: https://github.com/owner/repo
      command: |
          git clone https://github.com/owner/repo source
          cd source && make install
      output-path: ./bin/tool
      working-directory: ./build
      make-executable: true
```

### Multiple Outputs with Path Expansion

```yaml
- name: Build Multiple Artifacts
  uses: ./cache-artifact
  with:
      directory: ./src
      command: |
          make all
          tar -czf outputs.tar.gz bin/ lib/ docs/
      output-path: ~/outputs.tar.gz # Tilde will be expanded
```

## 🔐 Security Considerations

- The action only runs commands you explicitly provide
- Repository URLs are accessed read-only via `git ls-remote`
- Artifacts are stored within your repository's GitHub Actions context
- Cache keys are scoped to your repository
- No sensitive data is logged or exposed
- Output verification ensures built artifacts work as expected

## 🆚 Directory vs Repository Mode

| Feature              | Directory Mode                   | Repository Mode                |
| -------------------- | -------------------------------- | ------------------------------ |
| **Change Detection** | SHA256 of all files in directory | Git HEAD SHA via ls-remote     |
| **Performance**      | Fast local file scanning         | Very fast remote SHA lookup    |
| **Use Case**         | Local source code                | External tools/dependencies    |
| **Disk Usage**       | None (files already local)       | None (no cloning required)     |
| **Network Usage**    | None                             | Minimal (just HEAD SHA)        |
| **Accuracy**         | Detects any file changes         | Detects any repository changes |
