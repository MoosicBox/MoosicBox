# Install cert-manager namespace
resource "kubernetes_namespace" "cert_manager" {
  count = var.use_ssl ? 1 : 0

  metadata {
    name = "cert-manager"
  }

  depends_on = [
    digitalocean_kubernetes_cluster.cluster
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

  set {
    name  = "installCRDs"
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
  create_duration = "60s"
}

resource "kubernetes_deployment" "tunnel_server" {
  metadata {
    name = local.resource_names.tunnel_server
  }

  depends_on = [
    digitalocean_kubernetes_cluster.cluster,
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
          name = kubernetes_secret.registry_auth.metadata[0].name
        }

        container {
          name  = "tunnel-server"
          image = docker_image.tunnel_server.name

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
    name = local.resource_names.tunnel_service
  }

  depends_on = [
    digitalocean_kubernetes_cluster.cluster
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
    name = local.resource_names.lb
  }

  depends_on = [
    digitalocean_kubernetes_cluster.cluster,
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
          name = kubernetes_secret.registry_auth.metadata[0].name
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
          image = docker_image.load_balancer.name

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
              host_port     = port.value
            }
          }
        }
      }
    }
  }
}

resource "kubernetes_service" "load_balancer" {
  metadata {
    name = local.resource_names.lb
  }

  depends_on = [
    digitalocean_kubernetes_cluster.cluster
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
    name = local.resource_names.ingress
    annotations = var.use_ssl ? {
      "cert-manager.io/issuer" = local.resource_names.issuer
    } : {}
  }

  depends_on = [
    digitalocean_kubernetes_cluster.cluster,
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
              name = kubernetes_service.load_balancer.metadata[0].name
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

# Create Docker registry secret
resource "kubernetes_secret" "registry_auth" {
  metadata {
    name = "registry-moosicbox"
  }

  type = "kubernetes.io/dockerconfigjson"

  data = {
    ".dockerconfigjson" = jsonencode({
      auths = {
        "registry.digitalocean.com" = {
          auth = base64encode("${var.do_token}:${var.do_token}")
        }
      }
    })
  }

  depends_on = [
    digitalocean_kubernetes_cluster.cluster
  ]
}
