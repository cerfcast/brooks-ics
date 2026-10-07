// brooks-ics, Copyright 2026, Will Hawkins
//
// This file is part of brooks-ics.

// This file is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fmt::Display;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use actix_web::http::header::{HeaderName, HeaderValue};
use actix_web::{HttpRequest, dev::PeerAddr};
use actix_web::{get, web};

use awc::http::StatusCode;
use awc::http::header::HOST;
use brooks_lib::cdni::gmdp::ProcessedRequestResponse;
use brooks_lib::environment::scope::{Scope, Scopes};
use brooks_lib::integrations::common::safe_brooks_integration_handle;
use brooks_lib::integrations::hmds::{HmdsConfiguration, HmdsServerConfiguration};
use brooks_lib::logging::LogLevel::Debug;
use brooks_lib::logging::{LogMsgFormatter, LogMsgs};
use brooks_lib::mel::interpreter::builtins::builtin_builtin_function_interpreters;
use brooks_lib::mel::interpreter::interpret::TypedValue;
use brooks_lib::tools::prr;
use log::Level::Trace;
use log::{info, log};

use std::error::Error as StdError;

#[derive(Debug)]
struct ProxyHttpRequest<'a>(&'a actix_web::HttpRequest);
struct ProxyHttpResponse<'a>(&'a http::Response<Vec<u8>>);

#[derive(Debug)]
enum ActixConversionError {
    Status(Box<dyn StdError>),
    HeaderName(Box<dyn StdError>),
    HeaderValue(Box<dyn StdError>),
    Header(Box<dyn StdError>),
    Body(Box<dyn StdError>),
}

impl Display for ActixConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActixConversionError::Status(error) => write!(f, "Bad status: {error}"),
            ActixConversionError::HeaderName(error) => write!(f, "Bad header name: {error}"),
            ActixConversionError::HeaderValue(error) => write!(f, "Bad header value: {error}"),
            ActixConversionError::Header(error) => write!(f, "Bad header: {error}"),
            ActixConversionError::Body(error) => write!(f, "Bad body: {error}"),
        }
    }
}

impl StdError for ActixConversionError {}

/// Support conversion from http::Response to actix_web::HttpResponse.
impl<'a> TryFrom<ProxyHttpResponse<'a>> for actix_web::HttpResponse<Vec<u8>> {
    type Error = ActixConversionError;

    fn try_from(value: ProxyHttpResponse<'a>) -> Result<Self, Self::Error> {
        let mut builder = actix_web::HttpResponseBuilder::new(
            StatusCode::from_u16(value.0.status().as_u16())
                .map_err(|e| ActixConversionError::Status(e.into()))?,
        );

        for (name, value) in value.0.headers() {
            builder = builder
                .append_header((
                    HeaderName::from_str(name.as_str())
                        .map_err(|e| ActixConversionError::HeaderName(e.into()))?,
                    HeaderValue::from_str(
                        value
                            .to_str()
                            .map_err(|e| ActixConversionError::HeaderValue(e.into()))?,
                    )
                    .map_err(|e| ActixConversionError::Header(e.into()))?,
                ))
                .take();
        }

        builder
            .message_body(value.0.body().clone())
            .map_err(|e| ActixConversionError::Body(e.into()))
    }
}

impl<'a> TryFrom<ProxyHttpRequest<'a>> for http::Request<Vec<u8>> {
    type Error = ActixConversionError;
    fn try_from(
        value: ProxyHttpRequest<'a>,
    ) -> Result<http::Request<std::vec::Vec<u8>>, Self::Error> {
        let mut builder = http::request::Builder::new();

        for (name, value) in value.0.headers() {
            builder = builder.header(
                http::HeaderName::from_str(name.as_str())
                    .map_err(|e| ActixConversionError::HeaderName(e.into()))?,
                http::HeaderValue::from_str(
                    value
                        .to_str()
                        .map_err(|e| ActixConversionError::HeaderName(e.into()))?,
                )
                .map_err(|e| ActixConversionError::Header(e.into()))?,
            );
        }

        builder = builder.method(value.0.method().as_str());

        builder = builder.uri(value.0.full_url().as_str());

        builder
            .body(vec![])
            .map_err(|e| ActixConversionError::Body(e.into()))
    }
}

#[get("{tail}*")]
async fn index(
    req: HttpRequest,
    data: web::Data<Arc<Mutex<HmdsConfiguration>>>,
    _peer: PeerAddr,
) -> actix_web::HttpResponse<Vec<u8>> {
    let host_header = match req.headers().get(HOST) {
        Some(host_header) => host_header.clone(),
        None => {
            return actix_web::HttpResponse::InternalServerError()
                .message_body("Missing HOST header".to_string().into())
                .expect("Could not construct error response.");
        }
    };

    let key = match host_header.to_str() {
        Ok(key) => key.to_string(),
        Err(_) => {
            return actix_web::HttpResponse::InternalServerError()
                .message_body("Could not convert HOST header to string".to_string().into())
                .expect("Could not construct error response.");
        }
    };

    let http_request: http::Request<Vec<u8>> = match ProxyHttpRequest(&req).try_into() {
        Ok(req) => req,
        Err(e) => {
            return actix_web::HttpResponse::InternalServerError()
                .message_body(e.to_string().into())
                .expect("Could not construct error response.");
        }
    };

    let prr = match TryInto::<ProcessedRequestResponse>::try_into(&http_request) {
        Ok(prr) => prr,
        Err(e) => {
            return actix_web::HttpResponse::InternalServerError()
                .message_body(e.to_string().into())
                .expect("Could not construct error response.");
        }
    };

    let processing_result = match tokio::runtime::Handle::current()
        .spawn_blocking(move || {
            let scopes: Scopes<TypedValue> = (&builtin_builtin_function_interpreters()
                + &Scope::<TypedValue>::from(&prr as &dyn prr::Prr<Vec<u8>>))
                .into();

            let mut config = data.try_lock().expect("Could not lock the configuration.");
            let brooks_log = LogMsgs::new_with_prefix("CLI Proxy", Debug);
            let runtime = tokio::runtime::Handle::current();

            let processing_result = safe_brooks_integration_handle(
                &http_request,
                scopes,
                &key,
                &mut config,
                &runtime,
                brooks_log,
            );

            let (res, logs) = match processing_result {
                Ok((status, response, logs)) => (Ok((status, response)), logs),
                Err((e, logs)) => (Err(format!("{}", e)), logs),
            };

            for logmsg in logs.use_msgs() {
                log!(
                    logmsg.level().into(),
                    "{}",
                    logmsg.pretty(&LogMsgFormatter {
                        newline: false,
                        show_level: true
                    })
                )
            }

            res
        })
        .await
    {
        Ok(pr) => pr,
        Err(e) => {
            return actix_web::HttpResponse::InternalServerError()
                .message_body(e.to_string().into())
                .expect("Could not construct error response.");
        }
    };

    let (_status, response) = match processing_result {
        Ok((status, response)) => (status, response),
        Err(e) => {
            return actix_web::HttpResponse::InternalServerError()
                .message_body(e.to_string().into())
                .expect("Could not construct error response.");
        }
    };

    match ProxyHttpResponse(&response).try_into() {
        Ok(actix_resp) => actix_resp,
        Err(e) => actix_web::HttpResponse::InternalServerError()
            .message_body(e.to_string().into())
            .expect("Could not construct error response."),
    }
}

pub async fn proxy(ip: String, port: u16, config: HmdsServerConfiguration) -> std::io::Result<()> {
    use actix_web::{App, HttpServer, middleware::Logger};

    let hmds_config: Arc<Mutex<HmdsConfiguration>> = Arc::new(Mutex::new(config.into()));

    info!("Proxying on {}:{}", ip, port);
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(hmds_config.clone()))
            .wrap(Logger::default().log_level(Trace))
            .wrap(actix_cors::Cors::permissive())
            .service(index)
    })
    .bind((ip, port))?
    .run()
    .await
}
