# UPnP Device Discovery Example

Demonstrates how to discover UPnP/DLNA devices on your local network using the `switchy_upnp` library.

## Summary

This example shows the fundamental operation of scanning for UPnP devices on your network, retrieving information about discovered devices, and accessing cached device and service information. This is the first step required before controlling any UPnP device.

## What This Example Demonstrates

- Scanning the local network for UPnP/DLNA devices
- Retrieving the list of discovered devices from the cache
- Displaying device information (name, UDN, services)
- Retrieving a specific device by its Unique Device Name (UDN)
- Accessing device services like AVTransport
- Understanding the device and service caching mechanism

## Prerequisites

- A local network with UPnP/DLNA devices (e.g., smart TVs, media renderers, DLNA speakers, game consoles)
- Network configuration that allows multicast UDP traffic (required for SSDP discovery)
- Basic understanding of UPnP/DLNA architecture

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/upnp/examples/device_discovery/Cargo.toml
```

To see detailed logging output showing the discovery process:

```bash
RUST_LOG=debug cargo run --manifest-path packages/upnp/examples/device_discovery/Cargo.toml
```

## Expected Output

When UPnP devices are found on your network:

```
Starting UPnP device discovery...

Scanning for UPnP devices on the network...
Scan complete!

Found 2 UPnP device(s):

Device #1
  Name: Living Room TV
  UDN (Unique Device Name): uuid:12345678-1234-1234-1234-123456789012
  Services:
    - urn:upnp-org:serviceId:AVTransport (urn:schemas-upnp-org:service:AVTransport:1)
    - urn:upnp-org:serviceId:RenderingControl (urn:schemas-upnp-org:service:RenderingControl:1)
    - urn:upnp-org:serviceId:ConnectionManager (urn:schemas-upnp-org:service:ConnectionManager:1)

Device #2
  Name: Bedroom Speaker
  UDN (Unique Device Name): uuid:87654321-4321-4321-4321-210987654321
  Services:
    - urn:upnp-org:serviceId:AVTransport (urn:schemas-upnp-org:service:AVTransport:1)
    - urn:upnp-org:serviceId:RenderingControl (urn:schemas-upnp-org:service:RenderingControl:1)

Retrieving device by UDN: uuid:12345678-1234-1234-1234-123456789012
Successfully retrieved device from cache
  Device URL: http://192.168.1.100:8080/
  Friendly Name: Living Room TV
  Model Name: SmartTV XYZ
  Manufacturer: TV Manufacturer

Found AVTransport service on device: Living Room TV
  Service ID: urn:upnp-org:serviceId:AVTransport
  Successfully retrieved service from cache
  Service Type: urn:schemas-upnp-org:service:AVTransport:1

Example complete!
```

When no devices are found:

```
Starting UPnP device discovery...

Scanning for UPnP devices on the network...
Scan complete!

No UPnP devices found on the network.

Troubleshooting tips:
- Ensure UPnP devices are powered on and connected to the same network
- Check that your firewall allows multicast traffic
- Verify that UPnP is enabled on your devices
- Make sure you're on the same subnet as the devices

Example complete!
```

## Code Walkthrough

### Step 1: Scanning for Devices

```rust
switchy_upnp::scan_devices().await?;
```

This function broadcasts a UPnP discovery request using SSDP (Simple Service Discovery Protocol) on the local network. It waits for devices to respond and caches the discovered devices internally. The scan uses multicast UDP to discover devices.

### Step 2: Retrieving Discovered Devices

```rust
let discovered_devices = switchy_upnp::devices().await;
```

After scanning, retrieve the list of discovered devices from the internal cache. The cache persists for the lifetime of your application, so you don't need to re-scan unless you want to discover newly added devices.

### Step 3: Accessing Device Information

```rust
for device in &discovered_devices {
    println!("Name: {}", device.name);
    println!("UDN: {}", device.udn);

    for service in &device.services {
        println!("Service: {} ({})", service.id, service.r#type);
    }
}
```

Each discovered device contains:

- `name`: The friendly name of the device
- `udn`: Unique Device Name (UUID) that identifies the device
- `services`: List of UPnP services the device provides

### Step 4: Retrieving a Specific Device

```rust
match switchy_upnp::get_device(&first_device.udn) {
    Ok(device) => {
        println!("Device URL: {}", device.url());
        println!("Friendly Name: {}", device.friendly_name());
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

Use `get_device()` to retrieve a device from the cache by its UDN. This returns the full `rupnp::Device` object which provides additional methods for interacting with the device.

### Step 5: Retrieving a Specific Service

```rust
match switchy_upnp::get_service(&device.udn, &service_id) {
    Ok(service) => {
        println!("Service Type: {}", service.service_type());
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

Retrieve a specific service from a device using `get_service()`. Services are identified by their service ID (e.g., `urn:upnp-org:serviceId:AVTransport`). The returned service object can be used to invoke UPnP actions.

## Key Concepts

### UPnP Device Discovery (SSDP)

UPnP uses the Simple Service Discovery Protocol (SSDP) for device discovery. When you call `scan_devices()`, the library:

1. Sends a multicast UDP message to the SSDP multicast address (239.255.255.250:1900)
2. Waits for devices to respond with their description URLs
3. Fetches and parses device descriptions
4. Caches the device and service information

### Device Caching

The `switchy_upnp` library caches discovered devices and their services. This means:

- You only need to scan once per application run (unless you need to discover new devices)
- Subsequent `get_device()` and `get_service()` calls are fast lookups
- The cache persists until your application exits

### Common UPnP Services

- **AVTransport**: Controls media playback (play, pause, stop, seek)
- **RenderingControl**: Controls audio settings (volume, mute, equalizer)
- **ConnectionManager**: Manages connections between devices

### Unique Device Names (UDNs)

Each UPnP device has a UDN in the format `uuid:xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`. This identifier:

- Is unique and persistent for each device
- Should be used to identify devices across application runs
- Does not change even if the device's IP address changes

## Testing the Example

### On a Network with UPnP Devices

1. Ensure your UPnP devices (smart TVs, media renderers, etc.) are powered on
2. Make sure your computer and devices are on the same network
3. Run the example and verify devices are discovered
4. Check that services like AVTransport and RenderingControl are listed

### On a Network Without UPnP Devices

The example will run successfully but report no devices found. This is expected behavior.

### Verifying Network Configuration

If no devices are found but you expect them to be:

1. Check firewall settings - multicast UDP must be allowed
2. Verify devices are on the same subnet
3. Check that UPnP is enabled on your devices
4. Try running with `RUST_LOG=debug` to see detailed discovery logs

## Troubleshooting

### No Devices Found

**Problem**: `scan_devices()` completes but no devices are discovered.

**Solutions**:

- Verify UPnP devices are on and connected to the same network
- Check firewall settings - allow UDP port 1900 and multicast traffic
- Ensure UPnP/DLNA is enabled on your devices (check device settings)
- Verify you're on the same network subnet as the devices
- Try increasing the scan timeout (requires modifying the library)

### Network Permission Errors

**Problem**: Permission denied errors when trying to bind to multicast addresses.

**Solutions**:

- On Linux, you may need `CAP_NET_RAW` capability or run with elevated privileges
- Check that your network interface supports multicast
- Verify firewall rules allow multicast group membership

### Firewall Blocking Discovery

**Problem**: Some devices appear but not all expected devices.

**Solutions**:

- Configure your firewall to allow SSDP (UDP port 1900)
- Allow multicast traffic to 239.255.255.250
- Check for network segmentation (VLANs) separating devices

### Device Cache Issues

**Problem**: Device information seems outdated or incorrect.

**Solutions**:

- The cache persists for the application lifetime - restart to clear
- Devices that go offline may remain in cache until application restart
- To refresh device list, restart the application and re-scan

## Related Examples

This is currently the only example for `switchy_upnp`. Future examples may include:

- Media playback control (play, pause, stop, seek)
- Volume management
- Event subscriptions and notifications
- Device simulation for testing
