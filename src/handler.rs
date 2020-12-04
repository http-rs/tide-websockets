use std::future::Future;
use std::marker::{PhantomData, Send};

use crate::async_tungstenite::WebSocketStream;
use crate::tungstenite::protocol::Role;
use crate::WebSocketConnection;

use async_dup::Arc;
use async_std::task;
use sha1::{Digest, Sha1};

use tide::http::format_err;
use tide::http::headers::{HeaderName, CONNECTION, UPGRADE};
use tide::{Middleware, Request, Response, Result, StatusCode};

const WEBSOCKET_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub struct WebSocket<S, H> {
    handler: Arc<H>,
    ghostly_apparition: PhantomData<S>,
}

enum UpgradeStatus<S> {
    Upgraded(Result<Response>),
    NotUpgraded(Request<S>),
}
use UpgradeStatus::{NotUpgraded, Upgraded};

fn header_eq_ignore_case<T>(req: &Request<T>, header_name: HeaderName, value: &str) -> bool {
    req.header(header_name)
        .map(|h| h.as_str().eq_ignore_ascii_case(value))
        .unwrap_or(false)
}

impl<S, H, Fut> WebSocket<S, H>
where
    S: Send + Sync + Clone + 'static,
    H: Fn(Request<S>, WebSocketConnection) -> Fut + Sync + Send + 'static,
    Fut: Future<Output = tide::Result<()>> + Send + 'static,
{
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
            ghostly_apparition: PhantomData,
        }
    }

    async fn handle_upgrade(&self, req: Request<S>) -> UpgradeStatus<S> {
        let connection_upgrade = header_eq_ignore_case(&req, CONNECTION, "upgrade");
        let upgrade_to_websocket = header_eq_ignore_case(&req, UPGRADE, "websocket");
        let upgrade_requested = connection_upgrade && upgrade_to_websocket;

        if !upgrade_requested {
            return NotUpgraded(req);
        }

        let header = match req.header("Sec-Websocket-Key") {
            Some(h) => h.as_str(),
            None => return Upgraded(Err(format_err!("expected sec-websocket-key"))),
        };

        let mut response = Response::new(StatusCode::SwitchingProtocols);

        response.insert_header(UPGRADE, "websocket");
        response.insert_header(CONNECTION, "Upgrade");
        let hash = Sha1::new().chain(header).chain(WEBSOCKET_GUID).finalize();
        response.insert_header("Sec-Websocket-Accept", base64::encode(&hash[..]));
        response.insert_header("Sec-Websocket-Version", "13");

        let http_res: &mut tide::http::Response = response.as_mut();
        let upgrade_receiver = http_res.recv_upgrade().await;
        let handler = self.handler.clone();

        task::spawn(async move {
            if let Some(stream) = upgrade_receiver.await {
                let stream = WebSocketStream::from_raw_socket(stream, Role::Server, None).await;
                handler(req, WebSocketConnection::new(stream)).await
            } else {
                Err(format_err!("never received an upgrade!"))
            }
        });

        Upgraded(Ok(response))
    }
}

#[tide::utils::async_trait]
impl<H, S, Fut> tide::Endpoint<S> for WebSocket<S, H>
where
    H: Fn(Request<S>, WebSocketConnection) -> Fut + Sync + Send + 'static,
    Fut: Future<Output = tide::Result<()>> + Send + 'static,
    S: Send + Sync + Clone + 'static,
{
    async fn call(&self, req: Request<S>) -> tide::Result {
        match self.handle_upgrade(req).await {
            Upgraded(result) => result,
            NotUpgraded(_) => Ok(Response::new(StatusCode::UpgradeRequired)),
        }
    }
}

#[tide::utils::async_trait]
impl<H, S, Fut> Middleware<S> for WebSocket<S, H>
where
    H: Fn(Request<S>, WebSocketConnection) -> Fut + Sync + Send + 'static,
    Fut: Future<Output = tide::Result<()>> + Send + 'static,
    S: Send + Sync + Clone + 'static,
{
    async fn handle(&self, req: tide::Request<S>, next: tide::Next<'_, S>) -> tide::Result {
        match self.handle_upgrade(req).await {
            Upgraded(result) => result,
            NotUpgraded(req) => Ok(next.run(req).await),
        }
    }
}
