use async_std::prelude::*;
use tide_websockets::WebsocketMiddleware;

#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    env_logger::init();
    let mut app = tide::new();
    app.at("/")
        .with(WebsocketMiddleware::new(
            |_request, mut stream| async move {
                while let Some(Ok(message)) = stream.next().await {
                    let input = message.into_text().unwrap();
                    let output: String = input.chars().rev().collect();

                    stream
                        .send_string(format!("{} | {}", &input, &output))
                        .await?;
                }

                Ok(())
            },
        ))
        .get(|_| async move { Ok("this was not a websocket request") });

    app.listen("127.0.0.1:8080").await?;

    Ok(())
}
