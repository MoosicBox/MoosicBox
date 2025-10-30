#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic `UPnP` device discovery example.
//!
//! This example demonstrates how to:
//! - Scan for `UPnP`/DLNA devices on the network
//! - List discovered devices and their services
//! - Get device and service objects for further control operations

use std::error::Error;
use switchy_upnp::{devices, get_device_and_service, scan_devices};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging to see what's happening
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    println!("=== UPnP Device Discovery Example ===\n");

    // Step 1: Scan the network for UPnP devices
    println!("Scanning for UPnP devices on the network...");
    println!("(This may take a few seconds)\n");

    scan_devices().await?;

    // Step 2: Get the list of discovered devices
    let discovered_devices = devices().await;

    if discovered_devices.is_empty() {
        println!("No UPnP devices found on the network.");
        println!("\nTroubleshooting tips:");
        println!("  - Ensure UPnP/DLNA devices are powered on and connected to the same network");
        println!("  - Check that multicast is enabled on your network interface");
        println!("  - Verify firewall settings allow UPnP discovery");
        return Ok(());
    }

    // Step 3: Display discovered devices and their services
    println!("Found {} UPnP device(s):\n", discovered_devices.len());

    for (idx, device) in discovered_devices.iter().enumerate() {
        println!("Device #{}", idx + 1);
        println!("  Name: {}", device.name);
        println!("  UDN:  {}", device.udn);

        if let Some(volume) = &device.volume {
            println!("  Volume: {volume}");
        }

        if !device.services.is_empty() {
            println!("  Services:");
            for service in &device.services {
                println!("    - {} ({})", service.id, service.r#type);
            }
        }

        println!();
    }

    // Step 4: Demonstrate how to get device and service objects for control operations
    println!("=== Device and Service Access ===\n");

    // Look for a device with AVTransport service
    let device_with_transport = discovered_devices
        .iter()
        .find(|device| device.services.iter().any(|s| s.id.contains("AVTransport")));

    if let Some(device) = device_with_transport {
        println!("Accessing device and service: {}\n", device.name);

        // Get the AVTransport service ID
        let av_transport_service_id = device
            .services
            .iter()
            .find(|s| s.id.contains("AVTransport"))
            .map_or("urn:upnp-org:serviceId:AVTransport", |s| s.id.as_str());

        // Access the device and service objects
        match get_device_and_service(&device.udn, av_transport_service_id) {
            Ok((upnp_device, service)) => {
                println!("✓ Successfully retrieved device and service objects");
                println!("  Device URL:  {}", upnp_device.url());
                println!("  Service ID:  {}", service.service_id());
                println!("  Service Type: {}", service.service_type());
                println!("\nThese objects can be used for control operations:");
                println!("  - play(&service, device_url, instance_id, speed)");
                println!("  - pause(&service, device_url, instance_id)");
                println!("  - stop(&service, device_url, instance_id)");
                println!("  - seek(&service, device_url, instance_id, unit, target)");
                println!("  - set_av_transport_uri(&service, device_url, ...)");
            }
            Err(e) => {
                println!("✗ Could not access device service: {e}");
            }
        }

        // Check for RenderingControl service (volume control)
        println!();
        let rendering_control_service_id = device
            .services
            .iter()
            .find(|s| s.id.contains("RenderingControl"))
            .map(|s| s.id.as_str());

        if let Some(service_id) = rendering_control_service_id {
            match get_device_and_service(&device.udn, service_id) {
                Ok((_upnp_device, service)) => {
                    println!("✓ Device also supports volume control");
                    println!("  Service ID: {}", service.service_id());
                    println!("\nVolume operations available:");
                    println!("  - get_volume(&service, device_url, instance_id, channel)");
                    println!("  - set_volume(&service, device_url, instance_id, channel, volume)");
                }
                Err(e) => {
                    println!("✗ Could not access RenderingControl service: {e}");
                }
            }
        } else {
            println!("✗ Device does not support volume control (no RenderingControl service)");
        }
    } else {
        println!("No devices found with AVTransport service.");
        println!("Devices may not support media playback control.");
    }

    println!("\n=== Example Complete ===");
    println!("\nNext steps:");
    println!("  - Use the device UDN to control playback with play(), pause(), stop()");
    println!("  - Set media URIs with set_av_transport_uri()");
    println!("  - Control volume with get_volume() and set_volume()");
    println!("  - Subscribe to device events with subscribe_events()");

    Ok(())
}
