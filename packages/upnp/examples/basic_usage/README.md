# Basic UPnP Device Discovery Example

A comprehensive example demonstrating how to discover UPnP/DLNA devices on your network and access their services using the `switchy_upnp` library.

## Summary

This example shows how to scan for UPnP devices on the local network, list discovered devices and their services, and retrieve device and service objects that can be used for control operations like playback and volume management.

## What This Example Demonstrates

- Scanning the network for UPnP/DLNA devices
- Retrieving and displaying the list of discovered devices
- Enumerating available services on each device (AVTransport, RenderingControl, etc.)
- Accessing device and service objects by UDN and service ID
- Understanding device capabilities and available control operations
- Error handling for device discovery and access

## Prerequisites

- A local network with UPnP/DLNA capable devices (such as smart speakers, media renderers, smart TVs, or streaming devices)
- Devices must be powered on and connected to the same network as your computer
- Network must allow multicast traffic for device discovery
- Firewall settings should permit UPnP/SSDP communication

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/upnp/examples/basic_usage/Cargo.toml
```

Or with logging enabled:

```bash
RUST_LOG=info cargo run --manifest-path packages/upnp/examples/basic_usage/Cargo.toml
```

## Expected Output

When UPnP devices are found on the network:

```
=== UPnP Device Discovery Example ===

Scanning for UPnP devices on the network...
(This may take a few seconds)

Found 2 UPnP device(s):

Device #1
  Name: Living Room Speaker
  UDN:  uuid:12345678-1234-1234-1234-123456789abc
  Services:
    - urn:upnp-org:serviceId:AVTransport (urn:schemas-upnp-org:service:AVTransport:1)
    - urn:upnp-org:serviceId:RenderingControl (urn:schemas-upnp-org:service:RenderingControl:1)

Device #2
  Name: Bedroom Renderer
  UDN:  uuid:87654321-4321-4321-4321-cba987654321
  Services:
    - urn:upnp-org:serviceId:AVTransport (urn:schemas-upnp-org:service:AVTransport:1)

=== Device and Service Access ===

Accessing device and service: Living Room Speaker

✓ Successfully retrieved device and service objects
  Device URL:  http://192.168.1.100:49152/
  Service ID:  urn:upnp-org:serviceId:AVTransport
  Service Type: urn:schemas-upnp-org:service:AVTransport:1

These objects can be used for control operations:
  - play(&service, device_url, instance_id, speed)
  - pause(&service, device_url, instance_id)
  - stop(&service, device_url, instance_id)
  - seek(&service, device_url, instance_id, unit, target)
  - set_av_transport_uri(&service, device_url, ...)

✓ Device also supports volume control
  Service ID: urn:upnp-org:serviceId:RenderingControl

Volume operations available:
  - get_volume(&service, device_url, instance_id, channel)
  - set_volume(&service, device_url, instance_id, channel, volume)

=== Example Complete ===

Next steps:
  - Use the device UDN to control playback with play(), pause(), stop()
  - Set media URIs with set_av_transport_uri()
  - Control volume with get_volume() and set_volume()
  - Subscribe to device events with subscribe_events()
```

If no devices are found:

```
=== UPnP Device Discovery Example ===

Scanning for UPnP devices on the network...
(This may take a few seconds)

No UPnP devices found on the network.

Troubleshooting tips:
  - Ensure UPnP/DLNA devices are powered on and connected to the same network
  - Check that multicast is enabled on your network interface
  - Verify firewall settings allow UPnP discovery
```

## Code Walkthrough

### Step 1: Scanning for Devices

```rust
scan_devices().await?;
```

This initiates a network scan using SSDP (Simple Service Discovery Protocol) to discover UPnP devices. The scan typically takes 3-5 seconds as it waits for device responses.

### Step 2: Retrieving Discovered Devices

```rust
let discovered_devices = devices().await;
```

After scanning, retrieve the cached list of discovered devices. Each device includes its name, unique device name (UDN), and list of available services.

### Step 3: Displaying Device Information

```rust
for device in discovered_devices.iter() {
    println!("Name: {}", device.name);
    println!("UDN:  {}", device.udn);
    for service in &device.services {
        println!("  - {} ({})", service.id, service.r#type);
    }
}
```

Iterate through devices and display their properties. Services typically include AVTransport (media playback control) and RenderingControl (volume control).

### Step 4: Accessing Device Services

```rust
match get_device_and_service(&device.udn, av_transport_service_id) {
    Ok((upnp_device, service)) => {
        let device_url = upnp_device.url();
        // Use device and service for control operations
    }
    Err(e) => println!("Could not access device: {e}"),
}
```

Use the device UDN and service ID to retrieve the device and service objects needed for control operations.

### Step 5: Understanding Available Operations

```rust
println!("These objects can be used for control operations:");
println!("  - play(&service, device_url, instance_id, speed)");
println!("  - pause(&service, device_url, instance_id)");
println!("  - stop(&service, device_url, instance_id)");
```

The example displays which control operations are available based on the services discovered. Once you have the device and service objects, you can call the appropriate control functions from the `switchy_upnp` library.

## Key Concepts

### UPnP Device Discovery

UPnP uses SSDP over multicast UDP to discover devices on the local network. Devices respond with their capabilities and service descriptions. The `switchy_upnp` library caches discovered devices for efficient access.

### Services and Actions

UPnP devices expose services (like AVTransport and RenderingControl), and each service provides actions (like Play, Pause, GetVolume). This example queries state using informational actions without modifying device state.

### Device UDN and Service IDs

- **UDN** (Unique Device Name): A UUID that uniquely identifies each device
- **Service ID**: Identifies a specific service on a device (e.g., `urn:upnp-org:serviceId:AVTransport`)

These identifiers are used throughout the API to specify which device and service to interact with.

### Instance IDs

Many UPnP actions take an instance ID parameter (typically `0`). This allows a single service to manage multiple virtual instances, though most devices only use instance 0.

## Testing the Example

1. **Without UPnP Devices**: The example gracefully handles the case when no devices are found and provides troubleshooting tips.

2. **With Non-Media Devices**: If devices are found but don't support AVTransport, the example will list them but skip the detailed information section.

3. **With Media Renderers**: When devices support media playback, you'll see detailed transport, position, and media information.

4. **Network Troubleshooting**: If devices aren't discovered:
    - Verify devices are on the same subnet
    - Check firewall allows UDP port 1900 (SSDP)
    - Ensure multicast is enabled on your network interface
    - Try disabling VPNs or network isolation features

## Troubleshooting

### No Devices Found

**Issue**: The scan completes but no devices are discovered.

**Solutions**:

- Ensure UPnP/DLNA devices are powered on and connected to the network
- Check that your computer and devices are on the same network subnet
- Verify multicast is enabled (some networks disable multicast for security)
- Temporarily disable firewall to test if it's blocking SSDP discovery
- Check router settings for UPnP/SSDP enabling

### Device Access Errors

**Issue**: Devices are discovered but accessing services fails.

**Solutions**:

- Verify the service ID is correct (check the device's service list)
- Ensure the device URL is accessible (try accessing it in a browser)
- Check if the device requires authentication
- Confirm the device hasn't changed IP addresses since discovery

### Compilation Errors

**Issue**: Example fails to compile.

**Solutions**:

- Ensure you're running from the repository root
- Verify all workspace dependencies are up to date: `cargo update`
- Check that the example is listed in the workspace `Cargo.toml`
- Try cleaning the build: `cargo clean` then rebuild

## Related Examples

This is currently the only example for the `switchy_upnp` package. Future examples may include:

- Media playback control (play, pause, stop, seek)
- Volume control operations
- Event subscription and handling
- Setting media URIs and metadata
- Advanced device filtering and selection

## Additional Resources

- [UPnP Device Architecture Specification](http://upnp.org/specs/arch/UPnP-arch-DeviceArchitecture-v2.0.pdf)
- [AVTransport Service Specification](http://upnp.org/specs/av/UPnP-av-AVTransport-v3-Service.pdf)
- [DLNA Guidelines](https://www.dlna.org/)
