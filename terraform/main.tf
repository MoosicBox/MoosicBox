terraform {
  required_providers {
    digitalocean = {
      source  = "digitalocean/digitalocean"
      version = "~> 2.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.0"
    }
    docker = {
      source  = "kreuzwerker/docker"
      version = "~> 3.0"
    }
    kubectl = {
      source  = "gavinbunney/kubectl"
      version = "~> 1.14"
    }
    helm = {
      source  = "hashicorp/helm"
      version = "~> 2.0"
    }
  }

  required_version = ">= 0.13"
}

# Validate DO token is set
locals {
  token_is_set = var.do_token != "" ? true : file("ERROR: DigitalOcean token must be set")
  cluster_name = "${var.cluster_name}-${var.stage}"
}

provider "digitalocean" {
  token = var.do_token
}

# Try to fetch existing cluster
data "digitalocean_kubernetes_cluster" "existing" {
  count = try(data.digitalocean_kubernetes_cluster.existing[0].id, "") != "" ? 1 : 0
  name = local.cluster_name
}

locals {
  cluster_exists = length(data.digitalocean_kubernetes_cluster.existing) > 0
}

# Create a new Kubernetes cluster only if it doesn't exist
resource "digitalocean_kubernetes_cluster" "cluster" {
  count   = local.cluster_exists ? 0 : 1
  name    = local.cluster_name
  region  = var.region
  version = var.kubernetes_version

  node_pool {
    name       = "worker-pool"
    size       = var.node_size
    node_count = var.node_count

    labels = {
      environment = var.stage
    }
  }
}

locals {
  # Use existing cluster if available, otherwise use the newly created one
  active_cluster = local.cluster_exists ? data.digitalocean_kubernetes_cluster.existing[0] : digitalocean_kubernetes_cluster.cluster[0]
}

# Write kubeconfig locally for kubectl provider
resource "local_file" "kubeconfig" {
  content  = local.active_cluster.kube_config[0].raw_config
  filename = "${path.module}/kubeconfig"
}

provider "kubernetes" {
  host  = local.active_cluster.endpoint
  token = local.active_cluster.kube_config[0].token
  cluster_ca_certificate = base64decode(
    local.active_cluster.kube_config[0].cluster_ca_certificate
  )
}

provider "kubectl" {
  host  = local.active_cluster.endpoint
  token = local.active_cluster.kube_config[0].token
  cluster_ca_certificate = base64decode(
    local.active_cluster.kube_config[0].cluster_ca_certificate
  )
  load_config_file = false
}

provider "docker" {
}

provider "helm" {
  kubernetes {
    host  = local.active_cluster.endpoint
    token = local.active_cluster.kube_config[0].token
    cluster_ca_certificate = base64decode(
      local.active_cluster.kube_config[0].cluster_ca_certificate
    )
  }
}
