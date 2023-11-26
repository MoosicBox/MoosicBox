#!/usr/bin/env bash

SERVICE=$1
ECR_REPOSITORY_NAME=$2
AWS_ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)

docker compose build "$SERVICE"
docker tag "$SERVICE" "$AWS_ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com/$ECR_REPOSITORY_NAME"
aws ecr get-login-password --region us-east-1 | docker login --username AWS --password-stdin "$AWS_ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com"
docker push "$AWS_ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com/$ECR_REPOSITORY_NAME"
