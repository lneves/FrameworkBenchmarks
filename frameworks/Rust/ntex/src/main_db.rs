#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::{cell::Cell, pin::Pin, task::Context, task::Poll};

use bytes::BytesMut;
use futures::future::{ok, Future, FutureExt};
use ntex::http::header::{HeaderValue, CONTENT_TYPE, SERVER};
use ntex::http::{HttpService, KeepAlive, Request, Response};
use ntex::service::{Service, ServiceFactory};
use ntex::web::{Error, HttpResponse};
use yarte::Serialize;

mod db;
mod utils;

struct App {
    c1: db::PgConnection,
    c2: db::PgConnection,
    next: Cell<bool>,
}

impl App {
    fn get_db(&self) -> &db::PgConnection {
        if self.next.get() {
            self.next.set(!self.next.get());
            &self.c1
        } else {
            self.next.set(!self.next.get());
            &self.c2
        }
    }
}

impl Service for App {
    type Request = Request;
    type Response = Response;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response, Error>>>>;

    #[inline]
    fn poll_ready(&self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&self, req: Request) -> Self::Future {
        match req.path() {
            "/db" => Box::pin(self.get_db().get_world().map(|body| {
                Ok(HttpResponse::Ok()
                    .header(SERVER, HeaderValue::from_static("N"))
                    .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
                    .body(body))
            })),
            "/fortunes" => Box::pin(self.get_db().tell_fortune().map(|body| {
                Ok(HttpResponse::Ok()
                    .header(SERVER, HeaderValue::from_static("N"))
                    .header(
                        CONTENT_TYPE,
                        HeaderValue::from_static("text/html; charset=utf-8"),
                    )
                    .body(body))
            })),
            "/query" => Box::pin(
                self.get_db()
                    .get_worlds(utils::get_query_param(req.uri().query()))
                    .map(|worlds| {
                        Ok(HttpResponse::Ok()
                            .header(SERVER, HeaderValue::from_static("N"))
                            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
                            .body(worlds.to_bytes::<BytesMut>(35 * worlds.len())))
                    }),
            ),
            "/update" => Box::pin(
                self.get_db()
                    .update(utils::get_query_param(req.uri().query()))
                    .map(|worlds| {
                        Ok(HttpResponse::Ok()
                            .header(SERVER, HeaderValue::from_static("N"))
                            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
                            .body(worlds.to_bytes::<BytesMut>(35 * worlds.len())))
                    }),
            ),
            _ => Box::pin(ok(Response::new(http::StatusCode::NOT_FOUND))),
        }
    }
}

struct AppFactory;

impl ServiceFactory for AppFactory {
    type Config = ();
    type Request = Request;
    type Response = Response;
    type Error = Error;
    type Service = App;
    type InitError = ();
    type Future = Pin<Box<dyn Future<Output = Result<Self::Service, Self::InitError>>>>;

    fn new_service(&self, _: ()) -> Self::Future {
        const DB_URL: &str =
            "postgres://benchmarkdbuser:benchmarkdbpass@tfb-database/hello_world";

        Box::pin(async move {
            Ok(App {
                next: Cell::new(true),
                c1: db::PgConnection::connect(DB_URL).await,
                c2: db::PgConnection::connect(DB_URL).await,
            })
        })
    }
}

#[ntex::main]
async fn main() -> std::io::Result<()> {
    println!("Starting http server: 127.0.0.1:8080");

    ntex::server::build()
        .backlog(1024)
        .bind("techempower", "0.0.0.0:8080", || {
            HttpService::build()
                .keep_alive(KeepAlive::Os)
                .client_timeout(0)
                .disconnect_timeout(0)
                .buffer_params(65535, 65535, 1024)
                .h1(AppFactory)
                .tcp()
        })?
        .start()
        .await
}
