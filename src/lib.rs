//! it's websockets, for tide!
//!
//! ```rust
//! use async_std::prelude::*;
//! use tide_websockets::{Message, WebSocket};
//!
//! #[async_std::main]
//! async fn main() -> Result<(), std::io::Error> {
//!     let mut app = tide::new();
//!
//!     app.at("/ws")
//!         .get(WebSocket::new(|_request, mut stream| async move {
//!             while let Some(Ok(Message::Text(input))) = stream.next().await {
//!                 let output: String = input.chars().rev().collect();
//!
//!                 stream
//!                     .send_string(format!("{} | {}", &input, &output))
//!                     .await?;
//!             }
//!
//!             Ok(())
//!         }));
//!
//! # if false {
//!     app.listen("127.0.0.1:8080").await?;
//! # }
//!     Ok(())
//! }
//! ```

#![forbid(unsafe_code, future_incompatible)]
#![deny(
    missing_debug_implementations,
    nonstandard_style,
    missing_docs,
    unreachable_pub,
    missing_copy_implementations,
    unused_qualifications
)]

mod handler;
mod websocket_connection;

pub use handler::WebSocket;
pub use websocket_connection::WebSocketConnection;

pub use async_tungstenite;
pub use async_tungstenite::tungstenite::{self, Error, Message};
