// (De-)Serialization
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

// HTTP
extern crate actix_web;
extern crate futures;
#[macro_use]
extern crate hyper;
extern crate reqwest;

// Logging and CLI
extern crate chrono;
extern crate fern;
#[macro_use]
extern crate log;

use std::collections::HashMap;

use actix_web::{http, server, App, AsyncResponder, Error, HttpMessage, HttpRequest, HttpResponse};
use actix_web::middleware::Logger;

use futures::future::Future;

header! { (XCachetToken, "X-Cachet-Token") => [String] }

fn hook(req: HttpRequest) -> Box<Future<Item = HttpResponse, Error = Error>> {
    req.clone().json()                          // <- get JsonBody future
        .from_err()
        .and_then(move |alert: AlertHook| {  // <- deserialized value
            info!("{:?}", alert);

            //create map for json of cachet api call
            let mut map = HashMap::new();
            map.insert(
                "status",
                match alert.status {
                    Status::Firing => alert.alerts[0].annotations.severity,
                    Status::Resolved => 1,
                },
            );

            //send http put to cachet
            let client = reqwest::Client::new();
            let response = match client.put(&format!(
                    "{endpoint}/api/v1/components/{component}",
                    endpoint = match std::env::var("CACHET_BASE_URL") {
                        Ok(val) => val,
                        Err(_) => String::from(include_str!("cachet_endpoint.txt")),
                    },
                    component = alert.alerts[0].annotations.component
                ))
                // read "Authorization: Bearer" token into "X-Cachet_Token"
                .header(XCachetToken(match req.headers().get("Authorization") {
                    Some(header) => match header.to_str() {
                        Ok(header) => {
                            let mut header_prefix = String::from(header);
                            header_prefix.split_off(7) //cuts of the first 7 chars: "Bearer "
                        },
                        Err(_) => return Ok(HttpResponse::new(http::StatusCode::UNAUTHORIZED)),
                    },
                    None => {
                        return Ok(HttpResponse::new(http::StatusCode::UNAUTHORIZED));
                    },
                })).json(&map).send() {
                    Ok(res) => res,
                    Err(err) => {
                        error!("{}", err);
                        return Ok(HttpResponse::new(http::StatusCode::INTERNAL_SERVER_ERROR))
                    }
                };
            info!("{:?}", response);
            Ok(HttpResponse::new(http::StatusCode::from_u16(response.status().as_u16()).unwrap()))
        }).responder()
}

fn healthcheck(_: HttpRequest) -> &'static str {
    "✅"
}

fn main() {
    setup_logging(log::LevelFilter::Info);
    match server::new(|| {
        App::new()
            .middleware(Logger::default())
            .route("/", http::Method::POST, hook)
            .route("/health", http::Method::GET, healthcheck)
    }).bind("0.0.0.0:8888")
    {
        Ok(server) => server.run(),
        Err(err) => error!("Couldn't start server: {}", err),
    }
}

fn setup_logging(level: log::LevelFilter) {
    match fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                message
            ))
        })
        .level(level)
        .chain(std::io::stdout())
        .apply()
    {
        Err(_) => {
            eprintln!("error setting up logging!");
        }
        _ => info!("logging set up properly"),
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AlertHook {
    #[serde(rename = "version")]
    pub(crate) version: String,

    #[serde(rename = "groupKey")]
    pub(crate) group_key: String,

    #[serde(rename = "status")]
    pub(crate) status: Status,

    #[serde(rename = "receiver")]
    pub(crate) receiver: String,

    #[serde(rename = "groupLabels")]
    pub(crate) group_labels: String,

    #[serde(rename = "commonLabels")]
    pub(crate) common_labels: String,

    #[serde(rename = "commonAnnotations")]
    pub(crate) common_annotations: String,

    #[serde(rename = "externalURL")]
    pub(crate) external_url: String,

    #[serde(rename = "alerts")]
    pub(crate) alerts: Vec<Alert>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Alert {
    #[serde(rename = "labels")]
    pub(crate) labels: String,

    #[serde(rename = "annotations")]
    pub(crate) annotations: CachetAnnotation,

    #[serde(rename = "startsAt")]
    pub(crate) starts_at: String,

    #[serde(rename = "endsAt")]
    pub(crate) ends_at: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Status {
    #[serde(rename = "firing")]
    Firing,

    #[serde(rename = "resolved")]
    Resolved,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CachetAnnotation {
    #[serde(rename = "component")]
    pub(crate) component: i32,

    #[serde(rename = "severity")]
    pub(crate) severity: i32,
}
