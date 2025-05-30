locals {
  domain = var.stage == "prod" ? "tunnel.${var.domain_name}" : "tunnel-${var.stage}.${var.domain_name}"

  registry_name = split("/", var.registry_endpoint)[1]

  image_names = {
    load_balancer = "registry.digitalocean.com/${local.registry_name}/load-balancer:latest"
    tunnel_server = "registry.digitalocean.com/${local.registry_name}/tunnel-server:latest"
  }

  resource_names = {
    tunnel_server = "moosicbox-tunnel-server-${var.stage}"
    tunnel_service = "moosicbox-tunnel-service-${var.stage}"
    lb = "moosicbox-tunnel-server-lb-${var.stage}"
    cert = "moosicbox-tunnel-server-cert-${var.stage}"
    issuer = "moosicbox-tunnel-server-issuer-${var.stage}"
    ingress = "moosicbox-tunnel-server-ingress-${var.stage}"
  }

  clusters = var.extra_clusters != "" ? "${local.domain}:${local.resource_names.tunnel_service}:8004,${var.extra_clusters}" : "${local.domain}:${local.resource_names.tunnel_service}:8004"
}
