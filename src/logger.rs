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
    io,
    sync::{Arc, Mutex},
};

use awc::http::StatusCode;
use brooks_lib::hmds::{
    self,
    logging::{
        self,
        feed::{AtomEntry, AtomHeader, Atomic},
        format::{Format::CdniLf1, LoggingSerializable},
        supported::LiRecord,
    },
};
use chrono::Utc;
use log::{debug, info};
use tokio::time::sleep;

#[derive(Debug, Clone)]
pub struct LoggingInterfaceImpl {
    pub interval: std::time::Duration,
    pub last_gen: Arc<Mutex<chrono::DateTime<Utc>>>,
    pub max_archived_spans: usize,
    pub log_session: Arc<Mutex<hmds::logging::supported::LiSession>>,
    pub spans: Arc<Mutex<Vec<hmds::logging::session::Span<LiRecord>>>>,
}

impl LoggingInterfaceImpl {
    pub fn log(&self, id: String) -> actix_web::Result<String> {
        let id = id.parse::<usize>().map_err(|e| {
            actix_web::error::InternalError::new(e.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
        })?;

        let session = self.log_session.lock().unwrap();
        let spans = self.spans.lock().unwrap();
        let span = spans.get(id).ok_or(actix_web::error::InternalError::new(
            format!("Invalid log id {}", id),
            StatusCode::INTERNAL_SERVER_ERROR,
        ))?;

        Ok(session.serialize(span, CdniLf1).map_err(|e| {
            actix_web::error::InternalError::new(e.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
        })?)
    }

    pub async fn run(&self) -> io::Result<()> {
        loop {
            info!("Logging Interface Implementation: Preparing to sleep");
            sleep(self.interval).await;

            debug!("Logging Interface Implementation: Archiving active span ...");
            let archived_span_ct = {
                let mut spans = self.spans.lock().unwrap();
                let mut current_session = self.log_session.lock().unwrap();
                let mut last_gen = self.last_gen.lock().unwrap();

                let mut finished_span = current_session.current_span.clone();
                finished_span.stop = Some(chrono::Utc::now());
                spans.push(finished_span);

                let result = spans.len();

                current_session.current_span = logging::session::Span {
                    start: chrono::Utc::now(),
                    stop: None,
                    records: vec![],
                };

                if result > self.max_archived_spans {
                    // If there are more than the max spans, drop the oldest.
                    spans.remove(0);
                }

                *last_gen = chrono::Utc::now();
                result
            };
            debug!(
                "Logging Interface Implementation: Done Archiving active span (archived span count: {archived_span_ct}"
            );
        }
    }
}

impl Atomic for LoggingInterfaceImpl {
    type Error = logging::error::Error;

    fn atomic(&self) -> Result<(AtomHeader, Vec<AtomEntry>), Self::Error> {
        let (last_gen, uuid, generator, author) = {
            let log_session = self.log_session.lock().unwrap();
            let last_gen = self.last_gen.lock().unwrap();
            (
                *last_gen,
                log_session.metadata.uuid,
                log_session.metadata.generator.clone(),
                log_session.metadata.author.clone(),
            )
        };

        let header = AtomHeader {
            title: "Title".to_string(),
            updated: last_gen,
            uuid: uuid.unwrap_or_default(),
            generator: logging::feed::GeneratorId {
                version: "1".to_string(),
                name: generator.unwrap_or("Unknown Generator".to_string()),
            },
            author: author.unwrap_or("Unknown Author".to_string()),
        };

        let mut entries: Vec<AtomEntry> = vec![];
        let spans = {
            let spans = self.spans.lock().unwrap();
            spans.clone()
        };
        for (i, entry) in spans.iter().enumerate() {
            let atom_entry = AtomEntry {
                title: "Entry title.".to_string(),
                id: uuid.unwrap_or_default(),
                updated: entry.stop.unwrap_or(last_gen),
                path: format!("{i}"),
                summary: None,
            };

            entries.push(atom_entry);
        }

        Ok((header, entries))
    }
}
