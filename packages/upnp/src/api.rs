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

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "UPnP")),
    paths(
        scan_devices_endpoint,
        get_transport_info_endpoint,
        get_media_info_endpoint,
        get_position_info_endpoint,
        get_volume_endpoint,
        set_volume_endpoint,
        subscribe_endpoint,
        pause_endpoint,
        play_endpoint,
        seek_endpoint,
    ),
    components(schemas(
        MediaInfo,
        PositionInfo,
        TransportInfo,
        crate::TrackMetadata,
        crate::TrackMetadataItem,
        crate::TrackMetadataItemResource,
        UpnpDevice,
        crate::models::UpnpService,
    ))
)]
pub struct Api;

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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["UPnP"],
        post,
        path = "/scan-devices",
        description = "Scan the network for UPnP devices",
        params(),
        responses(
            (
                status = 200,
                description = "List of UPnP devices",
                body = Vec<UpnpDevice>,
            )
        )
    )
)]
#[route("/scan-devices", method = "GET")]
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["UPnP"],
        get,
        path = "/transport-info",
        description = "Get the current UPnP transport info",
        params(
            ("deviceUdn" = Option<String>, Query, description = "UPnP device UDN to get transport info from"),
            ("deviceUrl" = Option<String>, Query, description = "UPnP device URL to get transport info from"),
            ("instanceId" = u32, Query, description = "UPnP instance ID to get transport info from"),
        ),
        responses(
            (
                status = 200,
                description = "The current UPnP transport info",
                body = TransportInfo,
            )
        )
    )
)]
#[route("/transport-info", method = "GET")]
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["UPnP"],
        get,
        path = "/media-info",
        description = "Get the current UPnP media info",
        params(
            ("deviceUdn" = Option<String>, Query, description = "UPnP device UDN to get media info from"),
            ("deviceUrl" = Option<String>, Query, description = "UPnP device URL to get media info from"),
            ("instanceId" = u32, Query, description = "UPnP instance ID to get media info from"),
        ),
        responses(
            (
                status = 200,
                description = "The current UPnP media info",
                body = MediaInfo,
            )
        )
    )
)]
#[route("/media-info", method = "GET")]
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["UPnP"],
        get,
        path = "/position-info",
        description = "Get the current UPnP position info",
        params(
            ("deviceUdn" = Option<String>, Query, description = "UPnP device UDN to get position info from"),
            ("deviceUrl" = Option<String>, Query, description = "UPnP device URL to get position info from"),
            ("instanceId" = u32, Query, description = "UPnP instance ID to get position info from"),
        ),
        responses(
            (
                status = 200,
                description = "The current UPnP position info",
                body = PositionInfo,
            )
        )
    )
)]
#[route("/position-info", method = "GET")]
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["UPnP"],
        get,
        path = "/volume",
        description = "Get the current UPnP volume info for a device",
        params(
            ("channel" = Option<String>, Query, description = "UPnP device channel to get volume info from"),
            ("deviceUdn" = Option<String>, Query, description = "UPnP device UDN to get volume info from"),
            ("deviceUrl" = Option<String>, Query, description = "UPnP device URL to get volume info from"),
            ("instanceId" = u32, Query, description = "UPnP instance ID to get volume info from"),
        ),
        responses(
            (
                status = 200,
                description = "The current UPnP volume info",
                body = HashMap<String, String>,
            )
        )
    )
)]
#[route("/volume", method = "GET")]
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["UPnP"],
        post,
        path = "/volume",
        description = "Set the current UPnP volume for a device",
        params(
            ("channel" = Option<String>, Query, description = "UPnP device channel to get volume info from"),
            ("deviceUdn" = Option<String>, Query, description = "UPnP device UDN to get volume info from"),
            ("deviceUrl" = Option<String>, Query, description = "UPnP device URL to get volume info from"),
            ("instanceId" = u32, Query, description = "UPnP instance ID to get volume info from"),
            ("value" = u8, Query, description = "Integer to set the device volume to"),
        ),
        responses(
            (
                status = 200,
                description = "The set volume action response",
                body = HashMap<String, String>,
            )
        )
    )
)]
#[route("/volume", method = "POST")]
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["UPnP"],
        post,
        path = "/subscribe",
        description = "Subscribe to the specified device's service",
        params(
            ("deviceUdn" = Option<String>, Query, description = "UPnP device UDN to subscribe to"),
            ("deviceUrl" = Option<String>, Query, description = "UPnP device URL to subscribe to"),
            ("serviceId" = String, Query, description = "UPnP device service ID to subscribe to"),
        ),
        responses(
            (
                status = 200,
                description = "The subscribe SID",
                body = String,
            )
        )
    )
)]
#[route("/subscribe", method = "POST")]
pub async fn subscribe_endpoint(query: web::Query<SubscribeQuery>) -> Result<Json<String>> {
    let (device, service) = if let Some(udn) = &query.device_udn {
        get_device_and_service(udn, &query.service_id)?
    } else if let Some(url) = &query.device_url {
        get_device_and_service_from_url(url, &query.service_id)?
    } else {
        return Err(ErrorBadRequest("Must pass device_udn or device_url"));
    };
    let (sid, mut stream) = subscribe_events(&service, device.url()).await?;

    moosicbox_task::spawn(&format!("upnp: api subscribe {sid}"), {
        let sid = sid.clone();
        async move {
            while let Ok(Some(event)) = stream.try_next().await {
                log::info!("Received subscription event for sid={sid}: {event:?}");
            }
            log::info!("Stream ended for sid={sid}");
        }
    });

    Ok(Json(sid))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PauseQuery {
    device_udn: Option<String>,
    device_url: Option<String>,
    instance_id: u32,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["UPnP"],
        post,
        path = "/pause",
        description = "Pause the specified device's AVTransport",
        params(
            ("deviceUdn" = Option<String>, Query, description = "UPnP device UDN to pause"),
            ("deviceUrl" = Option<String>, Query, description = "UPnP device URL to pause"),
            ("instanceId" = u32, Query, description = "UPnP instance ID to pause"),
        ),
        responses(
            (
                status = 200,
                description = "The pause action response",
                body = String,
            )
        )
    )
)]
#[route("/pause", method = "POST")]
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["UPnP"],
        post,
        path = "/play",
        description = "Play the specified device's AVTransport",
        params(
            ("speed" = Option<f64>, Query, description = "Speed to play the playback at"),
            ("deviceUdn" = Option<String>, Query, description = "UPnP device UDN to play"),
            ("deviceUrl" = Option<String>, Query, description = "UPnP device URL to play"),
            ("instanceId" = u32, Query, description = "UPnP instance ID to play"),
        ),
        responses(
            (
                status = 200,
                description = "The play action response",
                body = String,
            )
        )
    )
)]
#[route("/play", method = "POST")]
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["UPnP"],
        post,
        path = "/seek",
        description = "Seek the specified device's AVTransport",
        params(
            ("position" = f64, Query, description = "Seek position to seek the playback to"),
            ("deviceUdn" = Option<String>, Query, description = "UPnP device UDN to seek"),
            ("deviceUrl" = Option<String>, Query, description = "UPnP device URL to seek"),
            ("instanceId" = u32, Query, description = "UPnP instance ID to seek"),
            ("unit" = Option<String>, Query, description = "Seek unit"),
        ),
        responses(
            (
                status = 200,
                description = "The seek action response",
                body = String,
            )
        )
    )
)]
#[route("/seek", method = "POST")]
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
