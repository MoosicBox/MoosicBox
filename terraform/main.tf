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

# Check if cluster exists using doctl
data "external" "cluster_check" {
  program = ["bash", "-c", <<-EOT
    export DIGITALOCEAN_ACCESS_TOKEN="${var.do_token}"
    if doctl kubernetes cluster get ${local.cluster_name} >/dev/null 2>&1; then
      echo '{"exists": "true"}'
    else
      echo '{"exists": "false"}'
    fi
  EOT
  ]
}

locals {
  # Check if cluster exists based on external check
  cluster_exists = data.external.cluster_check.result.exists == "true"
}

# Fetch existing cluster only if it exists
data "digitalocean_kubernetes_cluster" "existing" {
  count = local.cluster_exists ? 1 : 0
  name  = local.cluster_name
}

locals {
  # Use existing cluster if available, otherwise use the newly created one
  active_cluster = local.cluster_exists ? data.digitalocean_kubernetes_cluster.existing[0] : (
    length(digitalocean_kubernetes_cluster.cluster) > 0 ? digitalocean_kubernetes_cluster.cluster[0] : null
  )
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

# Write kubeconfig locally for kubectl provider
resource "local_file" "kubeconfig" {
  count    = local.active_cluster != null ? 1 : 0
  content  = local.active_cluster.kube_config[0].raw_config
  filename = "${path.module}/kubeconfig"
}

provider "kubernetes" {
  host  = local.active_cluster != null ? local.active_cluster.endpoint : ""
  token = local.active_cluster != null ? local.active_cluster.kube_config[0].token : ""
  cluster_ca_certificate = local.active_cluster != null ? base64decode(
    local.active_cluster.kube_config[0].cluster_ca_certificate
  ) : ""
}

provider "kubectl" {
  host  = local.active_cluster != null ? local.active_cluster.endpoint : ""
  token = local.active_cluster != null ? local.active_cluster.kube_config[0].token : ""
  cluster_ca_certificate = local.active_cluster != null ? base64decode(
    local.active_cluster.kube_config[0].cluster_ca_certificate
  ) : ""
  load_config_file = false
}

provider "docker" {
}

provider "helm" {
  kubernetes {
    host  = local.active_cluster != null ? local.active_cluster.endpoint : ""
    token = local.active_cluster != null ? local.active_cluster.kube_config[0].token : ""
    cluster_ca_certificate = local.active_cluster != null ? base64decode(
      local.active_cluster.kube_config[0].cluster_ca_certificate
    ) : ""
  }
}

# Get information about the Kubernetes node pool (only if cluster exists)
data "digitalocean_kubernetes_cluster" "cluster_info" {
  count = local.active_cluster != null ? 1 : 0
  name = local.cluster_name
  
  depends_on = [
    digitalocean_kubernetes_cluster.cluster
  ]
}

# Create a firewall for the Kubernetes cluster
resource "digitalocean_firewall" "kubernetes" {
  count = var.create_firewall ? 1 : 0
  name = "${local.cluster_name}-firewall"

  # Allow HTTP traffic
  inbound_rule {
    protocol         = "tcp"
    port_range       = "80"
    source_addresses = ["0.0.0.0/0", "::/0"]
  }

  # Allow NodePort traffic
  inbound_rule {
    protocol         = "tcp"
    port_range       = "30000-32767"
    source_addresses = ["0.0.0.0/0", "::/0"]
  }

  # Allow HTTPS traffic if using SSL
  dynamic "inbound_rule" {
    for_each = var.use_ssl ? [1] : []
    content {
      protocol         = "tcp"
      port_range       = "443"
      source_addresses = ["0.0.0.0/0", "::/0"]
    }
  }

  # Allow all outbound traffic
  outbound_rule {
    protocol              = "tcp"
    port_range           = "1-65535"
    destination_addresses = ["0.0.0.0/0", "::/0"]
  }

  outbound_rule {
    protocol              = "udp"
    port_range           = "1-65535"
    destination_addresses = ["0.0.0.0/0", "::/0"]
  }

  outbound_rule {
    protocol              = "icmp"
    destination_addresses = ["0.0.0.0/0", "::/0"]
  }

  # Tag the firewall with the cluster's default tag
  tags = local.active_cluster != null ? ["k8s:${local.active_cluster.id}"] : []
}
