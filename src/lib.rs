use async_dup::{Arc, Mutex};
use async_tungstenite::tungstenite::Message;
use futures_util::stream::{SplitSink, SplitStream, Stream};
use futures_util::{SinkExt, StreamExt};
use std::future::Future;
use std::marker::{PhantomData, Send};
use std::pin::Pin;

use async_std::task;
use async_tungstenite::{tungstenite::protocol::Role, WebSocketStream};
use sha1::{Digest, Sha1};

use tide::http::format_err;
use tide::http::headers::{CONNECTION, UPGRADE};
use tide::http::upgrade::Connection;
use tide::{Middleware, Request, Response, StatusCode};

pub use async_tungstenite;

const WEBSOCKET_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub struct WebsocketMiddleware<S, H> {
    handler: Arc<H>,
    ghostly: PhantomData<S>,
}

impl<S, H, Fut> WebsocketMiddleware<S, H>
where
    S: Send + Sync + Clone + 'static,
    H: Fn(Request<S>, WebSocketConnection) -> Fut + Sync + Send + 'static,
    Fut: Future<Output = tide::Result<()>> + Send + 'static,
{
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
            ghostly: PhantomData,
        }
    }
}

#[derive(Clone)]
pub struct WebSocketConnection(
    Arc<Mutex<SplitSink<WebSocketStream<Connection>, Message>>>,
    Arc<Mutex<SplitStream<WebSocketStream<Connection>>>>,
);

impl WebSocketConnection {
    pub async fn send_string(&self, s: String) -> tide::Result<()> {
        self.0.lock().send(Message::Text(s)).await?;
        Ok(())
    }

    pub async fn send_bytes(&self, bytes: Vec<u8>) -> tide::Result<()> {
        self.0.lock().send(Message::Binary(bytes)).await?;
        Ok(())
    }

    pub async fn send_json(&self, json: &impl serde::Serialize) -> tide::Result<()> {
        self.send_string(serde_json::to_string(json)?).await
    }

    pub fn new(ws: WebSocketStream<Connection>) -> Self {
        let (s, r) = ws.split();
        Self(Arc::new(Mutex::new(s)), Arc::new(Mutex::new(r)))
    }
}

impl Stream for WebSocketConnection {
    type Item = Result<Message, async_tungstenite::tungstenite::Error>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Option<Self::Item>> {
        Pin::new(&mut *self.1.lock()).poll_next(cx)
    }
}

#[tide::utils::async_trait]
impl<H, S, Fut> Middleware<S> for WebsocketMiddleware<S, H>
where
    H: Fn(Request<S>, WebSocketConnection) -> Fut + Sync + Send + 'static,
    Fut: Future<Output = tide::Result<()>> + Send + 'static,
    S: Send + Sync + Clone + 'static,
{
    async fn handle(&self, req: tide::Request<S>, next: tide::Next<'_, S>) -> tide::Result {
        let upgrade_requested = match (req.header(UPGRADE), req.header(CONNECTION)) {
            (Some(websocket), Some(upgrade))
                if upgrade.as_str().eq_ignore_ascii_case("upgrade")
                    && websocket.as_str().eq_ignore_ascii_case("websocket") =>
            {
                true
            }
            _ => false,
        };

        if !upgrade_requested {
            return Ok(next.run(req).await);
        }

        let header = req
            .header("Sec-Websocket-Key")
            .ok_or_else(|| format_err!("expected sec-websocket-key"))?
            .as_str();

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

        Ok(response)
    }
}
