use std::collections::HashMap;

use actix_web::{
    error::{ErrorBadRequest, ErrorFailedDependency, ErrorInternalServerError},
    route,
    web::{self, Json},
    Result,
};
use futures::TryStreamExt;
use serde::Deserialize;

use crate::{
    cache::get_device_and_service_from_url, get_device_and_service, get_media_info,
    get_position_info, get_transport_info, get_volume, models::UpnpDevice, pause, play,
    scan_devices, seek, set_volume, subscribe_events, ActionError, MediaInfo, PositionInfo,
    ScanError, TransportInfo,
};

impl From<ActionError> for actix_web::Error {
    fn from(e: ActionError) -> Self {
        match &e {
            ActionError::Rupnp(rupnp_err) => {
                ErrorFailedDependency(format!("UPnP error: {rupnp_err:?}"))
            }
            ActionError::MissingProperty(_property) => ErrorInternalServerError(e.to_string()),
            ActionError::Roxml(roxmltree_err) => {
                ErrorFailedDependency(format!("roxmltree error: {roxmltree_err:?}"))
            }
        }
    }
}

impl From<ScanError> for actix_web::Error {
    fn from(e: ScanError) -> Self {
        match e {
            ScanError::RenderingControlNotFound => ErrorFailedDependency(e.to_string()),
            ScanError::MediaRendererNotFound => ErrorFailedDependency(e.to_string()),
            ScanError::DeviceUdnNotFound { .. } => ErrorFailedDependency(e.to_string()),
            ScanError::DeviceUrlNotFound { .. } => ErrorFailedDependency(e.to_string()),
            ScanError::ServiceIdNotFound { .. } => ErrorFailedDependency(e.to_string()),
            ScanError::Rupnp(rupnp_err) => {
                ErrorFailedDependency(format!("UPnP error: {rupnp_err:?}"))
            }
        }
    }
}

#[route("/upnp/scan-devices", method = "GET")]
pub async fn scan_devices_endpoint() -> Result<Json<Vec<UpnpDevice>>> {
    Ok(Json(scan_devices().await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTransportInfoQuery {
    device_udn: Option<String>,
    device_url: Option<String>,
    instance_id: u32,
}

#[route("/upnp/transport-info", method = "GET")]
pub async fn get_transport_info_endpoint(
    query: web::Query<GetTransportInfoQuery>,
) -> Result<Json<TransportInfo>> {
    let (device, service) = if let Some(udn) = &query.device_udn {
        get_device_and_service(udn, "urn:upnp-org:serviceId:AVTransport")?
    } else if let Some(url) = &query.device_url {
        get_device_and_service_from_url(url, "urn:upnp-org:serviceId:AVTransport")?
    } else {
        return Err(ErrorBadRequest("Must pass device_udn or device_url"));
    };
    Ok(Json(
        get_transport_info(&service, device.url(), query.instance_id).await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMediaInfoQuery {
    device_udn: Option<String>,
    device_url: Option<String>,
    instance_id: u32,
}

#[route("/upnp/media-info", method = "GET")]
pub async fn get_media_info_endpoint(
    query: web::Query<GetMediaInfoQuery>,
) -> Result<Json<MediaInfo>> {
    let (device, service) = if let Some(udn) = &query.device_udn {
        get_device_and_service(udn, "urn:upnp-org:serviceId:AVTransport")?
    } else if let Some(url) = &query.device_url {
        get_device_and_service_from_url(url, "urn:upnp-org:serviceId:AVTransport")?
    } else {
        return Err(ErrorBadRequest("Must pass device_udn or device_url"));
    };
    Ok(Json(
        get_media_info(&service, device.url(), query.instance_id).await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPositionInfoQuery {
    device_udn: Option<String>,
    device_url: Option<String>,
    instance_id: u32,
}

#[route("/upnp/position-info", method = "GET")]
pub async fn get_position_info_endpoint(
    query: web::Query<GetPositionInfoQuery>,
) -> Result<Json<PositionInfo>> {
    let (device, service) = if let Some(udn) = &query.device_udn {
        get_device_and_service(udn, "urn:upnp-org:serviceId:AVTransport")?
    } else if let Some(url) = &query.device_url {
        get_device_and_service_from_url(url, "urn:upnp-org:serviceId:AVTransport")?
    } else {
        return Err(ErrorBadRequest("Must pass device_udn or device_url"));
    };
    Ok(Json(
        get_position_info(&service, device.url(), query.instance_id).await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetVolumeQuery {
    channel: Option<String>,
    device_udn: Option<String>,
    device_url: Option<String>,
    instance_id: u32,
}

#[route("/upnp/volume", method = "GET")]
pub async fn get_volume_endpoint(
    query: web::Query<GetVolumeQuery>,
) -> Result<Json<HashMap<String, String>>> {
    let (device, service) = if let Some(udn) = &query.device_udn {
        get_device_and_service(udn, "urn:upnp-org:serviceId:RenderingControl")?
    } else if let Some(url) = &query.device_url {
        get_device_and_service_from_url(url, "urn:upnp-org:serviceId:RenderingControl")?
    } else {
        return Err(ErrorBadRequest("Must pass device_udn or device_url"));
    };
    Ok(Json(
        get_volume(
            &service,
            device.url(),
            query.instance_id,
            query.channel.as_deref().unwrap_or("Master"),
        )
        .await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetVolumeQuery {
    channel: Option<String>,
    device_udn: Option<String>,
    device_url: Option<String>,
    instance_id: u32,
    value: u8,
}

#[route("/upnp/volume", method = "POST")]
pub async fn set_volume_endpoint(
    query: web::Query<SetVolumeQuery>,
) -> Result<Json<HashMap<String, String>>> {
    let (device, service) = if let Some(udn) = &query.device_udn {
        get_device_and_service(udn, "urn:upnp-org:serviceId:RenderingControl")?
    } else if let Some(url) = &query.device_url {
        get_device_and_service_from_url(url, "urn:upnp-org:serviceId:RenderingControl")?
    } else {
        return Err(ErrorBadRequest("Must pass device_udn or device_url"));
    };
    Ok(Json(
        set_volume(
            &service,
            device.url(),
            query.instance_id,
            query.channel.as_deref().unwrap_or("Master"),
            query.value,
        )
        .await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeQuery {
    device_udn: Option<String>,
    device_url: Option<String>,
    service_id: String,
}

#[route("/upnp/subscribe", method = "POST")]
pub async fn subscribe_endpoint(query: web::Query<SubscribeQuery>) -> Result<Json<String>> {
    let (device, service) = if let Some(udn) = &query.device_udn {
        get_device_and_service(udn, &query.service_id)?
    } else if let Some(url) = &query.device_url {
        get_device_and_service_from_url(url, &query.service_id)?
    } else {
        return Err(ErrorBadRequest("Must pass device_udn or device_url"));
    };
    let (sid, mut stream) = subscribe_events(&service, device.url()).await?;

    tokio::task::Builder::new()
        .name(&format!("upnp: api subscribe {sid}"))
        .spawn({
            let sid = sid.clone();
            async move {
                while let Ok(Some(event)) = stream.try_next().await {
                    log::info!("Received subscription event for sid={sid}: {event:?}");
                }
                log::info!("Stream ended for sid={sid}");
            }
        })
        .unwrap();

    Ok(Json(sid))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PauseQuery {
    device_udn: Option<String>,
    device_url: Option<String>,
    instance_id: u32,
}

#[route("/upnp/pause", method = "POST")]
pub async fn pause_endpoint(
    query: web::Query<PauseQuery>,
) -> Result<Json<HashMap<String, String>>> {
    let (device, service) = if let Some(udn) = &query.device_udn {
        get_device_and_service(udn, "urn:upnp-org:serviceId:AVTransport")?
    } else if let Some(url) = &query.device_url {
        get_device_and_service_from_url(url, "urn:upnp-org:serviceId:AVTransport")?
    } else {
        return Err(ErrorBadRequest("Must pass device_udn or device_url"));
    };
    Ok(Json(
        pause(&service, device.url(), query.instance_id).await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayQuery {
    speed: Option<f64>,
    device_udn: Option<String>,
    device_url: Option<String>,
    instance_id: u32,
}

#[route("/upnp/play", method = "POST")]
pub async fn play_endpoint(query: web::Query<PlayQuery>) -> Result<Json<HashMap<String, String>>> {
    let (device, service) = if let Some(udn) = &query.device_udn {
        get_device_and_service(udn, "urn:upnp-org:serviceId:AVTransport")?
    } else if let Some(url) = &query.device_url {
        get_device_and_service_from_url(url, "urn:upnp-org:serviceId:AVTransport")?
    } else {
        return Err(ErrorBadRequest("Must pass device_udn or device_url"));
    };
    Ok(Json(
        play(
            &service,
            device.url(),
            query.instance_id,
            query.speed.unwrap_or(1.0),
        )
        .await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeekQuery {
    position: f64,
    device_udn: Option<String>,
    device_url: Option<String>,
    instance_id: u32,
    unit: Option<String>,
}

#[route("/upnp/seek", method = "POST")]
pub async fn seek_endpoint(query: web::Query<SeekQuery>) -> Result<Json<HashMap<String, String>>> {
    let (device, service) = if let Some(udn) = &query.device_udn {
        get_device_and_service(udn, "urn:upnp-org:serviceId:AVTransport")?
    } else if let Some(url) = &query.device_url {
        get_device_and_service_from_url(url, "urn:upnp-org:serviceId:AVTransport")?
    } else {
        return Err(ErrorBadRequest("Must pass device_udn or device_url"));
    };
    Ok(Json(
        seek(
            &service,
            device.url(),
            query.instance_id,
            query.unit.as_deref().unwrap_or("ABS_TIME"),
            query.position as u32,
        )
        .await?,
    ))
}
