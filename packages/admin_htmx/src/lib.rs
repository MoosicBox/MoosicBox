#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use actix_htmx::Htmx;
use actix_web::{route, web, HttpResponse, Responder};
use maud::{html, DOCTYPE};

#[route("", method = "GET")]
pub async fn index_endpoint(
    _htmx: Htmx,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<impl Responder, actix_web::Error> {
    let mut response = HttpResponse::Ok();
    response.content_type("text/html");

    let scan_paths = moosicbox_scan::get_scan_paths(&**data.database).await?;

    Ok(response.body(
        html! {
            (DOCTYPE)
            html {
                head {
                    title { "MoosicBox Admin" }
                    style {("
                        .scan-path {
                            display: flex;
                        }
                    ")}
                    script
                        src="https://unpkg.com/htmx.org@2.0.2"
                        integrity="sha384-Y7hw+L/jvKeWIRRkqWYfPcvVxHzVzn5REgzbawhxAuQGwX1XWe70vji+VSeHOThJ"
                        crossorigin="anonymous"
                        {}
                }
                body {
                    h1 { "MoosicBox Admin" }
                    hr {}
                    h2 { "Scan" }
                    @for scan_path in scan_paths {
                        div.scan-path {
                            p { (scan_path) }
                            button type="button" { "Remove" }
                        }
                    }
                    button
                        type="button"
                        hx-post="/scan/start-scan?origins=LOCAL"
                        hx-swap="none"
                        { "Start Scan" }
                }
            }
        }
        .into_string(),
    ))
}
