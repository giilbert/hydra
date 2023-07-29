use crate::{commands::Command, proxy::WebSocketCommands, pty::PtyCommands};
use parking_lot::{Mutex, RwLock};
use shared::protocol::WebSocketMessage;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU32, Ordering},
};
use tokio::sync::mpsc;

static PTY_ID: AtomicU32 = AtomicU32::new(0);

pub struct State {
    pub commands: Mutex<mpsc::Sender<Command>>,
    websocket_commands: RwLock<HashMap<u32, mpsc::Sender<WebSocketCommands>>>,
    ptys: Mutex<HashMap<u32, mpsc::Sender<PtyCommands>>>,
}

impl State {
    pub fn new(commands: mpsc::Sender<Command>) -> Self {
        Self {
            commands: Mutex::new(commands),
            websocket_commands: RwLock::new(HashMap::new()),
            ptys: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_pty(&self, id: u32) -> Option<mpsc::Sender<PtyCommands>> {
        self.ptys.lock().get(&id).cloned()
    }

    pub fn register_pty(&self, pty: mpsc::Sender<PtyCommands>) -> u32 {
        let id = PTY_ID.fetch_add(1, Ordering::Relaxed);
        self.ptys.lock().insert(id, pty);
        id
    }

    pub fn remove_pty(&self, id: u32) {
        self.ptys.lock().remove(&id);
    }

    pub fn add_websocket(&self, id: u32, websocket: mpsc::Sender<WebSocketCommands>) {
        self.websocket_commands.write().insert(id, websocket);
    }

    pub fn get_websocket(&self, id: u32) -> Option<mpsc::Sender<WebSocketCommands>> {
        self.websocket_commands.read().get(&id).cloned()
    }

    pub async fn send_ws_message(&self, id: u32, command: WebSocketMessage) {
        self.get_websocket(id)
            .unwrap()
            .send(WebSocketCommands::Send(command))
            .await
            .unwrap();
    }

    pub fn remove_websocket(&self, id: u32) {
        self.websocket_commands.write().remove(&id);
    }
}
