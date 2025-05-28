resource "docker_image" "load_balancer" {
  name = local.image_names.load_balancer

  build {
    context    = "${path.root}/.."
    dockerfile = "packages/load_balancer/LoadBalancer.Dockerfile"
    platform   = "linux/amd64"
    build_args = {
      BUILDKIT_INLINE_CACHE = "1"
      DOCKER_BUILDKIT = "1"
    }
    no_cache = false
    remove = true
    force_remove = true
  }

  triggers = {
    dir_sha1 = sha1(join("", [for f in fileset("${path.root}/../packages/load_balancer", "**"): filesha1("${path.root}/../packages/load_balancer/${f}")]))
  }

  provisioner "local-exec" {
    command = <<-EOT
      if [ -z "${var.do_token}" ]; then
        echo "Error: do_token is empty"
        exit 1
      fi
      doctl registry login
      docker push ${self.name}
    EOT
  }
}

resource "docker_image" "tunnel_server" {
  name = local.image_names.tunnel_server

  build {
    context    = "${path.root}/.."
    dockerfile = "packages/tunnel_server/TunnelServer.Dockerfile"
    platform   = "linux/amd64"
    build_args = {
      BUILDKIT_INLINE_CACHE = "1"
      DOCKER_BUILDKIT = "1"
    }
    no_cache = false
    remove = true
    force_remove = true
  }

  triggers = {
    dir_sha1 = sha1(join("", [for f in fileset("${path.root}/../packages/tunnel_server", "**"): filesha1("${path.root}/../packages/tunnel_server/${f}")]))
  }

  provisioner "local-exec" {
    command = <<-EOT
      if [ -z "${var.do_token}" ]; then
        echo "Error: do_token is empty"
        exit 1
      fi
      doctl registry login
      docker push ${self.name}
    EOT
  }
} 
