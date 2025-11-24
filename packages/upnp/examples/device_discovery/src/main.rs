#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! `UPnP` Device Discovery Example
//!
//! This example demonstrates how to discover `UPnP`/DLNA devices on your local network
//! using the `switchy_upnp` library.

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging to see what's happening
    env_logger::init();

    println!("Starting UPnP device discovery...\n");

    // Step 1: Scan the network for UPnP devices
    // This will broadcast a discovery request on the local network
    // and wait for devices to respond
    println!("Scanning for UPnP devices on the network...");
    switchy_upnp::scan_devices().await?;
    println!("Scan complete!\n");

    // Step 2: Retrieve the list of discovered devices
    // The devices are cached internally, so we can retrieve them without re-scanning
    let discovered_devices = switchy_upnp::devices().await;

    // Step 3: Display information about each discovered device
    if discovered_devices.is_empty() {
        println!("No UPnP devices found on the network.");
        println!("\nTroubleshooting tips:");
        println!("- Ensure UPnP devices are powered on and connected to the same network");
        println!("- Check that your firewall allows multicast traffic");
        println!("- Verify that UPnP is enabled on your devices");
        println!("- Make sure you're on the same subnet as the devices");
    } else {
        println!("Found {} UPnP device(s):\n", discovered_devices.len());

        for (index, device) in discovered_devices.iter().enumerate() {
            println!("Device #{}", index + 1);
            println!("  Name: {}", device.name);
            println!("  UDN (Unique Device Name): {}", device.udn);

            // Display volume if available
            if let Some(ref volume) = device.volume {
                println!("  Volume: {volume}");
            }

            // Display services offered by this device
            if device.services.is_empty() {
                println!("  Services: None");
            } else {
                println!("  Services:");
                for service in &device.services {
                    println!("    - {} ({})", service.id, service.r#type);
                }
            }

            println!();
        }

        // Step 4: Demonstrate retrieving a specific device by its UDN
        if let Some(first_device) = discovered_devices.first() {
            println!("Retrieving device by UDN: {}", first_device.udn);

            match switchy_upnp::get_device(&first_device.udn) {
                Ok(device) => {
                    println!("Successfully retrieved device from cache");
                    println!("  Device URL: {}", device.url());
                    println!("  Friendly Name: {}", device.friendly_name());
                    println!("  Model Name: {}", device.model_name());
                    println!("  Manufacturer: {}", device.manufacturer());
                }
                Err(e) => {
                    eprintln!("Error retrieving device: {e}");
                }
            }
            println!();
        }

        // Step 5: Demonstrate retrieving a specific service
        // Look for a device with an AVTransport service (common for media renderers)
        for device in &discovered_devices {
            if let Some(av_transport_service) = device
                .services
                .iter()
                .find(|s| s.id.contains("AVTransport"))
            {
                println!("Found AVTransport service on device: {}", device.name);
                println!("  Service ID: {}", av_transport_service.id);

                match switchy_upnp::get_service(&device.udn, &av_transport_service.id) {
                    Ok(service) => {
                        println!("  Successfully retrieved service from cache");
                        println!("  Service Type: {}", service.service_type());
                    }
                    Err(e) => {
                        eprintln!("  Error retrieving service: {e}");
                    }
                }
                println!();
                break;
            }
        }
    }

    println!("Example complete!");

    Ok(())
}
