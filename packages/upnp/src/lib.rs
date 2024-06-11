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
use rupnp::{http::Uri, ssdp::SearchTarget, DeviceSpec, Service};
use std::{collections::HashMap, time::Duration};
use thiserror::Error;

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
    url: Option<&Uri>,
    device: &DeviceSpec,
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
        device.friendly_name(),
        url,
        device.manufacturer(),
        device.manufacturer_url().unwrap_or("N/A"),
        device.model_name(),
        device.model_description().unwrap_or("N/A"),
        device.model_number().unwrap_or("N/A"),
        device.model_url().unwrap_or("N/A"),
        device.serial_number().unwrap_or("N/A"),
        device.udn(),
        device.upc().unwrap_or("N/A"),
    );

    let upnp_device: UpnpDevice = device.into();
    let mut upnp_services = vec![];

    let services = device.services();

    if services.is_empty() {
        log::debug!("no services for {}", device.friendly_name());
    } else {
        let path = format!("{path}\t");
        for service in services {
            upnp_services.push(scan_service(url, service, Some(&path)).await?);
        }
    }

    let mut upnp_devices = vec![upnp_device.with_services(upnp_services)];

    let sub_devices = device.devices();

    if sub_devices.is_empty() {
        log::debug!("no sub-devices for {}", device.friendly_name());
    } else {
        let path = format!("{path}\t");
        for sub in sub_devices {
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
        upnp_devices.extend_from_slice(&scan_device(Some(device.url()), &device, None).await?);
    }

    if upnp_devices.is_empty() {
        log::debug!("No UPnP devices discovered");
    }

    Ok(upnp_devices)
}
