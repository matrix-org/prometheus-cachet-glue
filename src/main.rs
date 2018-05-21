// (De-)Serialization
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate regex;
extern crate serde_json;
#[macro_use]
extern crate lazy_static;

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

use std::{collections::HashMap, str::FromStr};

use actix_web::{http, server, App, AsyncResponder, Error, HttpMessage, HttpRequest, HttpResponse,
                middleware::Logger};

use futures::future::Future;

use regex::Regex;

header! { (XCachetToken, "X-Cachet-Token") => [String] }

fn hook(req: HttpRequest) -> Box<Future<Item = HttpResponse, Error = Error>> {
    req.clone().json()                       // <- get JsonBody future
        .from_err()                          // <- automap to the error type we might want
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
                .header(XCachetToken(match get_bearer_token(req) {
                    Ok(token) => token,
                    Err(err) => {
                        error!("Unable to find bearer token in the \"Authorization\" header: {}", err);
                        return Ok(HttpResponse::new(http::StatusCode::UNAUTHORIZED));
                    }
                }
                )).json(&map).send() {
                Ok(res) => res,
                Err(err) => {
                    error!("Could not contact the cachet API: {}", err);
                    return Ok(HttpResponse::new(http::StatusCode::INTERNAL_SERVER_ERROR));
                }
            };
            info!("{:?}", response);
            Ok(HttpResponse::new(http::StatusCode::from_u16(response.status().as_u16()).unwrap()))
        }).responder()
}

fn get_bearer_token(req: HttpRequest) -> Result<String, String> {
    lazy_static! {
        static ref BEARER_REGEX: Regex = Regex::new(r"^Bearer (.*)$").unwrap();
    }
    match req.headers().get("Authorization") {
        Some(header) => match header.to_str() {
            Ok(header) => match BEARER_REGEX.captures(header) {
                Some(cap) => Ok(cap[0].to_string()),
                None => Err(format!(
                    "Authorization header does not contain a Bearer token."
                )),
            },
            Err(err) => Err(format!("{}", err)),
        },
        None => Err(format!("No authorization header found")),
    }
}

fn health_check(_: HttpRequest) -> &'static str {
    ""
}

fn main() {
    setup_logging(match std::env::var("LOG_LEVEL") {
        Ok(val) => match log::LevelFilter::from_str(&val) {
            Ok(level) => level,
            Err(_) => log::LevelFilter::Warn,
        },
        Err(_) => log::LevelFilter::Warn,
    });
    match server::new(|| {
        App::new()
            .middleware(Logger::default())
            .route("/", http::Method::POST, hook)
            .route("/health", http::Method::GET, health_check)
    }).bind(match std::env::var("BIND_ADDRESS") {
        Ok(val) => val,
        Err(_) => String::from("0.0.0.0:8888"),
    }) {
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
    version: String,

    #[serde(rename = "groupKey")]
    group_key: String,

    #[serde(rename = "status")]
    status: Status,

    #[serde(rename = "receiver")]
    receiver: String,

    #[serde(rename = "groupLabels")]
    group_labels: HashMap<String, String>,

    #[serde(rename = "commonLabels")]
    common_labels: HashMap<String, String>,

    #[serde(rename = "commonAnnotations")]
    common_annotations: HashMap<String, String>,

    #[serde(rename = "externalURL")]
    external_url: String,

    #[serde(rename = "alerts")]
    alerts: Vec<Alert>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Alert {
    #[serde(rename = "labels")]
    labels: HashMap<String, String>,

    #[serde(rename = "annotations")]
    annotations: CachetAnnotation,

    #[serde(rename = "startsAt")]
    starts_at: String,

    #[serde(rename = "endsAt")]
    ends_at: String,
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
    #[serde(with = "numi8")]
    component: i8,

    #[serde(rename = "severity")]
    #[serde(with = "numi8")]
    severity: i8,
}

/* For some reason alertmanager decides to make strings out of all the annotations,
 * so we need to parse it back to a number here.
 */
pub mod numi8 {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &i8, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}", value)[..])
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<i8, D::Error>
    where
        D: Deserializer<'de>,
    {
        let result = String::deserialize(deserializer)?;
        result
            .parse::<i8>()
            .map_err(|_| D::Error::custom("something happened"))
    }
}
