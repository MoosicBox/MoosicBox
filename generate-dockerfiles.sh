#!/usr/bin/env bash

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Show help message
show_help() {
    echo "Usage: $0 [PACKAGE_NAME]"
    echo
    echo "Generate Dockerfiles for MoosicBox packages."
    echo
    echo "Arguments:"
    echo "  PACKAGE_NAME    Optional. Generate Dockerfile only for the specified package."
    echo "                  If not provided, generates Dockerfiles for all packages."
    echo
    echo "Available packages:"
    echo "  - server"
    echo "  - load_balancer"
    echo "  - tunnel_server"
    echo
    echo "Examples:"
    echo "  $0                    # Generate all Dockerfiles"
    echo "  $0 load_balancer      # Generate only LoadBalancer.Dockerfile"
    echo "  $0 server             # Generate only Server.Dockerfile"
    echo "  $0 tunnel_server      # Generate only TunnelServer.Dockerfile"
    echo
}

# Utility functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Build clippier if needed
build_clippier() {
    log_info "Building clippier tool..."
    if ! cargo build --package clippier --release; then
        log_error "Failed to build clippier"
        exit 1
    fi
    log_success "Clippier built successfully"
}

# Generate dockerfile for a package
generate_dockerfile() {
    local package_name="$1"
    local port="$2"
    local features="$3"

    # Use proper naming convention
    local dockerfile_name
    case "$package_name" in
        "tunnel_server")
            dockerfile_name="TunnelServer.Dockerfile"
            ;;
        "load_balancer")
            dockerfile_name="LoadBalancer.Dockerfile"
            ;;
        "server")
            dockerfile_name="Server.Dockerfile"
            ;;
        *)
            # Default: capitalize first letter
            dockerfile_name="${package_name^}.Dockerfile"
            ;;
    esac

    local dockerfile_path="packages/${package_name}/${dockerfile_name}"

    log_info "Generating Dockerfile and dockerignore for $package_name..."

    # Create backup of existing Dockerfile if it exists
    if [[ -f "$dockerfile_path" ]]; then
        cp "$dockerfile_path" "${dockerfile_path}.backup.$(date +%Y%m%d_%H%M%S)"
        log_warning "Backed up existing Dockerfile to ${dockerfile_path}.backup.*"
    fi

    # Build the command
    local cmd="./target/release/clippier generate-dockerfile . moosicbox_${package_name} --output $dockerfile_path --port $port --build-args STATIC_TOKEN,WS_HOST,TUNNEL_ACCESS_TOKEN --env MAX_THREADS=64 --env ACTIX_WORKERS=32"
    if [[ -n "$features" ]]; then
        cmd="$cmd --features=$features"
    fi

    # Generate the Dockerfile and dockerignore
    if eval "$cmd"; then
        log_success "Generated Dockerfile: $dockerfile_path"
        log_success "Generated dockerignore: ${dockerfile_path%.*}.dockerignore"

        # Show package count reduction
        local cmd_normal="./target/release/clippier workspace-deps . moosicbox_${package_name}"
        local cmd_all="./target/release/clippier workspace-deps . moosicbox_${package_name} --all-potential-deps"

        if [[ -n "$features" ]]; then
            cmd_normal="$cmd_normal --features=$features"
            cmd_all="$cmd_all --features=$features"
        fi

        local dep_count_normal dep_count_all
        dep_count_normal=$(eval "$cmd_normal" | wc -l)
        dep_count_all=$(eval "$cmd_all" | wc -l)
        log_info "Dependencies: $dep_count_normal actual, $dep_count_all potential (+$((dep_count_all - dep_count_normal)) for Docker compatibility)"
        return 0
    else
        log_error "Failed to generate Dockerfile for $package_name"
        return 1
    fi
}

# Main function
main() {
    local target_package="${1:-}"

    # Handle help options
    if [[ "$target_package" == "-h" || "$target_package" == "--help" ]]; then
        show_help
        exit 0
    fi

    if [[ -n "$target_package" ]]; then
        log_info "Starting Dockerfile generation for specific package: $target_package"
    else
        log_info "Starting Dockerfile generation for MoosicBox packages"
    fi

    # Change to script directory
    cd "$(dirname "$0")"

    # Build clippier
    build_clippier

    # Package configurations: name:port:features
    declare -a packages=(
        "server:8010:cpal,format-flac,static-token-auth,all-apis,sqlite"
        "load_balancer:8011:"
        "tunnel_server:8012:"
    )

    # Filter packages if a specific target is provided
    if [[ -n "$target_package" ]]; then
        declare -a filtered_packages=()
        local found=false

        for package_config in "${packages[@]}"; do
            IFS=':' read -r name port features <<< "$package_config"
            if [[ "$name" == "$target_package" ]]; then
                filtered_packages+=("$package_config")
                found=true
                break
            fi
        done

        if [[ "$found" == false ]]; then
            log_error "Package '$target_package' not found. Available packages:"
            for package_config in "${packages[@]}"; do
                IFS=':' read -r name port features <<< "$package_config"
                echo "  - $name"
            done
            exit 1
        fi

        packages=("${filtered_packages[@]}")
    fi

    log_info "Generating Dockerfiles for ${#packages[@]} package(s)..."

    local success_count=0
    local total_count=${#packages[@]}

    for package_config in "${packages[@]}"; do
        IFS=':' read -r name port features <<< "$package_config"

        set +e  # Temporarily disable exit on error
        generate_dockerfile "$name" "$port" "$features"
        local result=$?
        set -e  # Re-enable exit on error

        if [[ $result -eq 0 ]]; then
            success_count=$((success_count + 1))
        fi
        echo # blank line for readability
    done

    # Summary
    if [[ $success_count -eq $total_count ]]; then
        if [[ -n "$target_package" ]]; then
            log_success "Dockerfile for $target_package generated successfully!"
        else
            log_success "All $total_count Dockerfiles generated successfully!"
        fi
    else
        log_warning "Generated $success_count/$total_count Dockerfiles successfully"
    fi

    # Show what was generated
    log_info "Generated files:"
    for package_config in "${packages[@]}"; do
        IFS=':' read -r name port features <<< "$package_config"

        # Use proper naming convention
        local dockerfile_name
        case "$name" in
            "tunnel_server")
                dockerfile_name="TunnelServer.Dockerfile"
                ;;
            "load_balancer")
                dockerfile_name="LoadBalancer.Dockerfile"
                ;;
            "server")
                dockerfile_name="Server.Dockerfile"
                ;;
            *)
                dockerfile_name="${name^}.Dockerfile"
                ;;
        esac

        local dockerfile_path="packages/${name}/${dockerfile_name}"
        local dockerignore_path="packages/${name}/${dockerfile_name%.*}.dockerignore"

        if [[ -f "$dockerfile_path" ]]; then
            echo "  ✓ $dockerfile_path"
            if [[ -f "$dockerignore_path" ]]; then
                echo "  ✓ $dockerignore_path"
            else
                echo "  ✗ $dockerignore_path (MISSING)"
            fi
        else
            echo "  ✗ $dockerfile_path (FAILED)"
        fi
    done

    log_success "Dockerfile generation complete!"
    log_info "You can now build optimized Docker images with minimal dependencies"
    echo

    if [[ -n "$target_package" ]]; then
        # Show specific example for the target package
        local dockerfile_name
        case "$target_package" in
            "tunnel_server")
                dockerfile_name="TunnelServer.Dockerfile"
                ;;
            "load_balancer")
                dockerfile_name="LoadBalancer.Dockerfile"
                ;;
            "server")
                dockerfile_name="Server.Dockerfile"
                ;;
            *)
                dockerfile_name="${target_package^}.Dockerfile"
                ;;
        esac
        echo "Example usage:"
        echo "  docker build -f packages/${target_package}/${dockerfile_name} -t moosicbox-${target_package//_/-} ."
    else
        echo "Example usage:"
        echo "  docker build -f packages/server/Server.Dockerfile -t moosicbox-server ."
        echo "  docker build -f packages/load_balancer/LoadBalancer.Dockerfile -t moosicbox-load-balancer ."
        echo "  docker build -f packages/tunnel_server/TunnelServer.Dockerfile -t moosicbox-tunnel-server ."
    fi

    # Exit with error only if no Dockerfiles were generated
    if [[ $success_count -eq 0 ]]; then
        exit 1
    fi
}

# Run main function
main "$@"
