#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "listener")]
pub mod listener;
#[cfg(feature = "player")]
pub mod player;

pub mod models;

use async_recursion::async_recursion;
use futures::prelude::*;
use itertools::Itertools;
use models::{UpnpDevice, UpnpService};
pub use rupnp::{http::Uri, ssdp::SearchTarget, Device, DeviceSpec, Service};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
    time::Duration,
};
use thiserror::Error;
use tokio::sync::Mutex;

mod cache {
    use std::{
        collections::HashMap,
        sync::{LazyLock, RwLock},
    };

    use rupnp::{Device, Service};

    use crate::ScanError;

    #[derive(Debug, Clone)]
    struct DeviceMapping {
        device: Device,
        services: HashMap<String, Service>,
    }

    static DEVICE_URL_MAPPINGS: LazyLock<RwLock<HashMap<String, DeviceMapping>>> =
        LazyLock::new(|| RwLock::new(HashMap::new()));

    static DEVICE_MAPPINGS: LazyLock<RwLock<HashMap<String, DeviceMapping>>> =
        LazyLock::new(|| RwLock::new(HashMap::new()));

    pub(crate) fn get_device_from_url(url: &str) -> Result<Device, ScanError> {
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

    pub(crate) fn get_device(udn: &str) -> Result<Device, ScanError> {
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

    pub(crate) fn insert_device(device: Device) {
        DEVICE_URL_MAPPINGS.write().unwrap().insert(
            device.url().to_string(),
            DeviceMapping {
                device: device.clone(),
                services: HashMap::new(),
            },
        );
        DEVICE_MAPPINGS.write().unwrap().insert(
            device.udn().to_owned(),
            DeviceMapping {
                device,
                services: HashMap::new(),
            },
        );
    }

    pub(crate) fn get_service(device_udn: &str, service_id: &str) -> Result<Service, ScanError> {
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

    pub(crate) fn get_device_and_service(
        device_udn: &str,
        service_id: &str,
    ) -> Result<(Device, Service), ScanError> {
        let devices = DEVICE_MAPPINGS.read().unwrap();
        let device = devices
            .get(device_udn)
            .ok_or_else(|| ScanError::DeviceUdnNotFound {
                device_udn: device_udn.to_string(),
            })?;
        Ok((
            device.device.clone(),
            device
                .services
                .get(service_id)
                .ok_or_else(|| ScanError::ServiceIdNotFound {
                    service_id: service_id.to_string(),
                })?
                .clone(),
        ))
    }

    pub(crate) fn get_device_and_service_from_url(
        device_url: &str,
        service_id: &str,
    ) -> Result<(Device, Service), ScanError> {
        let devices = DEVICE_URL_MAPPINGS.read().unwrap();
        let device = devices
            .get(device_url)
            .ok_or_else(|| ScanError::DeviceUrlNotFound {
                device_url: device_url.to_string(),
            })?;
        Ok((
            device.device.clone(),
            device
                .services
                .get(service_id)
                .ok_or_else(|| ScanError::ServiceIdNotFound {
                    service_id: service_id.to_string(),
                })?
                .clone(),
        ))
    }

    pub(crate) fn insert_service(device: &Device, service: Service) {
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

pub fn get_device(udn: &str) -> Result<Device, ScanError> {
    cache::get_device(udn)
}

pub fn get_service(device_udn: &str, service_id: &str) -> Result<Service, ScanError> {
    cache::get_service(device_udn, service_id)
}

pub fn get_device_and_service(
    device_udn: &str,
    service_id: &str,
) -> Result<(Device, Service), ScanError> {
    cache::get_device_and_service(device_udn, service_id)
}

pub fn get_device_from_url(url: &str) -> Result<Device, ScanError> {
    cache::get_device_from_url(url)
}

pub fn get_device_and_service_from_url(
    device_url: &str,
    service_id: &str,
) -> Result<(Device, Service), ScanError> {
    cache::get_device_and_service_from_url(device_url, service_id)
}

#[derive(Debug, Error)]
pub enum ActionError {
    #[error(transparent)]
    Roxml(#[from] roxmltree::Error),
    #[error(transparent)]
    Rupnp(#[from] rupnp::Error),
    #[error("Missing property \"{0}\"")]
    MissingProperty(String),
}

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("Failed to find RenderingControl service")]
    RenderingControlNotFound,
    #[error("Failed to find MediaRenderer service")]
    MediaRendererNotFound,
    #[error("Failed to find UPnP Device device_udn={device_udn}")]
    DeviceUdnNotFound { device_udn: String },
    #[error("Failed to find UPnP Device device_url={device_url}")]
    DeviceUrlNotFound { device_url: String },
    #[error("Failed to find UPnP Service service_id={service_id}")]
    ServiceIdNotFound { service_id: String },
    #[error(transparent)]
    Rupnp(#[from] rupnp::Error),
}

pub fn str_to_duration(duration: &str) -> u32 {
    let time_components = duration
        .split(':')
        .map(|x| x.parse())
        .collect::<Result<Vec<u32>, std::num::ParseIntError>>()
        .expect("Failed to parse time...");

    time_components[0] * 60 * 60 + time_components[1] * 60 + time_components[2]
}

pub fn string_to_duration(duration: String) -> u32 {
    str_to_duration(duration.as_str())
}

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
) -> Result<HashMap<String, String>, ActionError> {
    let headers = "*";

    let transport_uri = xml::escape::escape_str_attribute(transport_uri);

    let metadata = format!(
        r###"
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
        "###,
        title = title
            .map(xml::escape::escape_str_attribute)
            .map_or("".to_string(), |x| format!("<dc:title>{x}</dc:title>")),
        creator = creator
            .map(xml::escape::escape_str_attribute)
            .map_or("".to_string(), |x| format!("<dc:creator>{x}</dc:creator>")),
        artist = artist
            .map(xml::escape::escape_str_attribute)
            .map_or("".to_string(), |x| format!(
                "<upnp:artist>{x}</upnp:artist>"
            )),
        album = album
            .map(xml::escape::escape_str_attribute)
            .map_or("".to_string(), |x| format!("<upnp:album>{x}</upnp:album>")),
        original_track_number = original_track_number.map_or("".to_string(), |x| format!(
            "<upnp:originalTrackNumber>{x}</upnp:originalTrackNumber>"
        )),
        duration = duration.map_or("".to_string(), |x| format!(
            " duration=\"{}\"",
            duration_to_string(x)
        )),
        size = size.map_or("".to_string(), |x| format!(" size=\"{x}\"",)),
    );

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

    let metadata = escape_xml(&compress_xml(&metadata));

    let args = format!(
        r###"
        <InstanceID>{instance_id}</InstanceID>
        <CurrentURI>{transport_uri}</CurrentURI>
        <CurrentURIMetaData>{metadata}</CurrentURIMetaData>
        "###
    );
    let args = compress_xml(&args);
    log::debug!("set_av_transport_uri args={args}");

    Ok(service
        .action(device_url, "SetAVTransportURI", &args)
        .await?)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TrackMetadata {
    items: Vec<TrackMetadataItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TrackMetadataItem {
    upnp_class: Option<String>,
    upnp_artist: Option<String>,
    upnp_album: Option<String>,
    upnp_original_track_number: Option<String>,
    dc_title: Option<String>,
    dc_creator: Option<String>,
    res: TrackMetadataItemResource,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TrackMetadataItemResource {
    duration: Option<u32>,
    protocol_info: Option<String>,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TransportInfo {
    current_transport_status: String,
    current_transport_state: String,
    current_speed: String,
}

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
            .to_string(),
        current_transport_state: map
            .get("CurrentTransportState")
            .ok_or(ActionError::MissingProperty("CurrentTransportState".into()))?
            .to_string(),
        current_speed: map
            .get("CurrentSpeed")
            .ok_or(ActionError::MissingProperty("TrackURI".into()))?
            .to_string(),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PositionInfo {
    track: u32,
    rel_time: u32,
    abs_time: u32,
    track_uri: String,
    track_metadata: TrackMetadata,
    rel_count: u32,
    abs_count: u32,
    track_duration: u32,
}

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
            .to_string(),
        track_metadata: parse_track_metadata(
            map.get("TrackMetaData")
                .ok_or(ActionError::MissingProperty("TrackMetaData".into()))?,
        )?,
    })
}

pub async fn seek(
    service: &Service,
    url: &Uri,
    instance_id: u32,
    unit: &str,
    target: u32,
) -> Result<HashMap<String, String>, ActionError> {
    let target_str = duration_to_string(target);
    log::trace!("seek: seeking to target={target_str} instance_id={instance_id} unit={unit}");

    Ok(service
        .action(
            url,
            "Seek",
            &format!(
                r###"
                <InstanceID>{instance_id}</InstanceID>
                <Unit>{unit}</Unit>
                <Target>{target_str}</Target>
                "###
            ),
        )
        .await?)
}

pub async fn get_volume(
    service: &Service,
    url: &Uri,
    instance_id: u32,
    channel: &str,
) -> Result<HashMap<String, String>, ActionError> {
    Ok(service
        .action(
            url,
            "GetVolume",
            &format!("<InstanceID>{instance_id}</InstanceID><Channel>{channel}</Channel>"),
        )
        .await?)
}

pub async fn set_volume(
    service: &Service,
    url: &Uri,
    instance_id: u32,
    channel: &str,
    volume: u8,
) -> Result<HashMap<String, String>, ActionError> {
    Ok(service
        .action(
            url,
            "SetVolume",
            &format!("<InstanceID>{instance_id}</InstanceID><Channel>{channel}</Channel><DesiredVolume>{volume}</DesiredVolume>"),
        )
        .await?)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MediaInfo {
    media_duration: u32,
    record_medium: String,
    write_status: String,
    current_uri_metadata: TrackMetadata,
    nr_tracks: u32,
    play_medium: String,
    current_uri: String,
}

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
            .to_string(),
        write_status: map
            .get("WriteStatus")
            .ok_or(ActionError::MissingProperty("WriteStatus".into()))?
            .to_string(),
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
            .to_string(),
        current_uri: map
            .get("CurrentURI")
            .ok_or(ActionError::MissingProperty("CurrentURI".into()))?
            .to_string(),
    })
}

pub async fn subscribe_events(
    service: &Service,
    url: &Uri,
) -> Result<
    (
        String,
        impl Stream<Item = Result<HashMap<String, String>, rupnp::Error>>,
    ),
    ScanError,
> {
    Ok(service.subscribe(url, 300).await?)
}

pub async fn play(
    service: &Service,
    url: &Uri,
    instance_id: u32,
    speed: f64,
) -> Result<HashMap<String, String>, ActionError> {
    Ok(service
        .action(
            url,
            "Play",
            &format!("<InstanceID>{instance_id}</InstanceID><Speed>{speed}</Speed>"),
        )
        .await?)
}

pub async fn pause(
    service: &Service,
    url: &Uri,
    instance_id: u32,
) -> Result<HashMap<String, String>, ActionError> {
    Ok(service
        .action(
            url,
            "Pause",
            &format!("<InstanceID>{instance_id}</InstanceID>"),
        )
        .await?)
}

pub async fn stop(
    service: &Service,
    url: &Uri,
    instance_id: u32,
) -> Result<HashMap<String, String>, ActionError> {
    Ok(service
        .action(
            url,
            "Stop",
            &format!("<InstanceID>{instance_id}</InstanceID>"),
        )
        .await?)
}

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
        device.as_ref().map(|x| x.url()),
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
                cache::insert_service(device, service.clone());
            }
            upnp_services
                .push(scan_service(device.as_ref().map(|x| x.url()), service, Some(&path)).await?);
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

pub async fn scan_devices() -> Result<(), UpnpDeviceScannerError> {
    UPNP_DEVICE_SCANNER.lock().await.scan().await
}

pub async fn devices() -> Vec<UpnpDevice> {
    UPNP_DEVICE_SCANNER.lock().await.devices.clone()
}

#[derive(Default)]
pub struct UpnpDeviceScanner {
    scanning: bool,
    pub devices: Vec<UpnpDevice>,
}

#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
pub enum UpnpDeviceScannerError {
    #[error("No outputs available")]
    NoOutputs,
    #[error(transparent)]
    Rupnp(#[from] rupnp::Error),
    #[error(transparent)]
    Scan(#[from] ScanError),
}

impl UpnpDeviceScanner {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn scan(&mut self) -> Result<(), UpnpDeviceScannerError> {
        if self.scanning || !self.devices.is_empty() {
            return Ok(());
        }

        self.scanning = true;

        let search_target = SearchTarget::RootDevice;
        let devices = rupnp::discover(&search_target, Duration::from_secs(3)).await?;
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
            log::debug!("No UPnP devices discovered");
        }

        self.devices = upnp_devices
            .into_iter()
            .unique_by(|x| x.udn.clone())
            .collect::<Vec<_>>();

        self.scanning = false;

        Ok(())
    }
}
