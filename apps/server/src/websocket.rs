use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::select;

use crate::{app_state::AppState, demo, models::DomainEvent};

#[derive(Debug, Deserialize)]
struct ClientCommand {
    #[serde(rename = "type")]
    command_type: String,
}

pub async fn handle_socket(mut socket: WebSocket, state: AppState, company_id: String) {
    let ready = serde_json::json!({
        "type": "connection.ready",
        "companyId": company_id,
        "mode": "mock"
    });
    if socket
        .send(Message::Text(ready.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    let mut receiver = state.event_tx.subscribe();
    loop {
        select! {
            event = receiver.recv() => {
                match event {
                    Ok(event) if event.company_id == company_id => {
                        if send_event(&mut socket, &event).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            message = socket.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(command) = serde_json::from_str::<ClientCommand>(&text) {
                            if command.command_type == "demo.start" {
                                let _ = demo::start(state.clone(), company_id.clone()).await;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }
}

async fn send_event(socket: &mut WebSocket, event: &DomainEvent) -> Result<(), ()> {
    let text = serde_json::to_string(event).map_err(|_| ())?;
    socket
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| ())
}
