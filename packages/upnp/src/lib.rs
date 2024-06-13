#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "listener")]
pub mod listener;

pub mod models;

use async_recursion::async_recursion;
use futures::prelude::*;
use models::{UpnpDevice, UpnpService};
use once_cell::sync::Lazy;
pub use rupnp::{http::Uri, ssdp::SearchTarget, Device, DeviceSpec, Service};
use std::{collections::HashMap, time::Duration};
use thiserror::Error;

mod cache {
    use std::{collections::HashMap, sync::RwLock};

    use once_cell::sync::Lazy;
    use rupnp::{Device, Service};

    #[derive(Debug, Clone)]
    struct DeviceMapping {
        device: Device,
        services: HashMap<String, Service>,
    }

    static DEVICE_URL_MAPPINGS: Lazy<RwLock<HashMap<String, DeviceMapping>>> =
        Lazy::new(|| RwLock::new(HashMap::new()));

    static DEVICE_MAPPINGS: Lazy<RwLock<HashMap<String, DeviceMapping>>> =
        Lazy::new(|| RwLock::new(HashMap::new()));

    pub(crate) fn get_device_from_url(url: &str) -> Option<Device> {
        DEVICE_URL_MAPPINGS
            .read()
            .unwrap()
            .get(url)
            .map(|x| x.device.clone())
    }

    pub(crate) fn get_device(udn: &str) -> Option<Device> {
        DEVICE_MAPPINGS
            .read()
            .unwrap()
            .get(udn)
            .map(|x| x.device.clone())
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

    pub(crate) fn get_service(device_udn: &str, service_id: &str) -> Option<Service> {
        DEVICE_MAPPINGS
            .read()
            .unwrap()
            .get(device_udn)
            .and_then(|x| x.services.get(service_id))
            .cloned()
    }

    pub(crate) fn get_device_and_service(
        device_udn: &str,
        service_id: &str,
    ) -> Option<(Device, Service)> {
        DEVICE_MAPPINGS
            .read()
            .unwrap()
            .get(device_udn)
            .and_then(|x| {
                x.services
                    .get(service_id)
                    .map(|s| (x.device.clone(), s.clone()))
            })
    }

    pub(crate) fn get_device_and_service_from_url(
        device_url: &str,
        service_id: &str,
    ) -> Option<(Device, Service)> {
        DEVICE_URL_MAPPINGS
            .read()
            .unwrap()
            .get(device_url)
            .and_then(|x| {
                x.services
                    .get(service_id)
                    .map(|s| (x.device.clone(), s.clone()))
            })
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

pub fn get_device(udn: &str) -> Option<Device> {
    cache::get_device(udn)
}

pub fn get_service(device_udn: &str, service_id: &str) -> Option<Service> {
    cache::get_service(device_udn, service_id)
}

pub fn get_device_and_service(device_udn: &str, service_id: &str) -> Option<(Device, Service)> {
    cache::get_device_and_service(device_udn, service_id)
}

pub fn get_device_from_url(url: &str) -> Option<Device> {
    cache::get_device_from_url(url)
}

pub fn get_device_and_service_from_url(
    device_url: &str,
    service_id: &str,
) -> Option<(Device, Service)> {
    cache::get_device_and_service_from_url(device_url, service_id)
}

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("Failed to find RenderingControl service")]
    RenderingControlNotFound,
    #[error("Failed to find MediaRenderer service")]
    MediaRendererNotFound,
    #[error(transparent)]
    Rupnp(#[from] rupnp::Error),
}

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
) -> Result<HashMap<String, String>, ScanError> {
    let headers = "*";

    let transport_uri = xml::escape::escape_str_attribute(transport_uri);

    let metadata = format!(
        r###"
        <DIDL-Lite
            xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/"
            xmlns:dc="http://purl.org/dc/elements/1.1/"
            xmlns:sec="http://www.sec.co.kr/"
            xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">
            <item id="0" parentID="-1" restricted="false">
                <upnp:class>object.item.audioItem.musicTrack</upnp:class>
                {title}
                {creator}
                {artist}
                {album}
                {original_track_number}
                <res protocolInfo="http-get:*:audio/{format}:{headers}">{transport_uri}</res>
            </item>
        </DIDL-Lite>
        "###,
        title = title.map_or("".to_string(), |x| format!("<dc:title>{x}</dc:title>")),
        creator = creator.map_or("".to_string(), |x| format!("<dc:creator>{x}</dc:creator>")),
        artist = artist.map_or("".to_string(), |x| format!(
            "<upnp:artist>{x}</upnp:artist>"
        )),
        album = album.map_or("".to_string(), |x| format!("<upnp:album>{x}</upnp:album>")),
        original_track_number = original_track_number.map_or("".to_string(), |x| format!(
            "<upnp:originalTrackNumber>{x}</upnp:originalTrackNumber>"
        )),
    );

    static BRACKET_WHITESPACE: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r">\s+<").expect("Invalid Regex"));
    static BETWEEN_WHITESPACE: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"\s{2,}").expect("Invalid Regex"));

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

pub async fn get_transport_info(
    service: &Service,
    url: &Uri,
    instance_id: u32,
) -> Result<HashMap<String, String>, ScanError> {
    Ok(service
        .action(
            url,
            "GetTransportInfo",
            &format!("<InstanceID>{instance_id}</InstanceID>"),
        )
        .await?)
}

pub async fn get_position_info(
    service: &Service,
    url: &Uri,
    instance_id: u32,
) -> Result<HashMap<String, String>, ScanError> {
    Ok(service
        .action(
            url,
            "GetPositionInfo",
            &format!("<InstanceID>{instance_id}</InstanceID>"),
        )
        .await?)
}

pub async fn get_volume(
    service: &Service,
    url: &Uri,
    instance_id: u32,
    channel: &str,
) -> Result<HashMap<String, String>, ScanError> {
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
) -> Result<HashMap<String, String>, ScanError> {
    Ok(service
        .action(
            url,
            "SetVolume",
            &format!("<InstanceID>{instance_id}</InstanceID><Channel>{channel}</Channel><DesiredVolume>{volume}</DesiredVolume>"),
        )
        .await?)
}

pub async fn get_media_info(
    service: &Service,
    url: &Uri,
    instance_id: u32,
) -> Result<HashMap<String, String>, ScanError> {
    Ok(service
        .action(
            url,
            "GetMediaInfo",
            &format!("<InstanceID>{instance_id}</InstanceID>"),
        )
        .await?)
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
) -> Result<HashMap<String, String>, ScanError> {
    Ok(service
        .action(
            url,
            "Play",
            &format!("<InstanceID>{instance_id}</InstanceID><Speed>{speed}</Speed>"),
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

pub async fn scan_devices() -> Result<Vec<UpnpDevice>, ScanError> {
    let search_target = SearchTarget::RootDevice;
    let devices = rupnp::discover(&search_target, Duration::from_secs(3)).await?;
    pin_utils::pin_mut!(devices);

    let mut upnp_devices = vec![];

    while let Some(device) = devices.try_next().await? {
        cache::insert_device(device.clone());
        let spec: &DeviceSpec = &device;
        upnp_devices.extend_from_slice(&scan_device(Some(device.clone()), spec, None).await?);
    }

    if upnp_devices.is_empty() {
        log::debug!("No UPnP devices discovered");
    }

    Ok(upnp_devices)
}
