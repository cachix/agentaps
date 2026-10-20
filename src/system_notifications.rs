//! Linux notifications use one worker and D-Bus connection for their whole lifetime.
use ashpd::zbus::{self, zvariant::Value};
use futures_util::{FutureExt, StreamExt};
use std::{cell::RefCell, collections::HashMap, sync::mpsc, thread, time::Duration};

const DESTINATION: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const INTERFACE: &str = "org.freedesktop.Notifications";

pub(crate) struct Notification {
    pub tag: String,
    pub title: String,
    pub body: String,
}

enum Command {
    Show(Notification),
    Dismiss(String),
    DismissAll,
}

enum Event {
    Activated(String),
    Closed(String),
}

struct Worker {
    commands: smol::channel::Sender<Command>,
    events: mpsc::Receiver<Event>,
    dismissed: Option<String>,
    _thread: thread::JoinHandle<()>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Wake the worker even while it is waiting for a notification action.
        self.commands.close();
    }
}

#[derive(Default)]
pub(crate) struct Service {
    worker: RefCell<Option<Worker>>,
    #[cfg(test)]
    address: Option<String>,
}

impl Service {
    pub fn show(&self, notification: Notification) -> Result<(), String> {
        let mut worker = self.worker.borrow_mut();
        if worker
            .as_ref()
            .is_none_or(|worker| worker.commands.is_closed())
        {
            let (commands, receiver) = smol::channel::bounded(64);
            let (events, responses) = mpsc::channel();
            #[cfg(test)]
            let address = self.address.clone();
            #[cfg(not(test))]
            let address: Option<String> = None;
            let handle = thread::Builder::new()
                .name("notifications".into())
                .spawn(move || {
                    smol::block_on(async move {
                        let result = async {
                            let builder = match address.as_deref() {
                                Some(address) => zbus::connection::Builder::address(address)?,
                                None => zbus::connection::Builder::session()?,
                            };
                            let connection = builder
                                .method_timeout(Duration::from_secs(5))
                                .build()
                                .await?;
                            run(connection, receiver, events).await
                        }
                        .await;
                        if let Err(error) = result {
                            eprintln!("Desktop notification service stopped: {error}");
                        }
                    })
                })
                .map_err(|error| error.to_string())?;
            *worker = Some(Worker {
                commands,
                events: responses,
                dismissed: None,
                _thread: handle,
            });
        }
        let worker = worker.as_mut().unwrap();
        let tag = notification.tag.clone();
        worker
            .commands
            .try_send(Command::Show(notification))
            .map_err(|error| error.to_string())?;
        if worker.dismissed.as_deref() == Some(&tag) {
            worker.dismissed = None;
        }
        Ok(())
    }

    pub fn dismiss(&self, tag: &str) {
        if let Some(worker) = self.worker.borrow_mut().as_mut()
            && worker.dismissed.as_deref() != Some(tag)
            && worker
                .commands
                .try_send(Command::Dismiss(tag.into()))
                .is_ok()
        {
            worker.dismissed = Some(tag.into());
        }
    }

    pub fn dismiss_all(&self) {
        if let Some(worker) = self.worker.borrow_mut().as_mut()
            && worker.commands.force_send(Command::DismissAll).is_ok()
        {
            worker.dismissed = None;
        }
    }

    pub fn take_actions(&self) -> Vec<String> {
        let mut actions = Vec::new();
        if let Some(worker) = self.worker.borrow_mut().as_mut() {
            while let Ok(event) = worker.events.try_recv() {
                match event {
                    Event::Activated(tag) => actions.push(tag),
                    Event::Closed(tag) => {
                        if worker.dismissed.as_deref() == Some(&tag) {
                            worker.dismissed = None;
                        }
                    }
                }
            }
        }
        actions
    }
}

async fn run(
    connection: zbus::Connection,
    commands: smol::channel::Receiver<Command>,
    events: mpsc::Sender<Event>,
) -> zbus::Result<()> {
    let proxy = zbus::Proxy::new(&connection, DESTINATION, PATH, INTERFACE).await?;
    // Subscribe before Notify: fast clicks or closes may precede its reply.
    let mut signals = proxy.receive_all_signals().await?;
    let mut owners = proxy.receive_owner_changed().await?;
    let mut active: HashMap<u32, String> = HashMap::new();
    enum Incoming {
        OwnerChanged(bool),
        Signal(Option<zbus::Message>),
        Command(Result<Command, smol::channel::RecvError>),
    }
    loop {
        let incoming = {
            let command = commands.recv().fuse();
            let signal = signals.next().fuse();
            let owner = owners.next().fuse();
            futures_util::pin_mut!(command, signal, owner);
            futures_util::select_biased! {
                owner = owner => Incoming::OwnerChanged(owner.is_some()),
                signal = signal => Incoming::Signal(signal),
                command = command => Incoming::Command(command),
            }
        };
        match incoming {
            Incoming::OwnerChanged(connected) => {
                if !connected {
                    break;
                }
                // Server-assigned IDs are invalid after the notification daemon restarts.
                for (_, tag) in active.drain() {
                    let _ = events.send(Event::Closed(tag));
                }
            }
            Incoming::Signal(signal) => {
                let Some(signal) = signal else {
                    break;
                };
                match signal.header().member().map(|name| name.as_str()) {
                    Some("NotificationClosed") => {
                        if let Ok((id, _reason)) = signal.body().deserialize::<(u32, u32)>()
                            && let Some(tag) = active.remove(&id)
                        {
                            let _ = events.send(Event::Closed(tag));
                        }
                    }
                    Some("ActionInvoked") => {
                        if let Ok((id, action)) = signal.body().deserialize::<(u32, String)>()
                            && action == "default"
                            && let Some(tag) = active.remove(&id)
                        {
                            let _ = events.send(Event::Activated(tag));
                        }
                    }
                    _ => {}
                }
            }
            Incoming::Command(command) => {
                if commands.is_closed() {
                    break;
                }
                let Ok(command) = command else {
                    break;
                };
                match command {
                    Command::Show(notification) => {
                        let previous = active
                            .iter()
                            .find_map(|(id, tag)| (tag == &notification.tag).then_some(*id));
                        let hints: HashMap<&str, Value<'_>> = HashMap::new();
                        let result: zbus::Result<u32> = proxy
                            .call(
                                "Notify",
                                &(
                                    "Agentaps",
                                    previous.unwrap_or(0),
                                    "",
                                    &notification.title,
                                    &notification.body,
                                    vec!["default", "Open session"],
                                    hints,
                                    -1_i32,
                                ),
                            )
                            .await;
                        match result {
                            Ok(id) => {
                                // Owner changes can arrive while Notify is awaiting its reply.
                                // Retire old IDs before recording the new server's notification.
                                while owners.next().now_or_never().flatten().is_some() {
                                    for (_, tag) in active.drain() {
                                        let _ = events.send(Event::Closed(tag));
                                    }
                                }
                                if let Some(previous) = previous {
                                    active.remove(&previous);
                                }
                                active.insert(id, notification.tag);
                            }
                            Err(error) => {
                                let _ = events.send(Event::Closed(notification.tag));
                                eprintln!("Could not show desktop notification: {error}");
                            }
                        }
                    }
                    Command::Dismiss(tag) => {
                        if let Some(id) = active
                            .iter()
                            .find_map(|(id, active_tag)| (active_tag == &tag).then_some(*id))
                        {
                            active.remove(&id);
                            let _: zbus::Result<()> = proxy.call("CloseNotification", &(id,)).await;
                        }
                    }
                    Command::DismissAll => {
                        for (id, _) in active.drain() {
                            let _: zbus::Result<()> = proxy.call("CloseNotification", &(id,)).await;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
