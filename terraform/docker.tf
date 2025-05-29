# Check if images already exist in registry
data "external" "check_load_balancer_image" {
  program = ["bash", "-c", <<-EOT
    export DIGITALOCEAN_ACCESS_TOKEN="${var.do_token}"
    doctl registry login >/dev/null 2>&1
    image_with_hash=${local.image_names.load_balancer}-${local.load_balancer_hash}
    if docker manifest inspect \$image_with_hash >/dev/null 2>&1; then
      echo '{"exists": "true", "hash_match": "true"}'
    elif docker manifest inspect ${local.image_names.load_balancer} >/dev/null 2>&1; then
      echo '{"exists": "true", "hash_match": "false"}'
    else
      echo '{"exists": "false", "hash_match": "false"}'
    fi
  EOT
  ]
}

data "external" "check_tunnel_server_image" {
  program = ["bash", "-c", <<-EOT
    export DIGITALOCEAN_ACCESS_TOKEN="${var.do_token}"
    doctl registry login >/dev/null 2>&1
    image_with_hash=${local.image_names.tunnel_server}-${local.tunnel_server_hash}
    if docker manifest inspect \$image_with_hash >/dev/null 2>&1; then
      echo '{"exists": "true", "hash_match": "true"}'
    elif docker manifest inspect ${local.image_names.tunnel_server} >/dev/null 2>&1; then
      echo '{"exists": "true", "hash_match": "false"}'
    else
      echo '{"exists": "false", "hash_match": "false"}'
    fi
  EOT
  ]
}

# Calculate source code hashes more precisely
locals {
  # Only track actual source files, not build artifacts or temp files
  load_balancer_files = fileset("${path.root}/../packages/load_balancer", "**/*.{js,ts,rs,go,py,json,yaml,yml,toml}")
  tunnel_server_files = fileset("${path.root}/../packages/tunnel_server", "**/*.{js,ts,rs,go,py,json,yaml,yml,toml}")
  
  load_balancer_hash = sha1(join("", concat(
    [for f in local.load_balancer_files: filesha1("${path.root}/../packages/load_balancer/${f}")],
    [filesha1("${path.root}/../packages/load_balancer/LoadBalancer.Dockerfile")]
  )))
  
  tunnel_server_hash = sha1(join("", concat(
    [for f in local.tunnel_server_files: filesha1("${path.root}/../packages/tunnel_server/${f}")],
    [filesha1("${path.root}/../packages/tunnel_server/TunnelServer.Dockerfile")]
  )))
  
  # Determine if we need to build
  should_build_load_balancer = data.external.check_load_balancer_image.result.exists != "true" || data.external.check_load_balancer_image.result.hash_match != "true"
  should_build_tunnel_server = data.external.check_tunnel_server_image.result.exists != "true" || data.external.check_tunnel_server_image.result.hash_match != "true"
}

# Build and push load balancer image only when needed
resource "null_resource" "load_balancer_image" {
  triggers = {
    source_hash = local.load_balancer_hash
    image_name  = local.image_names.load_balancer
    force_build = local.should_build_load_balancer
  }

  provisioner "local-exec" {
    command = <<-EOT
      set -e
      export DIGITALOCEAN_ACCESS_TOKEN="${var.do_token}"
      
      # Check if image exists with matching hash
      image_with_hash=${local.image_names.load_balancer}-${local.load_balancer_hash}
      if ! docker manifest inspect $image_with_hash >/dev/null 2>&1; then
        echo "Building load balancer image..."
        docker build \
          --platform linux/amd64 \
          --build-arg BUILDKIT_INLINE_CACHE=1 \
          --build-arg DOCKER_BUILDKIT=1 \
          -t ${local.image_names.load_balancer} \
          -t $image_with_hash \
          -f ${path.root}/../packages/load_balancer/LoadBalancer.Dockerfile \
          ${path.root}/..
        
        echo "Pushing load balancer image..."
        doctl registry login
        docker push ${local.image_names.load_balancer}
        docker push $image_with_hash
      else
        echo "Load balancer image with matching hash exists, skipping build"
      fi
    EOT
  }
}

# Build and push tunnel server image only when needed
resource "null_resource" "tunnel_server_image" {
  triggers = {
    source_hash = local.tunnel_server_hash
    image_name  = local.image_names.tunnel_server
    force_build = local.should_build_tunnel_server
  }

  provisioner "local-exec" {
    command = <<-EOT
      set -e
      export DIGITALOCEAN_ACCESS_TOKEN="${var.do_token}"
      
      # Check if image exists with matching hash
      image_with_hash=${local.image_names.tunnel_server}-${local.tunnel_server_hash}
      if ! docker manifest inspect $image_with_hash >/dev/null 2>&1; then
        echo "Building tunnel server image..."
        docker build \
          --platform linux/amd64 \
          --build-arg BUILDKIT_INLINE_CACHE=1 \
          --build-arg DOCKER_BUILDKIT=1 \
          -t ${local.image_names.tunnel_server} \
          -t $image_with_hash \
          -f ${path.root}/../packages/tunnel_server/TunnelServer.Dockerfile \
          ${path.root}/..
        
        echo "Pushing tunnel server image..."
        doctl registry login
        docker push ${local.image_names.tunnel_server}
        docker push $image_with_hash
      else
        echo "Tunnel server image with matching hash exists, skipping build"
      fi
    EOT
  }
} 
