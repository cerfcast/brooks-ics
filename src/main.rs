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

#[allow(
    redundant_imports,
    unused_imports,
    clippy::single_component_path_imports
)]
use brooks_lib;

#[cfg(test)]
mod test;

use brooks_lib::hmds::HmdsServerConfiguration;
use brooks_lib::hmds::logging::supported;
use clap::{ArgAction, CommandFactory, Parser, Subcommand};
use clio::ClioPath;
use flexi_logger::{FileSpec, LogSpecification, Logger};
use log::{LevelFilter, info};

use brooks_lib::hmds::logging::session::Span;
use std::sync::{Arc, Mutex};

use crate::logger::LoggingInterfaceImpl;

mod echo;
mod hmds;
mod logger;
mod proxy;

#[derive(Debug, Clone)]
enum AppDebugLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl From<u8> for AppDebugLevel {
    fn from(value: u8) -> Self {
        if value >= 3 {
            Self::Debug
        } else if value >= 2 {
            Self::Info
        } else if value >= 1 {
            Self::Warn
        } else {
            Self::Error
        }
    }
}

impl From<AppDebugLevel> for LevelFilter {
    fn from(value: AppDebugLevel) -> Self {
        match value {
            AppDebugLevel::Error => LevelFilter::Error,
            AppDebugLevel::Warn => LevelFilter::Warn,
            AppDebugLevel::Info => LevelFilter::Info,
            AppDebugLevel::Debug => LevelFilter::Debug,
        }
    }
}

#[derive(Parser)]
struct App {
    #[arg(long, action=ArgAction::Count)]
    debug: u8,

    #[arg(long)]
    log_file: Option<ClioPath>,

    #[command(subcommand)]
    command: Commands,
}

pub fn parse_timeout_duration(given_duration: &str) -> Result<chrono::Duration, clap::Error> {
    if given_duration.ends_with("s") {
        let time: i64 = given_duration[0..given_duration.len() - 1]
            .parse()
            .map_err(|_| clap::error::Error::new(clap::error::ErrorKind::ValueValidation))?;
        Ok(chrono::Duration::seconds(time))
    } else if given_duration.ends_with("ns") {
        let time: i64 = given_duration[0..given_duration.len() - 2]
            .parse()
            .map_err(|_| clap::error::Error::new(clap::error::ErrorKind::ValueValidation))?;
        Ok(chrono::Duration::nanoseconds(time))
    } else {
        Err(clap::error::Error::new(
            clap::error::ErrorKind::ValueValidation,
        ))
    }
}

#[derive(Subcommand, Debug)]
enum Commands {
    Proxy {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "8080")]
        port: u16,
        #[arg(long)]
        path: String,
    },
    Echo {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "8080")]
        port: u16,
    },
    Hmds {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "8080")]
        port: u16,

        #[cfg(feature = "domain")]
        #[arg(long, default_value = "/tmp/brooks/server")]
        path: clio::ClioPath,

        #[arg(long, default_value = "300s", value_parser=clap::builder::ValueParser::new(parse_timeout_duration))]
        timeout: chrono::Duration,

        #[cfg(feature = "domain")]
        #[arg(long)]
        user: Option<String>,
        #[cfg(feature = "domain")]
        #[arg(long)]
        group: Option<String>,
    },
}

#[derive(Debug)]
pub enum AppError {
    CliError(Box<std::io::Error>),
    IcsError(std::io::Error),
    EchoError(std::io::Error),
    ProxyError(std::io::Error),
}
pub type CliResult<T> = Result<T, AppError>;

impl Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::CliError(e) => write!(f, "{e}"),
            AppError::IcsError(e) => write!(f, "Interconnection server error: {e}"),
            AppError::ProxyError(e) => write!(f, "CDNI proxy server error: {e}"),
            AppError::EchoError(e) => write!(f, "HTTP echo server error: {e}"),
        }
    }
}

// Multi threaded runtime needed for the Brooks library.
#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let App {
        debug: raw_debug,
        log_file: maybe_log_file,
        command,
    } = App::parse();

    let (debug, command) = (Into::<AppDebugLevel>::into(raw_debug), command);

    // First, setup logging.
    let mut log_builder = LogSpecification::builder();
    log_builder.default(From::<AppDebugLevel>::from(debug));
    let mut logger = Logger::with(log_builder.build());
    logger = if let Some(log_file) = maybe_log_file {
        logger.log_to_file(FileSpec::default().directory(log_file.path()))
    } else {
        logger
    };

    let logger = logger
        .start()
        .unwrap_or_else(|e| panic!("Logger initialization failed with {}", e));

    info!(
        "Logging at {:?} level",
        logger
            .current_max_level()
            .unwrap_or_else(|e| panic!("Logger interrogation failed with {}", e))
    );

    info!("Executing requested command: {:?}", command);

    let result = match command {
        Commands::Proxy { host, port, path } => {
            match HmdsServerConfiguration::new_by_sense(&path) {
                Ok(config) => proxy::proxy(host, port, config)
                    .await
                    .map_err(AppError::ProxyError),
                Err(e) => Err(AppError::CliError(e.into())),
            }
        }
        Commands::Echo { host, port } => echo::echo(host, port).await.map_err(AppError::ProxyError),
        #[cfg(feature = "domain")]
        Commands::Hmds {
            host,
            port,
            path,
            timeout,
            user,
            group,
        } => {
            let mut ls = supported::LiSession {
                metadata: supported::LiMetadata {
                    uuid: Some(uuid::Uuid::from_bytes(rand::random())),
                    origin: Some((Some("localhost".to_string()), None)),
                    start: Some(chrono::Utc::now()),
                    stop: None,
                    author: Some("Brooks ICS".to_string()),
                    generator: Some("Brooks ICS".to_string()),
                },
                current_span: Span {
                    start: chrono::Utc::now(),
                    stop: None,
                    records: vec![],
                },
            };
            ls.metadata.uuid = Some(uuid::Uuid::from_bytes(rand::random()));

            let lii = LoggingInterfaceImpl {
                last_gen: Arc::new(Mutex::new(chrono::Utc::now())),
                max_archived_spans: 5,
                interval: std::time::Duration::from_secs(10),
                log_session: Arc::new(Mutex::new(ls)),
                spans: Arc::new(Mutex::new(vec![])),
            };

            match tokio::spawn(async move {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        Ok(())
                    },
                    result = hmds::server(host, port, path, timeout, user, group, lii.clone()) => result,
                    result = lii.run() => result,
            }}).await {
                Ok(ics_result) => ics_result.map_err(AppError::IcsError),
                Err(spawn_error) => Err(AppError::IcsError(spawn_error.into())),
            }
        }
        #[cfg(not(feature = "domain"))]
        Commands::Hmds {
            host,
            port,
            timeout,
        } => {
            let mut ls = supported::LiSession {
                metadata: supported::LiMetadata {
                    uuid: Some(uuid::Uuid::from_bytes(rand::random())),
                    origin: Some((Some("localhost".to_string()), None)),
                    start: Some(chrono::Utc::now()),
                    stop: None,
                    author: Some("Brooks ICS".to_string()),
                    generator: Some("Brooks ICS".to_string()),
                },
                current_span: Span {
                    start: chrono::Utc::now(),
                    stop: None,
                    records: vec![],
                },
            };
            ls.metadata.uuid = Some(uuid::Uuid::from_bytes(rand::random()));

            let lii = LoggingInterfaceImpl {
                last_gen: Arc::new(Mutex::new(chrono::Utc::now())),
                max_archived_spans: 5,
                interval: std::time::Duration::from_secs(10),
                log_session: Arc::new(Mutex::new(ls)),
                spans: Arc::new(Mutex::new(vec![])),
            };

            match tokio::spawn(async move {
                tokio::select! {
                        _ = tokio::signal::ctrl_c() => {
                            Ok(())
                        },
                        result = hmds::server(host, port, timeout, lii.clone()) => result,
                        result = lii.run() => result,
                }
            })
            .await
            {
                Ok(ics_result) => ics_result.map_err(AppError::IcsError),
                Err(spawn_error) => Err(AppError::IcsError(spawn_error.into())),
            }
        }
    };

    if let Err(e) = result {
        println!("Error: {e}");
        let mut cli = App::command();
        println!("{}", cli.render_help());
    }
}
