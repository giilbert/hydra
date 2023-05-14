use std::{
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicU32, Ordering},
};

use tokio::sync::mpsc;

use crate::{commands::Command, pty::PtyCommands};

static PTY_ID: AtomicU32 = AtomicU32::new(0);

pub struct State {
    commands: mpsc::Sender<Command>,
    ptys: HashMap<u32, mpsc::Sender<PtyCommands>>,
}

impl State {
    pub fn new(commands: mpsc::Sender<Command>) -> Self {
        Self {
            commands,
            ptys: HashMap::new(),
        }
    }

    pub fn get_pty(&mut self, id: u32) -> Option<&mut mpsc::Sender<PtyCommands>> {
        self.ptys.get_mut(&id)
    }

    pub fn register_pty(&mut self, pty: mpsc::Sender<PtyCommands>) -> u32 {
        let id = PTY_ID.fetch_add(1, Ordering::Relaxed);
        self.ptys.insert(id, pty);
        id
    }

    pub fn remove_pty(&mut self, id: u32) {
        self.ptys.remove(&id);
    }
}
