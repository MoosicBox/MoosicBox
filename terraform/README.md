# MoosicBox Infrastructure

This directory contains the OpenTofu (Terraform-compatible) infrastructure configuration for MoosicBox's tunnel service and load balancer. The infrastructure is designed to be deployed to DigitalOcean Kubernetes clusters with support for multiple environments/stages.

## Overview

The infrastructure consists of:

- Kubernetes cluster on DigitalOcean
- Tunnel server deployment
- Load balancer deployment
- SSL certificate management (optional)
- Ingress configuration

## Prerequisites

- OpenTofu installed (`curl --proto '=https' --tlsv1.2 -fsSL https://get.opentofu.org/install-opentofu.sh | sh`)
- DigitalOcean API token
- Docker registry credentials
- AWS credentials (if using AWS services)

## Local Development

1. Copy the example variables file:

    ```bash
    cp terraform.tfvars.example terraform.tfvars
    ```

2. Edit `terraform.tfvars` with your values:

    ```hcl
    stage              = "dev"          # Environment stage (dev, prod, etc.)
    domain_name        = "moosicbox.com" # Your domain name
    use_ssl            = true           # Whether to use SSL

    # Cluster configuration
    cluster_name       = "moosicbox"    # Base name for your cluster
    region             = "nyc1"         # DigitalOcean region
    kubernetes_version = "1.28"         # Kubernetes version
    node_size          = "s-2vcpu-4gb"  # Size of worker nodes
    node_count         = 2              # Number of worker nodes

    # Add your sensitive values
    do_token           = "your-token"
    aws_access_key_id  = "your-key"
    # ... other sensitive values
    ```

3. Use the provided helper script:

    ```bash
    # Plan changes
    ./test-local.sh --stage dev

    # Apply changes
    ./test-local.sh --stage dev --apply

    # Destroy infrastructure
    ./test-local.sh --stage dev --destroy

    # Deploy without SSL
    ./test-local.sh --stage dev --no-ssl

    # Deploy with extra clusters
    ./test-local.sh --stage dev --extra-clusters "cluster1:service1:8004;cluster2:service2:8004"
    ```

## GitHub Actions Workflow

The infrastructure can be deployed automatically using the GitHub Actions workflow at `.github/workflows/terraform-tunnel-service.yml`.

### Workflow Inputs

- `stage`: Deployment stage (default: prod)
- `destroy`: Whether to destroy the infrastructure (default: false)
- `apply`: Whether to apply changes (default: true)
- `extra_clusters`: Additional clusters to configure
- `ssl`: Whether to use SSL (default: true)
- `force`: Whether to force apply changes (default: false)

### Required Secrets

The following secrets must be set in your GitHub repository:

- `DIGITALOCEAN_ACCESS_TOKEN`
- `AWS_ACCESS_KEY_ID`
- `AWS_SECRET_ACCESS_KEY`
- `REGISTRY_ENDPOINT`
- `REGISTRY_USERNAME`
- `REGISTRY_PASSWORD`

### Running the Workflow

1. Go to the "Actions" tab in your GitHub repository
2. Select "Terraform Tunnel Service"
3. Click "Run workflow"
4. Fill in the desired parameters
5. Click "Run workflow"

## Directory Structure

```
terraform/
├── main.tf              # Provider configurations
├── variables.tf         # Input variable definitions
├── locals.tf           # Local variable definitions
├── docker.tf           # Docker image configurations
├── kubernetes.tf       # Kubernetes resource definitions
├── manifests/          # Kubernetes manifest files
│   └── cert-manager.yaml  # Cert-manager CRD definitions
├── terraform.tfvars.example  # Example variables file
└── README.md           # This file
```

## SSL Configuration

SSL is handled through cert-manager and Let's Encrypt. When `use_ssl` is enabled:

1. Cert-manager CRDs are installed
2. An ACME Issuer is configured
3. SSL certificates are automatically provisioned
4. The load balancer and ingress are configured for HTTPS

## Troubleshooting

### Common Issues

1. **Authentication Errors**

    ```
    Error: Error listing Kubernetes clusters: GET https://api.digitalocean.com/v2/kubernetes/clusters: 401
    ```

    Solution: Check your DigitalOcean token is valid and has the correct permissions.

2. **Certificate Issues**
    ```
    Error: Failed to create certificate
    ```
    Solution: Ensure DNS is properly configured and the domain points to your cluster's IP.

### Debugging

1. Enable debug logging:

    ```bash
    export TF_LOG=DEBUG
    ```

2. Check the Terraform logs:
    ```bash
    cat terraform.log
    ```

## Contributing

1. Follow the editorconfig settings
2. Ensure no trailing whitespace
3. Use 4-space indentation
4. Add meaningful commit messages

## Security Notes

- Never commit `terraform.tfvars` or any files containing sensitive values
- Use environment variables or encrypted secrets for sensitive values
- Regularly rotate API tokens and credentials
