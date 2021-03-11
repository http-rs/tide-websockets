use async_std::channel::{unbounded, Sender};
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tide::http::mime;
use tide::log::*;
use tide::Response;
use tide_websockets::{Message, WebSocket};

struct State {
    connections: HashMap<u32, Sender<Message>>,
    tick_count: u32,
    next_id: u32,
}
impl State {
    fn add_connection(&mut self, connection_tx: Sender<Message>) {
        self.connections.insert(self.next_id, connection_tx);
        self.next_id += 1;
    }
}
#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    env_logger::init();

    let state = Arc::new(Mutex::new(State {
        connections: HashMap::new(),
        tick_count: 0,
        next_id: 0,
    }));

    let mut app = tide::with_state(Arc::clone(&state));

    async_std::task::spawn(app_loop(state));

    app.at("/").get(|_| async move {
        Ok(Response::builder(200)
            .body(INDEX_HTML)
            .content_type(mime::HTML)
            .build())
    });

    app.at("/ws").get(WebSocket::new(
        |request: tide::Request<Arc<Mutex<State>>>, stream| async move {
            info!("new websocket opened");
            let state = request.state();

            // Channel for outgoing WebSocket messages from other threads
            let (send_ws_msg_tx, send_ws_msg_rx) = unbounded::<Message>();
            let mut send_ws_msg_rx = send_ws_msg_rx.fuse();

            state.lock().unwrap().add_connection(send_ws_msg_tx);

            let mut stream = stream.fuse();
            loop {
                let ws_msg = futures::select! {

                         ws_msg = stream.select_next_some() => {
                             match ws_msg {
                                //  _ => None,
                                 Ok(Message::Close(_)) => {
                                     println!("peer disconnected");
                                     break
                                 },
                                 Ok(Message::Ping(data)) => Some(Message::Pong(data)),
                                 Ok(Message::Pong(_)) => None,
                                 Ok(Message::Binary(_)) => None,
                                 Ok(Message::Text(text)) => {
                                     println!("Message received: {}",text);
                                     None
                                 },
                                 Err(_) =>{
                                     // done
                                     None
                                 }
                             }
                         },

                         // Handle WebSocket messages we created asynchronously
                         // to send them out now
                         ws_msg = send_ws_msg_rx.select_next_some() => Some(ws_msg),

                         // Once we're done, break the loop and return
                         complete => break,

                };

                // If there's a message to send out, do so now
                if let Some(ws_msg) = ws_msg {
                    stream.send(ws_msg).await?;
                }
            }

            // TODO: socket closed or errored, delete from remote handles

            Ok(())
        },
    ));

    app.listen("127.0.0.1:8080").await?;

    Ok(())
}

async fn app_loop(state: Arc<Mutex<State>>) -> () {
    loop {
        let (tick_count, connections) = {
            let mut state = state.lock().unwrap();
            println!(
                "Tick #{}. Connection count is {}",
                state.tick_count,
                state.connections.len()
            );
            let tick_count = state.tick_count;
            state.tick_count += 1;

            let connections = state.connections.clone();
            (tick_count, connections)
        };
        let connection_count = connections.len();

        for conn in connections {
            match conn
                .1
                .send(Message::Text(format!(
                    r#"{{ "ticks": {}, "connections": {} }}"#,
                    tick_count, connection_count
                )))
                .await
            {
                Ok(_) => (),
                Err(_e) => {
                    //println!("error: {}", e);
                    state.lock().unwrap().connections.remove(&conn.0);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(1000));
    }
}

const INDEX_HTML: &str = r##"
<!DOCTYPE html>
<html>
<head>
    <title>futures::select!() example</title>
    <script type="text/javascript">
        var count;
        var tick;
        var websocketConnection;
        function onServerMessage(event) {
            var msg;
            try {
                msg = JSON.parse(event.data);
            } catch (e) {
                console.log("bad: " + e);
                return;
            }
            count.innerText = msg.connections;
            tick.innerText = msg.ticks;
            websocketConnection.send("Hello there.");
        }
        window.onload = function () {
            count = document.getElementById("count");
            tick = document.getElementById("tick");
            var wsHost = window.location.hostname;
            var wsPort = window.location.port;
            if (wsPort)
                wsPort = ":" + wsPort;
            var wsUrl = "ws://" + wsHost + wsPort + "/ws";
            console.log("ws url:" + wsUrl);

            websocketConnection = new WebSocket(wsUrl);
            websocketConnection.addEventListener("message", onServerMessage);
        };
    </script>
</head>
<body>
    connection count: <span id="count">?</span> tick: <span id="tick">?</span>
</body>
</html>
"##;
