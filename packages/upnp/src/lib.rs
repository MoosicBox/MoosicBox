//! UPnP/DLNA device discovery and control library.
//!
//! This crate provides functionality for discovering and controlling UPnP/DLNA devices
//! on the local network. It supports device scanning, media playback control, volume
//! management, and event subscriptions for `UPnP` `AVTransport` and `RenderingControl` services.
//!
//! # Features
//!
//! * `api` - Actix-web API endpoints for `UPnP` operations
//! * `listener` - Event listener service for monitoring `UPnP` device state changes
//! * `player` - `UPnP` player implementation for media playback
//! * `openapi` - OpenAPI/utoipa schema support
//! * `simulator` - Simulated `UPnP` devices for testing
//!
//! # Examples
//!
//! Scanning for `UPnP` devices on the network:
//!
//! ```rust,no_run
//! # use switchy_upnp::{scan_devices, devices};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Scan the network for UPnP devices
//! scan_devices().await?;
//!
//! // Get the list of discovered devices
//! let upnp_devices = devices().await;
//! for device in upnp_devices {
//!     println!("Found device: {} ({})", device.name, device.udn);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Controlling playback on a `UPnP` device:
//!
//! ```rust,no_run
//! # use switchy_upnp::{get_device_and_service, play, pause, set_volume};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let device_udn = "uuid:device-id";
//! # let service_id = "urn:upnp-org:serviceId:AVTransport";
//! // Get device and AVTransport service
//! let (device, service) = get_device_and_service(device_udn, service_id)?;
//! let url = device.url();
//!
//! // Start playback
//! play(&service, url, 0, 1.0).await?;
//!
//! // Pause playback
//! pause(&service, url, 0).await?;
//!
//! // Set volume to 50%
//! # let rendering_control_service = service;
//! set_volume(&rendering_control_service, url, 0, "Master", 50).await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions, clippy::struct_field_names)]

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "listener")]
pub mod listener;
#[cfg(feature = "player")]
pub mod player;

pub mod models;

mod scanner;

use async_recursion::async_recursion;
use futures::prelude::*;
use itertools::Itertools;
use models::{UpnpDevice, UpnpService};
pub use rupnp::{Device, DeviceSpec, Service, http::Uri, ssdp::SearchTarget};
use scanner::UpnpScanner;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, LazyLock},
    time::Duration,
};
use thiserror::Error;
use tokio::sync::Mutex;

mod cache {
    use std::{
        collections::BTreeMap,
        sync::{LazyLock, RwLock},
    };

    use rupnp::{Device, Service};

    use crate::ScanError;

    #[derive(Debug, Clone)]
    struct DeviceMapping {
        device: Device,
        services: BTreeMap<String, Service>,
    }

    static DEVICE_URL_MAPPINGS: LazyLock<RwLock<BTreeMap<String, DeviceMapping>>> =
        LazyLock::new(|| RwLock::new(BTreeMap::new()));

    static DEVICE_MAPPINGS: LazyLock<RwLock<BTreeMap<String, DeviceMapping>>> =
        LazyLock::new(|| RwLock::new(BTreeMap::new()));

    pub fn get_device_from_url(url: &str) -> Result<Device, ScanError> {
        Ok(DEVICE_MAPPINGS
            .read()
            .unwrap()
            .get(url)
            .ok_or_else(|| ScanError::DeviceUrlNotFound {
                device_url: url.to_string(),
            })?
            .device
            .clone())
    }

    pub fn get_device(udn: &str) -> Result<Device, ScanError> {
        Ok(DEVICE_MAPPINGS
            .read()
            .unwrap()
            .get(udn)
            .ok_or_else(|| ScanError::DeviceUdnNotFound {
                device_udn: udn.to_string(),
            })?
            .device
            .clone())
    }

    pub fn insert_device(device: Device) {
        DEVICE_URL_MAPPINGS.write().unwrap().insert(
            device.url().to_string(),
            DeviceMapping {
                device: device.clone(),
                services: BTreeMap::new(),
            },
        );
        DEVICE_MAPPINGS.write().unwrap().insert(
            device.udn().to_owned(),
            DeviceMapping {
                device,
                services: BTreeMap::new(),
            },
        );
    }

    pub fn get_service(device_udn: &str, service_id: &str) -> Result<Service, ScanError> {
        Ok(DEVICE_MAPPINGS
            .read()
            .unwrap()
            .get(device_udn)
            .ok_or_else(|| ScanError::DeviceUdnNotFound {
                device_udn: device_udn.to_string(),
            })?
            .services
            .get(service_id)
            .ok_or_else(|| ScanError::ServiceIdNotFound {
                service_id: service_id.to_string(),
            })?
            .clone())
    }

    pub fn get_device_and_service(
        device_udn: &str,
        service_id: &str,
    ) -> Result<(Device, Service), ScanError> {
        let devices = DEVICE_MAPPINGS.read().unwrap();
        let device = devices
            .get(device_udn)
            .ok_or_else(|| ScanError::DeviceUdnNotFound {
                device_udn: device_udn.to_string(),
            })?;
        let resp = (
            device.device.clone(),
            device
                .services
                .get(service_id)
                .ok_or_else(|| ScanError::ServiceIdNotFound {
                    service_id: service_id.to_string(),
                })?
                .clone(),
        );
        drop(devices);

        Ok(resp)
    }

    pub fn get_device_and_service_from_url(
        device_url: &str,
        service_id: &str,
    ) -> Result<(Device, Service), ScanError> {
        let devices = DEVICE_URL_MAPPINGS.read().unwrap();
        let device = devices
            .get(device_url)
            .ok_or_else(|| ScanError::DeviceUrlNotFound {
                device_url: device_url.to_string(),
            })?;
        let resp = (
            device.device.clone(),
            device
                .services
                .get(service_id)
                .ok_or_else(|| ScanError::ServiceIdNotFound {
                    service_id: service_id.to_string(),
                })?
                .clone(),
        );
        drop(devices);

        Ok(resp)
    }

    pub fn insert_service(device: &Device, service: &Service) {
        if let Some(device_mapping) = DEVICE_URL_MAPPINGS
            .write()
            .as_mut()
            .unwrap()
            .get_mut(device.url().to_string().as_str())
        {
            device_mapping
                .services
                .insert(service.service_id().to_owned(), service.clone());
        }
        if let Some(device_mapping) = DEVICE_MAPPINGS
            .write()
            .as_mut()
            .unwrap()
            .get_mut(device.udn())
        {
            device_mapping
                .services
                .insert(service.service_id().to_owned(), service.clone());
        }
    }
}

/// Retrieves a cached `UPnP` device by its unique device name (UDN).
///
/// # Errors
///
/// * If a `Device` is not found with the given `udn`
pub fn get_device(udn: &str) -> Result<Device, ScanError> {
    cache::get_device(udn)
}

/// Retrieves a cached `UPnP` service by device UDN and service ID.
///
/// # Errors
///
/// * If a `Service` is not found with the given `device_udn` and `service_id`
pub fn get_service(device_udn: &str, service_id: &str) -> Result<Service, ScanError> {
    cache::get_service(device_udn, service_id)
}

/// Retrieves a cached `UPnP` device and service by device UDN and service ID.
///
/// # Errors
///
/// * If a `Device` or `Service` is not found with the given `device_udn` and `service_id`
pub fn get_device_and_service(
    device_udn: &str,
    service_id: &str,
) -> Result<(Device, Service), ScanError> {
    cache::get_device_and_service(device_udn, service_id)
}

/// Retrieves a cached `UPnP` device by its URL.
///
/// # Errors
///
/// * If a `Device` is not found with the given `url`
pub fn get_device_from_url(url: &str) -> Result<Device, ScanError> {
    cache::get_device_from_url(url)
}

/// Retrieves a cached `UPnP` device and service by device URL and service ID.
///
/// # Errors
///
/// * If a `Device` or `Service` is not found with the given `device_url` and `service_id`
pub fn get_device_and_service_from_url(
    device_url: &str,
    service_id: &str,
) -> Result<(Device, Service), ScanError> {
    cache::get_device_and_service_from_url(device_url, service_id)
}

/// Errors that can occur when executing `UPnP` actions.
#[derive(Debug, Error)]
pub enum ActionError {
    /// Error parsing XML response from `UPnP` device.
    #[error(transparent)]
    Roxml(#[from] roxmltree::Error),
    /// Error from the underlying `UPnP` library.
    #[error(transparent)]
    Rupnp(#[from] rupnp::Error),
    /// Required property missing from `UPnP` action response.
    #[error("Missing property \"{0}\"")]
    MissingProperty(String),
}

/// Errors that can occur when scanning for `UPnP` devices and services.
#[derive(Debug, Error)]
pub enum ScanError {
    /// `RenderingControl` service not found on the device.
    #[error("Failed to find `RenderingControl` service")]
    RenderingControlNotFound,
    /// `MediaRenderer` service not found on the device.
    #[error("Failed to find MediaRenderer service")]
    MediaRendererNotFound,
    /// `UPnP` device with the specified UDN not found in cache.
    #[error("Failed to find UPnP Device device_udn={device_udn}")]
    DeviceUdnNotFound {
        /// The device UDN that was not found.
        device_udn: String,
    },
    /// `UPnP` device with the specified URL not found in cache.
    #[error("Failed to find UPnP Device device_url={device_url}")]
    DeviceUrlNotFound {
        /// The device URL that was not found.
        device_url: String,
    },
    /// `UPnP` service with the specified service ID not found on the device.
    #[error("Failed to find UPnP Service service_id={service_id}")]
    ServiceIdNotFound {
        /// The service ID that was not found.
        service_id: String,
    },
    /// Error from the underlying `UPnP` library.
    #[error(transparent)]
    Rupnp(#[from] rupnp::Error),
}

/// Converts a duration string in the format "HH:MM:SS" to seconds.
///
/// # Panics
///
/// * If the duration str is an invalid format
#[must_use]
pub fn str_to_duration(duration: &str) -> u32 {
    let time_components = duration
        .split(':')
        .map(str::parse)
        .collect::<Result<Vec<u32>, std::num::ParseIntError>>()
        .expect("Failed to parse time...");

    time_components[0] * 60 * 60 + time_components[1] * 60 + time_components[2]
}

/// Converts a duration in seconds to a string in the format "HH:MM:SS".
#[must_use]
pub fn duration_to_string(duration: u32) -> String {
    format!(
        "{:0>2}:{:0>2}:{:0>2}",
        (duration / 60) / 60,
        (duration / 60) % 60,
        duration % 60
    )
}

static DIDL_LITE_NS: &str = "urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/";
static UPNP_NS: &str = "urn:schemas-upnp-org:metadata-1-0/upnp/";
static DC_NS: &str = "http://purl.org/dc/elements/1.1/";
static SEC_NS: &str = "http://www.sec.co.kr/";

/// Sets the AV transport URI for a `UPnP` device with metadata.
///
/// # Errors
///
/// * If the action failed to execute
#[allow(clippy::too_many_arguments)]
pub async fn set_av_transport_uri(
    service: &Service,
    device_url: &Uri,
    instance_id: u32,
    transport_uri: &str,
    format: &str,
    title: Option<&str>,
    creator: Option<&str>,
    artist: Option<&str>,
    album: Option<&str>,
    original_track_number: Option<u32>,
    duration: Option<u32>,
    size: Option<u64>,
) -> Result<BTreeMap<String, String>, ActionError> {
    static BRACKET_WHITESPACE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r">\s+<").expect("Invalid Regex"));
    static BETWEEN_WHITESPACE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"\s{2,}").expect("Invalid Regex"));

    // Remove extraneous whitespace
    fn compress_xml(xml: &str) -> String {
        BETWEEN_WHITESPACE
            .replace_all(
                BRACKET_WHITESPACE.replace_all(xml.trim(), "><").as_ref(),
                " ",
            )
            .to_string()
            .replace(['\r', '\n'], "")
            .replace("\" >", "\">")
    }

    fn escape_xml(xml: &str) -> String {
        xml::escape::escape_str_attribute(xml).to_string()
    }

    let headers = "*";

    let transport_uri = xml::escape::escape_str_attribute(transport_uri);

    let metadata = format!(
        r#"
        <DIDL-Lite
            xmlns="{DIDL_LITE_NS}"
            xmlns:dc="{DC_NS}"
            xmlns:sec="{SEC_NS}"
            xmlns:upnp="{UPNP_NS}">
            <item id="0" parentID="-1" restricted="false">
                <upnp:class>object.item.audioItem.musicTrack</upnp:class>
                {title}
                {creator}
                {artist}
                {album}
                {original_track_number}
                <res{duration}{size} protocolInfo="http-get:*:audio/{format}:{headers}">{transport_uri}</res>
            </item>
        </DIDL-Lite>
        "#,
        title = title
            .map(xml::escape::escape_str_attribute)
            .map_or_else(String::new, |x| format!("<dc:title>{x}</dc:title>")),
        creator = creator
            .map(xml::escape::escape_str_attribute)
            .map_or_else(String::new, |x| format!("<dc:creator>{x}</dc:creator>")),
        artist = artist
            .map(xml::escape::escape_str_attribute)
            .map_or_else(String::new, |x| format!("<upnp:artist>{x}</upnp:artist>")),
        album = album
            .map(xml::escape::escape_str_attribute)
            .map_or_else(String::new, |x| format!("<upnp:album>{x}</upnp:album>")),
        original_track_number = original_track_number.map_or_else(String::new, |x| format!(
            "<upnp:originalTrackNumber>{x}</upnp:originalTrackNumber>"
        )),
        duration = duration.map_or_else(String::new, |x| format!(
            " duration=\"{}\"",
            duration_to_string(x)
        )),
        size = size.map_or_else(String::new, |x| format!(" size=\"{x}\"",)),
    );

    let metadata = escape_xml(&compress_xml(&metadata));

    let args = format!(
        r"
        <InstanceID>{instance_id}</InstanceID>
        <CurrentURI>{transport_uri}</CurrentURI>
        <CurrentURIMetaData>{metadata}</CurrentURIMetaData>
        "
    );
    let args = compress_xml(&args);
    log::debug!("set_av_transport_uri args={args}");

    Ok(service
        .action(device_url, "SetAVTransportURI", &args)
        .await?
        .into_iter()
        .collect())
}

/// Parsed track metadata from `UPnP` `DIDL-Lite` XML.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TrackMetadata {
    /// List of track metadata items parsed from the XML.
    items: Vec<TrackMetadataItem>,
}

/// A single track metadata item from `UPnP` `DIDL-Lite` XML.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TrackMetadataItem {
    /// `UPnP` class of the item (e.g., "object.item.audioItem.musicTrack").
    upnp_class: Option<String>,
    /// Artist name from `UPnP` metadata.
    upnp_artist: Option<String>,
    /// Album name from `UPnP` metadata.
    upnp_album: Option<String>,
    /// Original track number from `UPnP` metadata.
    upnp_original_track_number: Option<String>,
    /// Track title from Dublin Core metadata.
    dc_title: Option<String>,
    /// Creator name from Dublin Core metadata.
    dc_creator: Option<String>,
    /// Resource information for the track.
    res: TrackMetadataItemResource,
}

/// Resource information for a track metadata item.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TrackMetadataItemResource {
    /// Duration of the track in seconds.
    duration: Option<u32>,
    /// Protocol information describing the resource format.
    protocol_info: Option<String>,
    /// URI of the media resource.
    source: String,
}

// "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:sec=\"http://www.sec.co.kr/\" xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\">
//     <item id=\"0\" parentID=\"-1\" restricted=\"false\">
//         <upnp:class>object.item.audioItem.musicTrack</upnp:class>
//         <dc:title>Friday</dc:title>
//         <dc:creator>Rebecca Black</dc:creator>
//         <upnp:artist>Rebecca Black</upnp:artist>
//         <upnp:album>Friday</upnp:album>
//         <upnp:originalTrackNumber>1</upnp:originalTrackNumber>
//         <res duration=\"00:03:31\" protocolInfo=\"http-get:*:audio/flac:*\">http://192.168.254.137:8001/track?trackId=12911&amp;source=LIBRARY</res>
//     </item>
// </DIDL-Lite>"
fn parse_track_metadata(track_metadata: &str) -> Result<TrackMetadata, ActionError> {
    let doc = roxmltree::Document::parse(track_metadata)?;

    let items = doc
        .descendants()
        .filter(|x| x.tag_name().name().to_lowercase() == "item")
        .map(|x| {
            let upnp_class = x.descendants().find(|x| {
                x.tag_name().namespace().is_some_and(|n| n == UPNP_NS)
                    && x.tag_name().name().to_lowercase() == "class"
            });
            let upnp_artist = x.descendants().find(|x| {
                x.tag_name().namespace().is_some_and(|n| n == UPNP_NS)
                    && x.tag_name().name().to_lowercase() == "artist"
            });
            let upnp_album = x.descendants().find(|x| {
                x.tag_name().namespace().is_some_and(|n| n == UPNP_NS)
                    && x.tag_name().name().to_lowercase() == "album"
            });
            let upnp_original_track_number = x.descendants().find(|x| {
                x.tag_name().namespace().is_some_and(|n| n == UPNP_NS)
                    && x.tag_name().name().to_lowercase() == "originaltracknumber"
            });
            let dc_title = x.descendants().find(|x| {
                x.tag_name().namespace().is_some_and(|n| n == DC_NS)
                    && x.tag_name().name().to_lowercase() == "title"
            });
            let dc_creator = x.descendants().find(|x| {
                x.tag_name().namespace().is_some_and(|n| n == DC_NS)
                    && x.tag_name().name().to_lowercase() == "creator"
            });
            let res = x
                .descendants()
                .find(|x| {
                    x.tag_name().namespace().is_some_and(|n| n == DIDL_LITE_NS)
                        && x.tag_name().name().to_lowercase() == "res"
                })
                .ok_or_else(|| ActionError::MissingProperty("Missing res".into()))?;
            Ok(TrackMetadataItem {
                upnp_class: upnp_class.and_then(|x| x.text()).map(ToOwned::to_owned),
                upnp_artist: upnp_artist.and_then(|x| x.text()).map(ToOwned::to_owned),
                upnp_album: upnp_album.and_then(|x| x.text()).map(ToOwned::to_owned),
                upnp_original_track_number: upnp_original_track_number
                    .and_then(|x| x.text())
                    .map(ToOwned::to_owned),
                dc_title: dc_title.and_then(|x| x.text()).map(ToOwned::to_owned),
                dc_creator: dc_creator.and_then(|x| x.text()).map(ToOwned::to_owned),
                res: TrackMetadataItemResource {
                    duration: res.attribute("duration").map(str_to_duration),
                    protocol_info: res.attribute("protocolInfo").map(ToOwned::to_owned),
                    source: res
                        .text()
                        .ok_or_else(|| ActionError::MissingProperty("Missing res value".into()))?
                        .to_owned(),
                },
            })
        })
        .collect::<Result<Vec<_>, ActionError>>();

    Ok(TrackMetadata { items: items? })
}

/// `UPnP` `AVTransport` service transport information.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TransportInfo {
    /// Current transport status (e.g., "OK", "`ERROR_OCCURRED`").
    current_transport_status: String,
    /// Current transport state (e.g., "PLAYING", "`PAUSED_PLAYBACK`", "STOPPED").
    current_transport_state: String,
    /// Current playback speed (typically "1" for normal speed).
    current_speed: String,
}

/// Retrieves transport information from a `UPnP` `AVTransport` service.
///
/// # Errors
///
/// * If the action failed to execute
/// * If the transport info is missing the required properties
pub async fn get_transport_info(
    service: &Service,
    url: &Uri,
    instance_id: u32,
) -> Result<TransportInfo, ActionError> {
    let map = service
        .action(
            url,
            "GetTransportInfo",
            &format!("<InstanceID>{instance_id}</InstanceID>"),
        )
        .await?;

    Ok(TransportInfo {
        current_transport_status: map
            .get("CurrentTransportStatus")
            .ok_or(ActionError::MissingProperty(
                "CurrentTransportStatus".into(),
            ))?
            .clone(),
        current_transport_state: map
            .get("CurrentTransportState")
            .ok_or(ActionError::MissingProperty("CurrentTransportState".into()))?
            .clone(),
        current_speed: map
            .get("CurrentSpeed")
            .ok_or(ActionError::MissingProperty("TrackURI".into()))?
            .clone(),
    })
}

/// `UPnP` `AVTransport` service position information.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PositionInfo {
    /// Current track number in the playlist (1-based).
    track: u32,
    /// Relative playback position in seconds within the current track.
    rel_time: u32,
    /// Absolute playback position in seconds across the entire playlist.
    abs_time: u32,
    /// URI of the current track.
    track_uri: String,
    /// Metadata for the current track.
    track_metadata: TrackMetadata,
    /// Relative counter value.
    rel_count: u32,
    /// Absolute counter value.
    abs_count: u32,
    /// Total duration of the current track in seconds.
    track_duration: u32,
}

/// Retrieves position information from a `UPnP` `AVTransport` service.
///
/// # Errors
///
/// * If the action failed to execute
/// * If the position info is missing the required properties
pub async fn get_position_info(
    service: &Service,
    url: &Uri,
    instance_id: u32,
) -> Result<PositionInfo, ActionError> {
    let map = service
        .action(
            url,
            "GetPositionInfo",
            &format!("<InstanceID>{instance_id}</InstanceID>"),
        )
        .await?;

    Ok(PositionInfo {
        abs_time: str_to_duration(
            map.get("AbsTime")
                .ok_or(ActionError::MissingProperty("AbsTime".into()))?,
        ),
        rel_time: str_to_duration(
            map.get("RelTime")
                .ok_or(ActionError::MissingProperty("RelTime".into()))?,
        ),
        track_duration: str_to_duration(
            map.get("TrackDuration")
                .ok_or(ActionError::MissingProperty("TrackDuration".into()))?,
        ),
        abs_count: map
            .get("AbsCount")
            .ok_or(ActionError::MissingProperty("AbsCount".into()))?
            .parse::<u32>()
            .map_err(|e| ActionError::MissingProperty(format!("AbsCount (\"{e:?}\")")))?,
        rel_count: map
            .get("RelCount")
            .ok_or(ActionError::MissingProperty("RelCount".into()))?
            .parse::<u32>()
            .map_err(|e| ActionError::MissingProperty(format!("RelCount (\"{e:?}\")")))?,
        track: map
            .get("Track")
            .ok_or(ActionError::MissingProperty("Track".into()))?
            .parse::<u32>()
            .map_err(|e| ActionError::MissingProperty(format!("Track (\"{e:?}\")")))?,
        track_uri: map
            .get("TrackURI")
            .ok_or(ActionError::MissingProperty("TrackURI".into()))?
            .clone(),
        track_metadata: parse_track_metadata(
            map.get("TrackMetaData")
                .ok_or(ActionError::MissingProperty("TrackMetaData".into()))?,
        )?,
    })
}

/// Seeks to a specific position in the current media on a `UPnP` `AVTransport` service.
///
/// # Errors
///
/// * If the action failed to execute
pub async fn seek(
    service: &Service,
    url: &Uri,
    instance_id: u32,
    unit: &str,
    target: u32,
) -> Result<BTreeMap<String, String>, ActionError> {
    let target_str = duration_to_string(target);
    log::trace!("seek: seeking to target={target_str} instance_id={instance_id} unit={unit}");

    Ok(service
        .action(
            url,
            "Seek",
            &format!(
                r"
                <InstanceID>{instance_id}</InstanceID>
                <Unit>{unit}</Unit>
                <Target>{target_str}</Target>
                "
            ),
        )
        .await?
        .into_iter()
        .collect())
}

/// Retrieves the volume from a `UPnP` `RenderingControl` service.
///
/// # Errors
///
/// * If the action failed to execute
pub async fn get_volume(
    service: &Service,
    url: &Uri,
    instance_id: u32,
    channel: &str,
) -> Result<BTreeMap<String, String>, ActionError> {
    Ok(service
        .action(
            url,
            "GetVolume",
            &format!("<InstanceID>{instance_id}</InstanceID><Channel>{channel}</Channel>"),
        )
        .await?
        .into_iter()
        .collect())
}

/// Sets the volume on a `UPnP` `RenderingControl` service.
///
/// # Errors
///
/// * If the action failed to execute
pub async fn set_volume(
    service: &Service,
    url: &Uri,
    instance_id: u32,
    channel: &str,
    volume: u8,
) -> Result<BTreeMap<String, String>, ActionError> {
    Ok(service
        .action(
            url,
            "SetVolume",
            &format!("<InstanceID>{instance_id}</InstanceID><Channel>{channel}</Channel><DesiredVolume>{volume}</DesiredVolume>"),
        )
        .await?.into_iter()
        .collect())
}

/// `UPnP` `AVTransport` service media information.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MediaInfo {
    /// Total duration of the media in seconds.
    media_duration: u32,
    /// Recording medium type (e.g., "`NOT_IMPLEMENTED`").
    record_medium: String,
    /// Write status of the media (e.g., "`NOT_IMPLEMENTED`").
    write_status: String,
    /// Metadata for the current media URI.
    current_uri_metadata: TrackMetadata,
    /// Number of tracks in the current playlist.
    nr_tracks: u32,
    /// Playback medium type (e.g., "NETWORK", "NONE").
    play_medium: String,
    /// URI of the current media.
    current_uri: String,
}

/// Retrieves media information from a `UPnP` `AVTransport` service.
///
/// # Errors
///
/// * If the action failed to execute
/// * If the media info is missing the required properties
pub async fn get_media_info(
    service: &Service,
    url: &Uri,
    instance_id: u32,
) -> Result<MediaInfo, ActionError> {
    let map = service
        .action(
            url,
            "GetMediaInfo",
            &format!("<InstanceID>{instance_id}</InstanceID>"),
        )
        .await?;

    Ok(MediaInfo {
        media_duration: str_to_duration(
            map.get("MediaDuration")
                .ok_or(ActionError::MissingProperty("MediaDuration".into()))?,
        ),
        record_medium: map
            .get("RecordMedium")
            .ok_or(ActionError::MissingProperty("MediaDuration".into()))?
            .clone(),
        write_status: map
            .get("WriteStatus")
            .ok_or(ActionError::MissingProperty("WriteStatus".into()))?
            .clone(),
        current_uri_metadata: parse_track_metadata(
            map.get("CurrentURIMetaData")
                .ok_or(ActionError::MissingProperty("CurrentURIMetaData".into()))?,
        )?,
        nr_tracks: map
            .get("NrTracks")
            .ok_or(ActionError::MissingProperty("NrTracks".into()))?
            .parse::<u32>()
            .map_err(|e| ActionError::MissingProperty(format!("NrTracks (\"{e:?}\")")))?,
        play_medium: map
            .get("PlayMedium")
            .ok_or(ActionError::MissingProperty("PlayMedium".into()))?
            .clone(),
        current_uri: map
            .get("CurrentURI")
            .ok_or(ActionError::MissingProperty("CurrentURI".into()))?
            .clone(),
    })
}

/// Subscribes to events from a `UPnP` service.
///
/// # Errors
///
/// * If the subscription failed to execute
pub async fn subscribe_events(
    service: &Service,
    url: &Uri,
) -> Result<
    (
        String,
        impl Stream<Item = Result<BTreeMap<String, String>, rupnp::Error>> + use<>,
    ),
    ScanError,
> {
    let (url, stream) = service.subscribe(url, 300).await?;

    Ok((url, stream.map(|x| x.map(|x| x.into_iter().collect()))))
}

/// Starts playback on a `UPnP` `AVTransport` service.
///
/// # Errors
///
/// * If the action failed to execute
pub async fn play(
    service: &Service,
    url: &Uri,
    instance_id: u32,
    speed: f64,
) -> Result<BTreeMap<String, String>, ActionError> {
    Ok(service
        .action(
            url,
            "Play",
            &format!("<InstanceID>{instance_id}</InstanceID><Speed>{speed}</Speed>"),
        )
        .await?
        .into_iter()
        .collect())
}

/// Pauses playback on a `UPnP` `AVTransport` service.
///
/// # Errors
///
/// * If the action failed to execute
pub async fn pause(
    service: &Service,
    url: &Uri,
    instance_id: u32,
) -> Result<BTreeMap<String, String>, ActionError> {
    Ok(service
        .action(
            url,
            "Pause",
            &format!("<InstanceID>{instance_id}</InstanceID>"),
        )
        .await?
        .into_iter()
        .collect())
}

/// Stops playback on a `UPnP` `AVTransport` service.
///
/// # Errors
///
/// * If the action failed to execute
pub async fn stop(
    service: &Service,
    url: &Uri,
    instance_id: u32,
) -> Result<BTreeMap<String, String>, ActionError> {
    Ok(service
        .action(
            url,
            "Stop",
            &format!("<InstanceID>{instance_id}</InstanceID>"),
        )
        .await?
        .into_iter()
        .collect())
}

/// Scans and retrieves information about a `UPnP` service.
///
/// # Errors
///
/// * If failed to scan for `UPnP` services
pub async fn scan_service(
    url: Option<&Uri>,
    service: &Service,
    path: Option<&str>,
) -> Result<UpnpService, ScanError> {
    let path = path.unwrap_or_default();

    log::debug!(
        "\n\
        {path}Scanning service:\n\t\
        {path}service_type={}\n\t\
        {path}service_id={}\n\t\
        ",
        service.service_type(),
        service.service_id(),
    );

    log::trace!(
        "service '{}' scpd={}",
        service.service_id(),
        if let Some(url) = url {
            format!("{:?}", service.scpd(url).await.ok())
        } else {
            "N/A".to_string()
        }
    );

    Ok(service.into())
}

/// Scans a `UPnP` device and its sub-devices, returning information about all discovered devices.
///
/// # Errors
///
/// * If failed to scan for `UPnP` devices
#[async_recursion]
pub async fn scan_device(
    device: Option<Device>,
    spec: &DeviceSpec,
    path: Option<&str>,
) -> Result<Vec<UpnpDevice>, ScanError> {
    let path = path.unwrap_or_default();

    log::debug!(
        "\n\
        {path}Scanning device: {}\n\t\
        {path}url={:?}\n\t\
        {path}manufacturer={}\n\t\
        {path}manufacturer_url={}\n\t\
        {path}model_name={}\n\t\
        {path}model_description={}\n\t\
        {path}model_number={}\n\t\
        {path}model_url={}\n\t\
        {path}serial_number={}\n\t\
        {path}udn={}\n\t\
        {path}upc={}\
        ",
        spec.friendly_name(),
        device.as_ref().map(rupnp::Device::url),
        spec.manufacturer(),
        spec.manufacturer_url().unwrap_or("N/A"),
        spec.model_name(),
        spec.model_description().unwrap_or("N/A"),
        spec.model_number().unwrap_or("N/A"),
        spec.model_url().unwrap_or("N/A"),
        spec.serial_number().unwrap_or("N/A"),
        spec.udn(),
        spec.upc().unwrap_or("N/A"),
    );

    let upnp_device: UpnpDevice = spec.into();
    let mut upnp_services = vec![];

    let services = spec.services();

    if services.is_empty() {
        log::debug!("no services for {}", spec.friendly_name());
    } else {
        let path = format!("{path}\t");
        for service in services {
            if let Some(device) = &device {
                cache::insert_service(device, service);
            }
            upnp_services.push(
                scan_service(
                    device.as_ref().map(rupnp::Device::url),
                    service,
                    Some(&path),
                )
                .await?,
            );
        }
    }

    let mut upnp_devices = vec![upnp_device.with_services(upnp_services)];

    let sub_devices = spec.devices();

    if sub_devices.is_empty() {
        log::debug!("no sub-devices for {}", spec.friendly_name());
    } else {
        let path = format!("{path}\t");
        for sub in sub_devices {
            // FIXME: should somehow insert sub-devices into the cache
            upnp_devices.extend_from_slice(&scan_device(None, sub, Some(&path)).await?);
        }
    }

    Ok(upnp_devices)
}

static UPNP_DEVICE_SCANNER: LazyLock<Arc<Mutex<UpnpDeviceScanner>>> =
    LazyLock::new(|| Arc::new(Mutex::new(UpnpDeviceScanner::new())));

static SCANNER: LazyLock<Box<dyn UpnpScanner>> = LazyLock::new(|| {
    #[cfg(feature = "simulator")]
    {
        Box::new(scanner::simulator::SimulatorScanner)
    }

    #[cfg(not(feature = "simulator"))]
    {
        Box::new(scanner::RupnpScanner)
    }
});

/// Scans the network for `UPnP` devices and caches them.
///
/// # Errors
///
/// * If failed to scan for `UPnP` devices
pub async fn scan_devices() -> Result<(), UpnpDeviceScannerError> {
    UPNP_DEVICE_SCANNER.lock().await.scan().await
}

/// Returns the list of cached `UPnP` devices from the last scan.
pub async fn devices() -> Vec<UpnpDevice> {
    UPNP_DEVICE_SCANNER.lock().await.devices.clone()
}

/// Scanner for discovering `UPnP` devices on the network.
#[derive(Default)]
pub struct UpnpDeviceScanner {
    scanning: bool,
    /// List of discovered `UPnP` devices from the most recent scan.
    pub devices: Vec<UpnpDevice>,
}

/// Errors that can occur when scanning for `UPnP` devices.
#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
pub enum UpnpDeviceScannerError {
    /// No audio outputs are available.
    #[error("No outputs available")]
    NoOutputs,
    /// Error from the underlying `UPnP` library.
    #[error(transparent)]
    Rupnp(#[from] rupnp::Error),
    /// Error scanning for `UPnP` devices or services.
    #[error(transparent)]
    Scan(#[from] ScanError),
}

impl UpnpDeviceScanner {
    /// Creates a new `UPnP` device scanner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Scans the network for `UPnP` devices and populates the device list.
    ///
    /// This method discovers devices on the local network, caches them, and stores
    /// their information in the scanner's device list. If devices have already been
    /// scanned or a scan is in progress, this method returns immediately without
    /// performing another scan.
    ///
    /// # Errors
    ///
    /// * If failed to scan for `UPnP` devices
    pub async fn scan(&mut self) -> Result<(), UpnpDeviceScannerError> {
        if self.scanning || !self.devices.is_empty() {
            return Ok(());
        }

        self.scanning = true;

        let search_target = SearchTarget::RootDevice;
        let devices = SCANNER
            .discover(&search_target, Duration::from_secs(3))
            .await?;
        pin_utils::pin_mut!(devices);

        let mut upnp_devices = vec![];

        loop {
            match devices.try_next().await {
                Ok(Some(device)) => {
                    cache::insert_device(device.clone());
                    let spec: &DeviceSpec = &device;
                    upnp_devices
                        .extend_from_slice(&scan_device(Some(device.clone()), spec, None).await?);
                }
                Ok(None) => {
                    break;
                }
                Err(e) => {
                    log::error!("Received error device response: {e:?}");
                }
            }
        }

        if upnp_devices.is_empty() {
            log::debug!("No `UPnP` devices discovered");
        }

        self.devices = upnp_devices
            .into_iter()
            .unique_by(|x| x.udn.clone())
            .collect::<Vec<_>>();

        self.scanning = false;

        Ok(())
    }
}
