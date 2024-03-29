use crate::Container;
use shared::prelude::*;
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicI32, Ordering},
        Arc,
    },
};
use tokio::sync::{mpsc, Notify, RwLock};

type Queue = Arc<RwLock<VecDeque<mpsc::Sender<Arc<Container>>>>>;

static CONTAINER_QUEUE_NOTIFY: Notify = Notify::const_new();
static CREATING_CONTAINER_COUNT: AtomicI32 = AtomicI32::new(0);

#[derive(Debug)]
pub struct ContainerPool {
    pub deletion_tx: tokio::sync::mpsc::Sender<String>,
    // mapping docker id to Container
    all_containers: Arc<RwLock<HashMap<String, Arc<Container>>>>,
    queue: Queue,
}

impl ContainerPool {
    pub async fn new(pool_size: u32) -> Self {
        let (deletion_tx, mut deletion_rx) = mpsc::channel(500);
        let containers_in_pool = Arc::<RwLock<HashMap<String, Arc<Container>>>>::default();
        let all_containers = Arc::<RwLock<HashMap<String, Arc<Container>>>>::default();

        let containers_clone = containers_in_pool.clone();
        let deletion_tx_clone = deletion_tx.clone();
        let all_containers_clone = all_containers.clone();

        // spawn a task that listens for container deletion events,
        // and spawns a new container if the pool is not full
        tokio::spawn(async move {
            while let Some(id) = deletion_rx.recv().await {
                let containers_clone = containers_clone.clone();
                let deletion_tx_clone = deletion_tx_clone.clone();
                let all_containers_clone = all_containers_clone.clone();
                all_containers_clone.write().await.remove(&id);

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
                            .insert(new_container.docker_id.clone(), new_container.clone());
                        all_containers_clone
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
            let mut handles = vec![];

            for _ in 0..pool_size {
                let deletion_tx_clone = deletion_tx.clone();
                let containers_clone = containers_in_pool.clone();
                let all_containers_clone = all_containers.clone();
                let handle = tokio::spawn(async move {
                    let new_container = Container::new(Some(deletion_tx_clone.clone()))
                        .await
                        .expect("error creating container.");
                    containers_clone
                        .write()
                        .await
                        .insert(new_container.docker_id.clone(), new_container.clone());
                    all_containers_clone
                        .write()
                        .await
                        .insert(new_container.docker_id.clone(), new_container);
                });
                handles.push(handle);
            }

            futures_util::future::join_all(handles).await;
        }

        let queue = Queue::default();

        let queue_clone = queue.clone();
        tokio::spawn(async move {
            loop {
                CONTAINER_QUEUE_NOTIFY.notified().await;

                let next_id = match containers_in_pool.read().await.keys().next().cloned() {
                    Some(id) => id,
                    None => continue,
                };

                let popped = {
                    let mut queue_clone = queue_clone.write().await;
                    queue_clone.pop_back()
                };

                if let Some(sender) = popped {
                    // unreachable due to precondition
                    let container = containers_in_pool
                        .write()
                        .await
                        .remove(&next_id)
                        .expect("container should still be in the pool");

                    sender
                        .send(container)
                        .await
                        .expect("error fulfilling container request");
                }
            }
        });

        Self {
            all_containers,
            deletion_tx,
            queue,
        }
    }

    pub async fn take_one(&self) -> mpsc::Receiver<Arc<Container>> {
        let (sender, receiver) = mpsc::channel(1);
        self.queue.write().await.push_back(sender);
        CONTAINER_QUEUE_NOTIFY.notify_waiters();
        return receiver;
    }

    pub async fn shutdown(&self) -> Result<()> {
        use tokio::fs;

        for (id, container) in self.all_containers.read().await.iter() {
            fs::remove_dir_all(&container.socket_dir).await?;
            log::info!("Removed socket dir {}", id);
        }

        Ok(())
    }
}
