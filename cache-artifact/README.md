# Cache Artifact 🚀

A super cool reusable GitHub Action that intelligently caches build artifacts based on directory checksums. It runs custom commands only when changes are detected and efficiently manages artifacts to speed up your CI/CD pipelines.

## ✨ Features

- **🔍 Smart Change Detection**: Checksums directories to detect changes with SHA256 accuracy
- **⚡ Intelligent Caching**: Uses both GitHub Actions cache and artifacts for optimal performance
- **🎯 Conditional Execution**: Runs commands only when input changes are detected
- **📦 Artifact Management**: Automatically uploads/downloads artifacts with proper naming
- **🔧 Fully Customizable**: Run any command with any inputs/outputs
- **🏷️ Flexible Naming**: Custom artifact names with sensible defaults
- **📊 Detailed Logging**: Beautiful output with emojis and comprehensive reporting

## 📋 Usage

### Basic Example

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

- name: Use the built binary
  run: |
      echo "Cache hit: ${{ steps.rust-build.outputs.cache-hit }}"
      echo "Checksum: ${{ steps.rust-build.outputs.checksum }}"
      echo "Artifact: ${{ steps.rust-build.outputs.artifact-name }}"
      ./target/release/server --version
```

## 🔧 Inputs

| Input               | Description                                             | Required | Default                  |
| ------------------- | ------------------------------------------------------- | -------- | ------------------------ |
| `directory`         | Directory to checksum for change detection              | ✅       | -                        |
| `command`           | Custom command to run when changes are detected         | ✅       | -                        |
| `output-path`       | Path to the output file/artifact created by the command | ✅       | -                        |
| `artifact-name`     | Name for the artifact                                   | ❌       | `{directory-name}-cache` |
| `cache-key-prefix`  | Prefix for cache keys                                   | ❌       | `{repository-name}`      |
| `working-directory` | Working directory to run the command in                 | ❌       | `.`                      |
| `shell`             | Shell to use for running commands                       | ❌       | `bash`                   |

## 📤 Outputs

| Output          | Description                                  |
| --------------- | -------------------------------------------- |
| `cache-hit`     | Whether the cache was hit (`true`/`false`)   |
| `checksum`      | The computed checksum of the directory       |
| `artifact-name` | The name of the uploaded/downloaded artifact |

## 🎯 How It Works

1. **Checksum Calculation**: Computes a SHA256 hash of all files in the specified directory
2. **Cache Check**: Looks for existing cache entries using the checksum
3. **Artifact Download**: If cache misses, tries to download existing artifacts
4. **Command Execution**: Runs your custom command only if no cache/artifact exists
5. **Caching & Upload**: Saves results to both cache and artifacts for future use
6. **Reporting**: Provides detailed information about what happened

## 🌟 Use Cases

### Rust Binary Compilation

```yaml
- name: Build Rust Binary
  uses: ./cache-artifact
  with:
      directory: ./src
      command: cargo build --release
      output-path: ./target/release/my-app
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

### Generated Code

```yaml
- name: Generate Protobuf Code
  uses: ./cache-artifact
  with:
      directory: ./proto
      command: |
          protoc --go_out=. --go_opt=paths=source_relative *.proto
          tar -czf generated.tar.gz *.pb.go
      output-path: ./generated.tar.gz
```

### Machine Learning Models

```yaml
- name: Train Model
  uses: ./cache-artifact
  with:
      directory: ./training-data
      command: |
          python train_model.py
          tar -czf model.tar.gz model/
      output-path: ./model.tar.gz
```

## 🔄 Caching Strategy

The action uses a dual-layer caching approach:

1. **GitHub Actions Cache**: Fast access for the current workflow run
2. **Artifacts**: Persistent storage across workflow runs and branches

This ensures maximum performance while maintaining reliability across different scenarios.

## 🎨 Output Examples

### Cache Hit

```
🎯 Cache HIT! Using cached version of ./target/release/server
📊 Summary:
  - Directory: ./packages/server
  - Checksum: a1b2c3d4e5f6...
  - Cache Key: my-repo-server-cache-a1b2c3d4e5f6...
  - Artifact: server-cache-a1b2c3d4e5f6...
  - Output: ./target/release/server
```

### Cache Miss + Build

```
🚀 Running custom command: cargo build --release
📂 Working directory: ./
✅ Command completed successfully, output created: ./target/release/server
🔨 Built new version and cached it
📊 Summary:
  - Directory: ./packages/server
  - Checksum: f6e5d4c3b2a1...
  - Cache Key: my-repo-server-cache-f6e5d4c3b2a1...
  - Artifact: server-cache-f6e5d4c3b2a1...
  - Output: ./target/release/server
```

## 🚀 Performance Benefits

- **⚡ Skip Redundant Builds**: Only rebuild when source code changes
- **🔄 Cross-Workflow Caching**: Share artifacts between different workflow runs
- **📦 Efficient Storage**: Compressed artifacts with 30-day retention
- **🎯 Precise Detection**: SHA256 checksums ensure accurate change detection

## 🛠️ Advanced Configuration

### Custom Shell

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
```

### Multiple Outputs

```yaml
- name: Build Multiple Artifacts
  uses: ./cache-artifact
  with:
      directory: ./src
      command: |
          make all
          tar -czf outputs.tar.gz bin/ lib/ docs/
      output-path: ./outputs.tar.gz
```

## 🔐 Security Considerations

- The action only runs commands you explicitly provide
- Artifacts are stored within your repository's GitHub Actions context
- Cache keys are scoped to your repository
- No sensitive data is logged or exposed
