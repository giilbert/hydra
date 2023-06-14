use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use tokio::sync::{mpsc, Notify, RwLock};

use crate::Container;

type Queue = Arc<RwLock<VecDeque<mpsc::Sender<Container>>>>;

static CONTAINER_QUEUE_NOTIFY: Notify = Notify::const_new();

#[derive(Debug)]
pub struct ContainerPool {
    pub deletion_tx: tokio::sync::mpsc::Sender<String>,
    // mapping docker id to Container
    _containers: Arc<RwLock<HashMap<String, Container>>>,
    queue: Queue,
}

impl ContainerPool {
    pub async fn new(pool_size: u32) -> Self {
        let (deletion_tx, mut deletion_rx) = mpsc::channel(500);
        let containers = Arc::<RwLock<HashMap<String, Container>>>::default();

        let containers_clone = containers.clone();
        let deletion_tx_clone = deletion_tx.clone();

        // spawn a task that listens for container deletion events,
        // and spawns a new container if the pool is not full
        tokio::spawn(async move {
            while let Some(_) = deletion_rx.recv().await {
                let containers_clone = containers_clone.clone();
                let deletion_tx_clone = deletion_tx_clone.clone();

                // if the amount of containers in the pool is less
                // than the declared pool_size, spawn a new container
                //
                // TODO:
                // If multiple containers are deleted at almost the same time,
                // this will spawn multiple containers. This is not a big deal, as
                // the pool will eventually reach the desired size and we have a
                // sizeable amount of memory to spare, but it would be nice to
                // avoid this.
                if containers_clone.read().await.len() < pool_size as usize {
                    tokio::spawn(async move {
                        let new_container = Container::new(Some(deletion_tx_clone.clone()))
                            .await
                            .expect("error creating container.");
                        containers_clone
                            .write()
                            .await
                            .insert(new_container.docker_id.clone(), new_container);

                        CONTAINER_QUEUE_NOTIFY.notify_waiters();
                    });
                }
            }
        });

        // spawn the initial containers that are in the pool
        {
            for _ in 0..pool_size {
                let deletion_tx_clone = deletion_tx.clone();
                let containers_clone = containers.clone();
                let new_container = Container::new(Some(deletion_tx_clone.clone()))
                    .await
                    .expect("error creating container.");
                containers_clone
                    .write()
                    .await
                    .insert(new_container.docker_id.clone(), new_container);
            }
        }

        let queue = Queue::default();

        let queue_clone = queue.clone();
        let containers_clone = containers.clone();
        tokio::spawn(async move {
            loop {
                CONTAINER_QUEUE_NOTIFY.notified().await;

                let next_id = match containers_clone.read().await.keys().next().cloned() {
                    Some(id) => id,
                    None => continue,
                };

                let popped = {
                    let mut queue_clone = queue_clone.write().await;
                    queue_clone.pop_back()
                };

                if let Some(sender) = popped {
                    // unreachable due to precondition
                    let container = containers_clone.write().await.remove(&next_id).unwrap();

                    sender
                        .send(container)
                        .await
                        .expect("error fulfilling container request");
                }
            }
        });

        Self {
            _containers: containers,
            deletion_tx,
            queue,
        }
    }

    pub async fn take_one(&mut self) -> mpsc::Receiver<Container> {
        log::info!("Take one");
        let (sender, receiver) = mpsc::channel(1);
        self.queue.write().await.push_back(sender);
        CONTAINER_QUEUE_NOTIFY.notify_waiters();
        return receiver;
    }
}
