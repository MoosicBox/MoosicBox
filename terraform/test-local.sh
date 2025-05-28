#!/usr/bin/env bash

# Default values
STAGE="dev"
SSL="true"
EXTRA_CLUSTERS=""
ACTION="plan"

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --stage)
      STAGE="$2"
      shift 2
      ;;
    --no-ssl)
      SSL="false"
      shift
      ;;
    --extra-clusters)
      EXTRA_CLUSTERS="$2"
      shift 2
      ;;
    --apply)
      ACTION="apply"
      shift
      ;;
    --destroy)
      ACTION="destroy"
      shift
      ;;
    --debug)
      DEBUG="true"
      shift
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Check if terraform.tfvars exists
if [ ! -f "terraform.tfvars" ]; then
  echo "Error: terraform.tfvars not found. Please create it from terraform.tfvars.example"
  exit 1
fi

# Source the tfvars file to get the variables in the environment
# Convert terraform.tfvars to shell format and source it
eval "$(sed -e 's/ *= */=/' terraform.tfvars | sed -e 's/^/export TF_VAR_/')"

# Export TF_WORKSPACE for consistency with CI/CD
export TF_WORKSPACE="$STAGE"

# Set variables from command line (these override tfvars)
export TF_VAR_stage="$STAGE"
export TF_VAR_use_ssl="$SSL"
export TF_VAR_extra_clusters="$EXTRA_CLUSTERS"

# Debug information
echo "Environment Setup:"
echo "Stage: $TF_VAR_stage"
echo "SSL Enabled: $TF_VAR_use_ssl"
echo "DO Token length: ${#TF_VAR_do_token}"
echo "Cluster Name: $TF_VAR_cluster_name"
echo "Registry Endpoint: $TF_VAR_registry_endpoint"

# Run Terraform
case $ACTION in
  "plan")
    tofu plan
    ;;
  "apply")
    tofu apply
    ;;
  "destroy")
    tofu destroy
    ;;
esac
