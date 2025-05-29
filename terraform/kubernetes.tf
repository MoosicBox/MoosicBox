# Install cert-manager namespace
resource "kubernetes_namespace" "cert_manager" {
  count = var.use_ssl ? 1 : 0

  metadata {
    name = "cert-manager"
  }

  depends_on = [
    local.active_cluster
  ]
}

# Install cert-manager
resource "helm_release" "cert_manager" {
  count = var.use_ssl ? 1 : 0

  name       = "cert-manager"
  repository = "https://charts.jetstack.io"
  chart      = "cert-manager"
  version    = "v1.13.3"
  namespace  = "cert-manager"

  timeout = 900 # 15 minutes

  set {
    name  = "installCRDs"
    value = "true"
  }

  set {
    name  = "startupapicheck.enabled"
    value = "false"  # Disable the startup API check
  }

  set {
    name  = "webhook.timeoutSeconds"
    value = "30"
  }

  set {
    name  = "ingressShim.defaultIssuerName"
    value = local.resource_names.issuer
  }

  set {
    name  = "ingressShim.defaultIssuerKind"
    value = "Issuer"
  }

  set {
    name  = "webhook.hostNetwork"
    value = "true"
  }

  depends_on = [
    kubernetes_namespace.cert_manager
  ]
}

# Wait for cert-manager to be ready
resource "time_sleep" "wait_for_cert_manager" {
  count = var.use_ssl ? 1 : 0

  depends_on = [helm_release.cert_manager]
  create_duration = "120s"  # Increase to 2 minutes
}

# First, try to read the existing secret (only if cluster exists)
data "kubernetes_secret" "existing_registry_auth" {
  count = local.active_cluster != null ? 1 : 0
  
  metadata {
    name = "registry-moosicbox"
  }
  depends_on = [local.active_cluster]
}

# Check for existing deployments using external data source
data "external" "check_deployments" {
  count = local.active_cluster != null ? 1 : 0
  
  depends_on = [local.active_cluster]
  
  program = ["bash", "-c", <<-EOT
    tunnel_exists=$(kubectl get deployment ${local.resource_names.tunnel_server} -o name 2>/dev/null || echo "")
    lb_exists=$(kubectl get deployment ${local.resource_names.lb} -o name 2>/dev/null || echo "")
    echo "{\"tunnel_exists\": \"$tunnel_exists\", \"lb_exists\": \"$lb_exists\"}"
  EOT
  ]
}

locals {
  # Registry auth checks (handle both missing cluster and missing secret)
  registry_secret_exists = local.active_cluster != null && length(data.kubernetes_secret.existing_registry_auth) > 0 ? can(data.kubernetes_secret.existing_registry_auth[0].metadata[0].name) : false
  registry_auth_name = local.registry_secret_exists ? data.kubernetes_secret.existing_registry_auth[0].metadata[0].name : (length(kubernetes_secret.registry_auth) > 0 ? kubernetes_secret.registry_auth[0].metadata[0].name : "registry-moosicbox")
  
  # Use consistent names without timestamps to avoid creating multiple deployments
  tunnel_server_name = local.resource_names.tunnel_server
  load_balancer_name = local.resource_names.lb
  tunnel_service_name = local.resource_names.tunnel_service
  load_balancer_service_name = "${local.resource_names.lb}-service"
  ingress_name = local.resource_names.ingress
}

resource "kubernetes_deployment" "tunnel_server" {
  metadata {
    name = local.tunnel_server_name
  }

  depends_on = [
    local.active_cluster,
    time_sleep.wait_for_cert_manager,
    kubernetes_secret.registry_auth
  ]

  spec {
    replicas = 1

    selector {
      match_labels = {
        app = local.resource_names.tunnel_server
      }
    }

    template {
      metadata {
        labels = {
          app = local.resource_names.tunnel_server
        }
      }

      spec {
        image_pull_secrets {
          name = local.registry_auth_name
        }

        container {
          name  = "tunnel-server"
          image = local.image_names.tunnel_server
          image_pull_policy = "Always"

          dynamic "env" {
            for_each = var.aws_access_key_id != "" && var.aws_secret_access_key != "" ? [1] : []
            content {
              name  = "AWS_ACCESS_KEY_ID"
              value = var.aws_access_key_id
            }
          }

          dynamic "env" {
            for_each = var.aws_access_key_id != "" && var.aws_secret_access_key != "" ? [1] : []
            content {
              name  = "AWS_SECRET_ACCESS_KEY"
              value = var.aws_secret_access_key
            }
          }

          port {
            container_port = 8004
          }
        }
      }
    }
  }
}

resource "kubernetes_service" "tunnel_server" {
  metadata {
    name = local.tunnel_service_name
  }

  depends_on = [
    local.active_cluster,
    kubernetes_deployment.tunnel_server
  ]

  spec {
    selector = {
      app = local.resource_names.tunnel_server
    }

    port {
      port        = 8004
      target_port = 8004
    }
  }
}

resource "kubernetes_deployment" "load_balancer" {
  metadata {
    name = local.load_balancer_name
  }

  depends_on = [
    local.active_cluster,
    time_sleep.wait_for_cert_manager,
    kubernetes_secret.registry_auth
  ]

  spec {
    replicas = 1

    selector {
      match_labels = {
        app = local.resource_names.lb
      }
    }

    template {
      metadata {
        labels = {
          app = local.resource_names.lb
          "io.kompose.service" = "moosicbox-tunnel-server-lb"
        }
      }

      spec {
        # Use hostNetwork to bind directly to host ports 80/443
        host_network = true
        
        image_pull_secrets {
          name = local.registry_auth_name
        }

        dynamic "volume" {
          for_each = var.use_ssl ? [1] : []
          content {
            name = "cert-volume"
            secret {
              secret_name = local.resource_names.cert
            }
          }
        }

        container {
          name  = "load-balancer"
          image = local.image_names.load_balancer
          image_pull_policy = "Always"

          env {
            name  = "CLUSTERS"
            value = local.clusters
          }
          
          # Add port configuration environment variables
          env {
            name  = "HTTP_PORT"
            value = "80"
          }
          
          dynamic "env" {
            for_each = var.use_ssl ? [1] : []
            content {
              name  = "HTTPS_PORT"
              value = "443"
            }
          }

          dynamic "volume_mount" {
            for_each = var.use_ssl ? [1] : []
            content {
              name       = "cert-volume"
              mount_path = "/etc/ssl/certs"
              read_only  = true
            }
          }

          port {
            container_port = 80      # Must match hostPort when using hostNetwork
            host_port     = 80
            protocol      = "TCP"
          }
          
          dynamic "port" {
            for_each = var.use_ssl ? [1] : []
            content {
              container_port = 443   # Must match hostPort when using hostNetwork
              host_port     = 443
              protocol      = "TCP"
            }
          }
        }
      }
    }
  }
}

resource "kubernetes_service" "load_balancer" {
  metadata {
    name = local.load_balancer_service_name
  }

  depends_on = [
    local.active_cluster,
    kubernetes_deployment.load_balancer
  ]

  spec {
    type = "ClusterIP"  # With hostNetwork, pods are directly accessible on node IPs

    selector = {
      app = local.resource_names.lb
    }

    port {
      name        = "http"
      port        = 80
      target_port = 80
      protocol    = "TCP"
    }
    
    dynamic "port" {
      for_each = var.use_ssl ? [1] : []
      content {
        name        = "https"
        port        = 443
        target_port = 443
        protocol    = "TCP"
      }
    }
  }
}

resource "kubectl_manifest" "certificate" {
  count = var.use_ssl ? 1 : 0
  yaml_body = <<YAML
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: ${local.resource_names.cert}
spec:
  secretName: ${local.resource_names.cert}
  dnsNames:
    - ${local.domain}
  issuerRef:
    name: ${local.resource_names.issuer}
    kind: Issuer
YAML

  depends_on = [
    helm_release.cert_manager,
    time_sleep.wait_for_cert_manager
  ]
}

resource "kubectl_manifest" "issuer" {
  count = var.use_ssl ? 1 : 0
  yaml_body = <<YAML
apiVersion: cert-manager.io/v1
kind: Issuer
metadata:
  name: ${local.resource_names.issuer}
spec:
  acme:
    server: https://acme-v02.api.letsencrypt.org/directory
    email: admin@${var.domain_name}
    privateKeySecretRef:
      name: ${local.resource_names.issuer}-account-key
    solvers:
      - dns01:
          digitalocean:
            tokenSecretRef:
              name: digitalocean-dns
              key: access-token
YAML

  depends_on = [
    helm_release.cert_manager,
    time_sleep.wait_for_cert_manager
  ]
}

# Only create if it doesn't exist
resource "kubernetes_secret" "registry_auth" {
  count = local.registry_secret_exists ? 0 : 1
  metadata {
    name = "registry-moosicbox"
  }

  type = "kubernetes.io/dockerconfigjson"

  data = {
    ".dockerconfigjson" = jsonencode({
      auths = {
        "${var.registry_endpoint}" = {
          auth = base64encode("${var.do_token}:${var.do_token}")
        }
      }
    })
  }

  depends_on = [local.active_cluster]
}

# Secret for DNS-01 challenge with DigitalOcean
resource "kubernetes_secret" "digitalocean_dns" {
  count = var.use_ssl ? 1 : 0
  
  metadata {
    name = "digitalocean-dns"
  }

  type = "Opaque"

  data = {
    "access-token" = var.do_token
  }

  depends_on = [local.active_cluster]
}
