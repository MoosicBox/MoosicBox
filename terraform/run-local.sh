#!/usr/bin/env bash

# Check if DO token is provided
if [ -z "$DIGITALOCEAN_TOKEN" ]; then
    echo "Error: DIGITALOCEAN_TOKEN environment variable is not set"
    echo "Please set it first:"
    echo "export DIGITALOCEAN_TOKEN=your_token_here"
    exit 1
fi

# Export the token as both formats that Terraform might expect
export DIGITALOCEAN_ACCESS_TOKEN=$DIGITALOCEAN_TOKEN
export TF_VAR_do_token=$DIGITALOCEAN_TOKEN

# Debug information (only showing length for security)
echo "Token validation:"
echo "DIGITALOCEAN_TOKEN length: ${#DIGITALOCEAN_TOKEN}"
echo "DIGITALOCEAN_ACCESS_TOKEN length: ${#DIGITALOCEAN_ACCESS_TOKEN}"
echo "TF_VAR_do_token length: ${#TF_VAR_do_token}"

# Test the token with a simple API call
echo -e "\nTesting DigitalOcean API access..."
response=$(curl -s -X GET \
  -H "Authorization: Bearer $DIGITALOCEAN_TOKEN" \
  "https://api.digitalocean.com/v2/account")

if echo "$response" | grep -q "unauthorized"; then
    echo "Error: Token authentication failed!"
    echo "API Response:"
    echo "$response"
    exit 1
else
    echo "Token authentication successful!"
fi

# Initialize Terraform if .terraform directory doesn't exist
if [ ! -d ".terraform" ]; then
    echo -e "\nInitializing Terraform..."
    tofu init
fi

# Enable Terraform logging for debugging
export TF_LOG=DEBUG
export TF_LOG_PATH=./terraform.log

echo -e "\nRunning Terraform command..."
# Run the actual command
if [ "$1" == "plan" ] || [ -z "$1" ]; then
    tofu plan
elif [ "$1" == "apply" ]; then
    tofu apply
elif [ "$1" == "destroy" ]; then
    tofu destroy
else
    echo "Unknown command: $1"
    echo "Usage: $0 [plan|apply|destroy]"
    exit 1
fi

# If we get here and terraform.log exists, show relevant error messages
if [ -f "terraform.log" ]; then
    echo -e "\nRelevant error messages from terraform.log:"
    grep -i "error\|fail" terraform.log | tail -n 10
fi
