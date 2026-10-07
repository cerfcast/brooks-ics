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

use actix_web::web::{self, Bytes};
use actix_web::{HttpRequest, dev::PeerAddr};
use actix_web::{HttpResponseBuilder, Responder};
use awc::http::StatusCode;
use log::Level::Trace;
use log::info;

async fn handle_echo(req: HttpRequest, body: Bytes, _peer: PeerAddr) -> impl Responder {
    let mut res = HttpResponseBuilder::new(StatusCode::OK).take();

    for header in req.headers() {
        res = res.insert_header(header).take();
    }

    res.take().body(body)
}

pub async fn echo(ip: String, port: u16) -> std::io::Result<()> {
    use actix_web::{App, HttpServer, middleware::Logger};

    info!("HTTP Echo server running on {}:{}", ip, port);
    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default().log_level(Trace))
            .wrap(actix_cors::Cors::permissive())
            .route("/{tail}*", web::get().to(handle_echo))
            .route("/{tail}*", web::post().to(handle_echo))
    })
    .bind((ip, port))?
    .run()
    .await
}
