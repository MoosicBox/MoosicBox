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

# First, try to read the existing secret
data "kubernetes_secret" "existing_registry_auth" {
  metadata {
    name = "registry-moosicbox"
  }
  depends_on = [local.active_cluster]
}

# Check for existing deployments using external data source
data "external" "check_deployments" {
  depends_on = [local.active_cluster]
  
  program = ["bash", "-c", <<-EOT
    tunnel_exists=$(kubectl get deployment ${local.resource_names.tunnel_server} -o name 2>/dev/null || echo "")
    lb_exists=$(kubectl get deployment ${local.resource_names.lb} -o name 2>/dev/null || echo "")
    echo "{\"tunnel_exists\": \"$tunnel_exists\", \"lb_exists\": \"$lb_exists\"}"
  EOT
  ]
}

locals {
  # Registry auth checks
  registry_secret_exists = can(data.kubernetes_secret.existing_registry_auth.metadata[0].name)
  registry_auth_name = local.registry_secret_exists ? data.kubernetes_secret.existing_registry_auth.metadata[0].name : kubernetes_secret.registry_auth[0].metadata[0].name
  
  # Use timestamp to create unique names for new deployments and services if existing ones are found
  timestamp = formatdate("YYYYMMDDhhmmss", timestamp())
  tunnel_server_name = "${local.resource_names.tunnel_server}-${local.timestamp}"
  load_balancer_name = "${local.resource_names.lb}-${local.timestamp}"
  tunnel_service_name = "${local.resource_names.tunnel_service}-${local.timestamp}"
  load_balancer_service_name = "${local.resource_names.lb}-service-${local.timestamp}"
  ingress_name = "${local.resource_names.ingress}-${local.timestamp}"
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
        }
      }

      spec {
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

          dynamic "volume_mount" {
            for_each = var.use_ssl ? [1] : []
            content {
              name       = "cert-volume"
              mount_path = "/etc/ssl/certs"
              read_only  = true
            }
          }

          dynamic "port" {
            for_each = var.use_ssl ? [80, 443] : [80]
            content {
              container_port = port.value
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
    type = "NodePort"

    selector = {
      app = local.resource_names.lb
    }

    dynamic "port" {
      for_each = var.use_ssl ? [
        { name = "http", port = 80 },
        { name = "https", port = 443 }
      ] : [
        { name = "http", port = 80 }
      ]
      content {
        name        = port.value.name
        port        = port.value.port
        target_port = port.value.port
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
      - http01:
          ingress:
            class: nginx
YAML

  depends_on = [
    helm_release.cert_manager,
    time_sleep.wait_for_cert_manager
  ]
}

resource "kubernetes_ingress_v1" "tunnel_server" {
  metadata {
    name = local.ingress_name
    annotations = var.use_ssl ? {
      "cert-manager.io/issuer" = local.resource_names.issuer
    } : {}
  }

  depends_on = [
    local.active_cluster,
    time_sleep.wait_for_cert_manager
  ]

  spec {
    ingress_class_name = "nginx"

    rule {
      host = local.domain
      http {
        path {
          path      = "/"
          path_type = "Prefix"
          backend {
            service {
              name = local.load_balancer_service_name
              port {
                number = 80
              }
            }
          }
        }
      }
    }

    dynamic "tls" {
      for_each = var.use_ssl ? [1] : []
      content {
        hosts       = [local.domain]
        secret_name = local.resource_names.cert
      }
    }
  }
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
