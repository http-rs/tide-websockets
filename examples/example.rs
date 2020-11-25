use async_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt};
use tide_websockets::WebsocketMiddleware;

#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    env_logger::init();
    let mut app = tide::new();
    app.at("/")
        .with(WebsocketMiddleware::new(|_request, stream| async move {
            let (mut sender, mut receiver) = stream.split();
            while let Some(Ok(message)) = receiver.next().await {
                sender
                    .send(Message::Text(format!("hello: {}", message)))
                    .await
                    .ok();
            }
        }))
        .get(|_| async move { Ok("this was not a websocket request") });

    app.listen("127.0.0.1:8080").await?;

    Ok(())
}
