use crate::commands::Command;
use futures_util::{Stream, StreamExt};
use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use protocol::ContainerSent;
use std::{
    io::{Read, Write},
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::atomic::{AtomicU32, Ordering},
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone, Debug)]
pub enum PtyInput {
    Text(String),
    // Resize(u16, u16),
}

pub struct PtyStream {
    stream: Pin<Box<dyn Stream<Item = String> + Send>>,
}

impl PtyStream {
    pub fn new(mut read: Box<dyn Read + Send>) -> Self {
        let (tx, mut rx) = mpsc::channel::<String>(100);

        // TODO: uncurse this
        tokio::task::spawn_blocking(move || {
            let mut buf = [0; 1024];

            loop {
                let n = read.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }

                let s = String::from_utf8_lossy(&buf[..n]).to_string();
                tx.blocking_send(s).unwrap();
            }
        });

        let stream = async_stream::stream! {
            while let Some(s) = rx.recv().await {
                yield s;
            }
        };

        Self {
            stream: Box::pin(stream),
        }
    }
}

impl Deref for PtyStream {
    type Target = Pin<Box<dyn Stream<Item = String> + Send>>;

    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

impl DerefMut for PtyStream {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stream
    }
}

pub struct PtySink {
    write: Box<dyn Write + Send>,
    // master: Box<dyn MasterPty + Send>,
}

impl PtySink {
    fn new(
        write: Box<dyn Write + Send>,
        // master: Box<dyn MasterPty + Send>
    ) -> Self {
        Self {
            write,
            // master
        }
    }

    pub fn input(&mut self, input: PtyInput) -> anyhow::Result<()> {
        match input {
            PtyInput::Text(s) => {
                self.write.write_all(s.as_bytes())?;
            } // PtyInput::Resize(cols, rows) => {
              //     self.master.resize(PtySize {
              //         rows,
              //         cols,
              //         pixel_width: 0,
              //         pixel_height: 0,
              //     })?;
              // }
        }

        Ok(())
    }
}

fn create_raw(
    command: CommandBuilder,
    default_size: PtySize,
) -> anyhow::Result<(PtySink, PtyStream, Box<dyn Child + Send + Sync>)> {
    let pty_system = native_pty_system();

    let pair = pty_system.openpty(default_size)?;

    let child = pair.slave.spawn_command(command)?;

    let reader = pair.master.try_clone_reader()?;
    let writer = pair.master.try_clone_writer()?;

    let stream = PtyStream::new(reader);
    let sink = PtySink::new(
        writer,
        // pair.master
    );

    Ok((sink, stream, child))
}

static ID: AtomicU32 = AtomicU32::new(0);

pub struct Pty {
    pub id: u32,
    pub commands_tx: mpsc::Sender<PtyCommands>,
    pub output: Option<mpsc::Receiver<String>>,
}

impl Pty {
    pub async fn send_output(mut self, commands: mpsc::Sender<Command>) -> anyhow::Result<()> {
        let mut output = self.output.take().unwrap();

        while let Some(x) = output.recv().await {
            commands
                .send(Command::Send(Message::Binary(rmp_serde::to_vec_named(
                    &ContainerSent::PtyOutput {
                        id: self.id,
                        output: x,
                    },
                )?)))
                .await
                .unwrap();
        }

        self.commands_tx.send(PtyCommands::Kill).await?;

        commands.send(Command::RemovePty(self.id.into())).await?;
        commands
            .send(Command::Send(Message::Binary(rmp_serde::to_vec_named(
                &ContainerSent::PtyExit { id: self.id },
            )?)))
            .await?;

        Ok(())
    }
}

#[derive(Debug)]
pub enum PtyCommands {
    Input(PtyInput),
    Kill,
}

pub async fn create(command: CommandBuilder) -> anyhow::Result<Pty> {
    let id = ID.fetch_add(1, Ordering::Relaxed);
    let (mut tx, mut rx, mut child) = create_raw(
        command,
        portable_pty::PtySize {
            rows: 30,
            cols: 90,
            pixel_width: 0,
            pixel_height: 0,
        },
    )?;

    let (commands_tx, mut commands_rx) = mpsc::channel(100);
    let (output_tx, output_rx) = mpsc::channel(100);

    tokio::spawn(async move {
        while let Some(x) = commands_rx.recv().await {
            match x {
                PtyCommands::Input(input) => {
                    tx.input(input)?;
                }
                PtyCommands::Kill => {
                    child.kill()?;
                    break;
                }
            }
        }

        Ok::<_, anyhow::Error>(())
    });

    tokio::spawn(async move {
        while let Some(x) = rx.next().await {
            output_tx.send(x).await?;
        }

        Ok::<_, anyhow::Error>(())
    });

    Ok(Pty {
        id,
        commands_tx,
        output: Some(output_rx),
    })
}
