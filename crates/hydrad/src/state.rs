use crate::{commands::Command, pty::PtyCommands};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU32, Ordering},
};
use tokio::sync::mpsc;

static PTY_ID: AtomicU32 = AtomicU32::new(0);

pub struct State {
    pub commands: Mutex<mpsc::Sender<Command>>,
    ptys: Mutex<HashMap<u32, mpsc::Sender<PtyCommands>>>,
}

impl State {
    pub fn new(commands: mpsc::Sender<Command>) -> Self {
        Self {
            commands: Mutex::new(commands),
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
}
