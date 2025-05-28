variable "stage" {
  description = "The deployment stage (e.g., prod, dev)"
  type        = string
}

variable "do_token" {
  description = "DigitalOcean API token"
  type        = string
  sensitive   = true
}

variable "cluster_name" {
  description = "Base name for the Kubernetes cluster"
  type        = string
  default     = "moosicbox"
}

variable "region" {
  description = "DigitalOcean region for the cluster"
  type        = string
  default     = "nyc1"
}

variable "kubernetes_version" {
  description = "Kubernetes version to use"
  type        = string
  default     = "1.31.6-do.1"
}

variable "node_size" {
  description = "Size of the worker nodes"
  type        = string
  default     = "s-2vcpu-4gb"
}

variable "node_count" {
  description = "Number of worker nodes"
  type        = number
  default     = 2
}

variable "aws_access_key_id" {
  description = "AWS Access Key ID"
  type        = string
  sensitive   = true
}

variable "aws_secret_access_key" {
  description = "AWS Secret Access Key"
  type        = string
  sensitive   = true
}

variable "registry_endpoint" {
  description = "The Docker registry endpoint"
  type        = string

  validation {
    condition     = length(var.registry_endpoint) > 0
    error_message = "The registry_endpoint variable must be set to your DigitalOcean registry URL (e.g., registry.digitalocean.com/your-registry-name)"
  }
}

variable "domain_name" {
  description = "Base domain name"
  type        = string
  default     = "moosicbox.com"
}

variable "use_ssl" {
  description = "Whether to use SSL"
  type        = bool
  default     = true
}

variable "extra_clusters" {
  description = "Additional clusters to configure"
  type        = string
  default     = ""
}

variable "create_firewall" {
  description = "Whether to create a DigitalOcean firewall (requires additional permissions)"
  type        = bool
  default     = false
}
