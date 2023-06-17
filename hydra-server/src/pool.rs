use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicI32, Ordering},
        Arc,
    },
};

use tokio::sync::{mpsc, Notify, RwLock};

use crate::Container;

type Queue = Arc<RwLock<VecDeque<mpsc::Sender<Container>>>>;

static CONTAINER_QUEUE_NOTIFY: Notify = Notify::const_new();
static CREATING_CONTAINER_COUNT: AtomicI32 = AtomicI32::new(0);

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

                // TODO: test this?
                let amount_after_creation = containers_clone.read().await.len() as i32
                    + CREATING_CONTAINER_COUNT.load(Ordering::SeqCst);

                if amount_after_creation < pool_size as i32 {
                    tokio::spawn(async move {
                        CREATING_CONTAINER_COUNT.fetch_add(1, Ordering::Relaxed);

                        let new_container = Container::new(Some(deletion_tx_clone.clone()))
                            .await
                            .expect("error creating container.");
                        containers_clone
                            .write()
                            .await
                            .insert(new_container.docker_id.clone(), new_container);

                        CONTAINER_QUEUE_NOTIFY.notify_waiters();
                        CREATING_CONTAINER_COUNT.fetch_sub(1, Ordering::Relaxed);
                    });
                }
            }
        });

        // spawn the initial containers that are in the pool
        {
            for _ in 0..pool_size {
                let deletion_tx_clone = deletion_tx.clone();
                let containers_clone = containers.clone();
                tokio::spawn(async move {
                    let new_container = Container::new(Some(deletion_tx_clone.clone()))
                        .await
                        .expect("error creating container.");
                    containers_clone
                        .write()
                        .await
                        .insert(new_container.docker_id.clone(), new_container);
                });
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
        log::info!("Take one called..");
        let (sender, receiver) = mpsc::channel(1);
        self.queue.write().await.push_back(sender);
        CONTAINER_QUEUE_NOTIFY.notify_waiters();
        return receiver;
    }
}
