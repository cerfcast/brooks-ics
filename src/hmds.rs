// brooks-ics, Copyright 2026, Will Hawkins
//
// This file is part of brooks-ics.
//
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

use std::{
    collections::HashMap,
    io,
    sync::{Arc, Mutex},
};

use actix_web::{App, HttpServer, delete, get, middleware::Logger, put, web};
use brooks_lib::{
    cdni::md::spec::TypedHostMetadata,
    integrations::{self, hmds::ExpirableJsonValue},
};
use chrono::{DateTime, Duration, Utc};

use log::info;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(feature = "domain")]
use clio::ClioPath;
#[cfg(feature = "domain")]
use std::{fs, path::Path};
#[cfg(feature = "domain")]
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};

pub type Hmds = (DateTime<Utc>, TypedHostMetadata<()>);

#[derive(Debug, Clone)]
pub struct HmdsWebConfiguration {
    ip: String,
    port: u16,
    hmds: Arc<Mutex<HashMap<String, Hmds>>>,
    timeout: Duration,
}

#[cfg(feature = "domain")]
pub struct HmdsDomainConfiguration {
    hmds: Arc<Mutex<HashMap<String, Hmds>>>,
    server_path: ClioPath,
    user: Option<String>,
    group: Option<String>,
}
#[cfg(not(feature = "domain"))]
pub struct HmdsDomainConfiguration {}

#[derive(Serialize, Deserialize)]
struct Query {
    host: String,
}

#[delete("/update/")]
async fn delete_hmd(
    query: web::Query<Query>,
    data: web::Data<HmdsWebConfiguration>,
) -> actix_web::Result<String> {
    let address = &query.host;

    let mut hmds = data
        .hmds
        .lock()
        .map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string()))?;

    hmds.remove(address);

    Ok("".to_string())
}

#[put("/update/")]
async fn add_hmd(
    query: web::Query<Query>,
    info: web::Json<TypedHostMetadata<()>>,
    data: web::Data<HmdsWebConfiguration>,
) -> actix_web::Result<String> {
    let address = &query.host;

    let mut hmds = data
        .hmds
        .lock()
        .map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string()))?;

    let expiry = Utc::now() + data.timeout;

    let response = serde_json::to_string(&ExpirableJsonValue {
        expiry,
        host: address.to_string(),
        value: serde_json::to_value(&info.0).expect("Could not serialize an HMD entry"),
    })?;

    hmds.insert(address.clone(), (expiry, info.0));

    Ok(response)
}

#[get("/status/")]
async fn status(data: web::Data<HmdsWebConfiguration>) -> actix_web::Result<String> {
    let res: Vec<_> = data
        .hmds
        .lock()
        .map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string()))?
        .iter()
        .map(|(host, (timeout, value))| ExpirableJsonValue {
            expiry: *timeout,
            host: host.to_string(),
            value: serde_json::to_value(value).expect("Could not serialize a HMD entry"),
        })
        .collect();
    Ok(serde_json::to_string(&res)?)
}

#[get("/qry/")]
async fn qry(
    query: web::Query<Query>,
    data: web::Data<HmdsWebConfiguration>,
) -> actix_web::Result<String> {
    let query = &query.host;

    let found = {
        let hmds = data.hmds.lock().unwrap();
        hmds.get_key_value(query)
            .map(|(key, (ts, f))| (key.clone(), (*ts, f.clone())))
    };

    match found {
        Some((key, (timestamp, found))) => Ok(serde_json::to_string(
            &integrations::hmds::ExpirableJsonValue {
                expiry: timestamp,
                host: key,
                value: serde_json::to_value(found)?,
            },
        )?),
        None => Ok(serde_json::to_string(&Value::Null)?),
    }
}

#[allow(unused)]
pub async fn server(
    ip: String,
    port: u16,
    #[cfg(feature = "domain")] path: ClioPath,
    timeout: Duration,
    user: Option<String>,
    group: Option<String>,
) -> io::Result<()> {
    let metadata: HashMap<String, (DateTime<Utc>, TypedHostMetadata<()>)> = HashMap::new();
    let metadata = Arc::new(Mutex::new(metadata));

    let web_configuration = HmdsWebConfiguration {
        ip: ip.clone(),
        port,
        hmds: metadata.clone(),
        timeout,
    };

    #[cfg(feature = "domain")]
    let domain_configuration = HmdsDomainConfiguration {
        hmds: metadata.clone(),
        server_path: path.clone(),
        user: user.clone(),
        group: group.clone(),
    };
    #[cfg(not(feature = "domain"))]
    let domain_configuration = HmdsDomainConfiguration {};

    let result = tokio::spawn(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                Ok(())
            },
            result = domain_hmds(domain_configuration) => result,
            result = {
                info!("About to start HTTP HMDS server on http://{ip}:{port}/");
                let inner_web_configuration = web_configuration.clone();
                HttpServer::new(move || {
                    App::new()
                    .app_data(web::Data::new(inner_web_configuration.clone()))
                    .wrap(Logger::default())
                    .wrap(actix_cors::Cors::permissive())
                    .service(delete_hmd)
                    .service(add_hmd)
                    .service(status)
                    .service(qry)
                })
                .bind((web_configuration.ip, web_configuration.port))?
                .run()
            } => result,
        }
    })
    .await;

    // Instead of using a Drop implementation (because that does not let us show an error),
    // delete the path to the domain socket here.
    #[cfg(feature = "domain")]
    fs::remove_file(path.path())?;

    result?
}

#[cfg(feature = "domain")]
async fn write_entire(s: &mut UnixStream, d: &[u8]) -> io::Result<usize> {
    s.write_u64_le(d.len() as u64).await?;

    let mut already_sent = 0usize;
    loop {
        s.writable().await?;
        match s.try_write(&d[already_sent..]) {
            Ok(n) => {
                if n == 0 {
                    return Ok(already_sent);
                }
                already_sent += n;
                if already_sent == d.len() {
                    return Ok(already_sent);
                }
                continue;
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(feature = "domain")]
async fn read_entire(s: &mut UnixStream, d: &mut [u8]) -> io::Result<usize> {
    let mut already_read = 0usize;
    loop {
        s.readable().await?;
        match s.try_read(&mut d[already_read..]) {
            Ok(n) => {
                if n == 0 {
                    return Ok(already_read);
                }
                already_read += n;
                if already_read == d.len() {
                    return Ok(already_read);
                }
                continue;
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(feature = "domain")]
pub async fn domain_hmds(config: HmdsDomainConfiguration) -> io::Result<()> {
    let server_path = &config.server_path;
    let socket = UnixListener::bind(server_path.path())?;

    #[cfg(feature = "domain")]
    {
        // Now that we have bound, let's try to change the permissions on that file.
        let user = if let Some(user) = config.user {
            let user = pwd_grp::getpwnam(&user)?.ok_or(io::Error::other(format!(
                "Cannot find information about user {}",
                user
            )))?;
            Some(user.uid)
        } else {
            None
        };

        let group = if let Some(group) = config.group {
            let group = pwd_grp::getgrnam(&group)?.ok_or(io::Error::other(format!(
                "Cannot find information about group {}",
                group
            )))?;
            Some(group.gid)
        } else {
            None
        };
        std::os::unix::fs::chown(server_path.path(), user, group)?;
    }

    #[cfg(not(feature = "domain"))]
    {
        if config.user.is_some() || config.group.is_some() {
            eprintln!(
                "Configuring a user/group for the domain socket on a non-UNIX platform is a no-op."
            )
        }
    }

    info!("Listening for HMDS queries on UNIX domain socket at {server_path}");
    loop {
        // Wait for the socket to be readable
        let (mut client, _) = socket.accept().await?;

        client.readable().await?;

        let incoming_buffer_size = match client.read_u64_le().await {
            Ok(ibs) => ibs,
            Err(_) => todo!(),
        };

        let mut buf: Vec<u8> = vec![0; incoming_buffer_size as usize];

        if read_entire(&mut client, &mut buf).await.is_err() {
            todo!();
        };

        let query = String::from_utf8(buf).expect("Could not convert into string");

        let found = {
            let hmds = config.hmds.lock().unwrap();
            hmds.get_key_value(&query)
                .map(|(key, (ts, f))| (key.clone(), (*ts, f.clone())))
        };

        match found {
            Some((key, (timestamp, found))) => {
                write_entire(
                    &mut client,
                    serde_json::to_string(&integrations::hmds::ExpirableJsonValue {
                        expiry: timestamp,
                        host: key,
                        value: serde_json::to_value(found)?,
                    })
                    .expect("Could not serialize")
                    .as_bytes(),
                )
                .await?;
            }
            None => {
                write_entire(&mut client, &[]).await?;
            }
        }
    }
}

#[cfg(not(feature = "domain"))]
mod never_ready {
    use std::{
        io,
        pin::Pin,
        task::{Context, Poll},
    };
    pub(crate) struct NeverReady {}

    impl Future for NeverReady {
        type Output = io::Result<()>;

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }
}

#[cfg(not(feature = "domain"))]
pub async fn domain_hmds(_: HmdsDomainConfiguration) -> io::Result<()> {
    info!("UNIX domain socket access to the proxy server is not supported.");
    never_ready::NeverReady {}.await
}

#[cfg(feature = "domain")]
#[allow(unused)]
pub async fn query_proxy_config(query: &str, server_path: &Path) -> io::Result<String> {
    let mut socket = UnixStream::connect(server_path).await?;

    // Wait for the socket to be readable
    socket.writable().await?;

    write_entire(&mut socket, query.as_bytes()).await?;

    socket.readable().await?;

    let incoming_buffer_size = match socket.read_u64_le().await {
        Ok(ibs) => ibs,
        Err(_) => todo!(),
    };

    let mut buf: Vec<u8> = vec![0; incoming_buffer_size as usize];

    if read_entire(&mut socket, &mut buf).await.is_err() {
        todo!();
    };

    let s = String::from_utf8(buf).map_err(|_| io::ErrorKind::InvalidData)?;
    Ok(s)
}
