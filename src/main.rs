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

use actix_web::{
    http, middleware::Logger, server, App, AsyncResponder, Error, HttpMessage, HttpRequest,
    HttpResponse,
};

use futures::future::Future;

use regex::Regex;

header! { (XCachetToken, "X-Cachet-Token") => [String] }

fn hook(req: HttpRequest) -> Box<Future<Item = HttpResponse, Error = Error>> {
    req.clone().json()                       // <- get JsonBody future
        .from_err()                          // <- automap to the error type we might want
        .and_then(move |alert_hook: AlertHook| {  // <- deserialized value
            info!("{:?}", alert_hook);

            let mut components : HashMap<i8, i8> = HashMap::new();
            let mut responses = Vec::new();

            for alert in alert_hook.alerts {
                let current_severity : i8 = match components.get(&alert.annotations.component) {
                    Some(severity) => *severity,
                    None => 0 as i8,
                }.clone();
                let target_severity = match alert.status {
                    Status::Firing => alert.annotations.severity,
                    Status::Resolved => 1,
                };
                match current_severity {
                    n if n < target_severity => {
                        components.insert(alert.annotations.component, target_severity);
                    }
                    _ => {},
                };
            }

            let http_client = reqwest::Client::new();

            for (component, status) in components {

                //create map for json of cachet api call
                let mut map = HashMap::new();
                map.insert("status", status);

                match get_bearer_token(req.clone()) {
                    Ok(token) => {//send http put to cachet
                        match http_client.put(&format!(
                            "{}/api/v1/components/{}",
                            match std::env::var("CACHET_BASE_URL") {
                                Ok(val) => val,
                                Err(_) => String::from(include_str!("cachet_endpoint.txt")),
                            },
                            component
                        ))
                            // read "Authorization: Bearer" token into "X-Cachet_Token"
                            .header(XCachetToken(token)).json(&map).send() {
                            Ok(res) => {
                                responses.push(CachetResponse {
                                    http_status: res.status().as_u16(),
                                    status: CachetAnnotation {
                                        component,
                                        severity: status,
                                    }
                                })
                            },
                            Err(err) => {
                                error!("Could not contact the cachet API: {}", err);
                                responses.push(CachetResponse {
                                    http_status: http::StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                                    status: CachetAnnotation {
                                        component,
                                        severity: status,
                                    }
                                })
                            }
                        }
                    },
                    Err(err) => {
                        responses.push(CachetResponse {
                            http_status: http::StatusCode::UNAUTHORIZED.as_u16(),
                            status: CachetAnnotation {
                                component,
                                severity: status,
                            }
                        });
                        error!("Unable to find bearer token in the \"Authorization\" header: {}", err);
                    }
                }
            };


            let (status, body) = match serde_json::to_string(&responses) {
                Ok(body) => (http::StatusCode::OK, body),
                Err(err) => {
                    error!("Couldn't serialize the cachet responses: {}", err);
                    (http::StatusCode::INTERNAL_SERVER_ERROR, "Couldn't serialize cachet responses.".to_string())
                }
            };
            info!("{:?}", responses);
            Ok(HttpResponse::with_body(status, body.into_bytes()))
        }).responder()
}

fn get_bearer_token(req: HttpRequest) -> Result<String, String> {
    lazy_static! {
        static ref BEARER_REGEX: Regex = Regex::new(r"^Bearer (.*)$").unwrap();
    }
    match req.headers().get("Authorization") {
        Some(header) => match header.to_str() {
            Ok(header) => match BEARER_REGEX.captures(header) {
                Some(cap) => Ok(cap[1].to_string()),
                None => Err(format!(
                    "Authorization header does not contain a Bearer token."
                )),
            },
            Err(err) => Err(format!("{}", err)),
        },
        None => match std::env::var("CACHET_AUTH_TOKEN") {
            Ok(val) => Ok(val),
            Err(_) => Err(format!("No Authorization header")),
        },
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
    #[serde(rename = "status")]
    status: Status,

    #[serde(rename = "labels")]
    labels: HashMap<String, String>,

    #[serde(rename = "annotations")]
    annotations: CachetAnnotation,

    #[serde(rename = "startsAt")]
    starts_at: String,

    #[serde(rename = "endsAt")]
    ends_at: String,

    #[serde(rename = "generatorURL")]
    generator_url: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Status {
    #[serde(rename = "firing")]
    Firing,

    #[serde(rename = "resolved")]
    Resolved,
}

impl PartialEq for Status {
    fn eq(&self, other: &Status) -> bool {
        match (self, other) {
            (Status::Firing, Status::Firing) => true,
            (Status::Resolved, Status::Resolved) => true,
            (Status::Firing, Status::Resolved) => false,
            (Status::Resolved, Status::Firing) => false,
        }
    }
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

#[derive(Serialize, Deserialize, Debug)]
pub struct CachetResponse {
    #[serde(rename = "httpStatus")]
    http_status: u16,

    #[serde(rename = "status")]
    status: CachetAnnotation,
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
