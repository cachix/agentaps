use super::*;
use std::{
    collections::HashSet,
    io::{BufRead, BufReader},
    process::{Child, Command as ProcessCommand, Stdio},
    sync::{Arc, Mutex},
};

struct Bus(Child);

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Shown {
    id: u32,
    replaces: u32,
    sender: String,
}

struct FakeNotifications {
    next_id: u32,
    shown: mpsc::Sender<Shown>,
    closed: mpsc::Sender<u32>,
    live: Arc<Mutex<HashSet<u32>>>,
}

#[zbus::interface(name = "org.freedesktop.Notifications", crate = "ashpd::zbus")]
impl FakeNotifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &mut self,
        app_name: &str,
        replaces_id: u32,
        _app_icon: &str,
        _summary: &str,
        _body: &str,
        actions: Vec<String>,
        _hints: HashMap<String, zbus::zvariant::OwnedValue>,
        _expire_timeout: i32,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> u32 {
        assert_eq!(app_name, "Agentaps");
        assert_eq!(actions, ["default", "Open session"]);
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            self.next_id += 1;
            self.next_id
        };
        self.live.lock().unwrap().insert(id);
        self.shown
            .send(Shown {
                id,
                replaces: replaces_id,
                sender: header.sender().unwrap().to_string(),
            })
            .unwrap();
        id
    }

    fn close_notification(&self, id: u32) {
        self.live.lock().unwrap().remove(&id);
        self.closed.send(id).unwrap();
    }
}

fn show(service: &Service, shown: &mpsc::Receiver<Shown>, tag: &str) -> Shown {
    service
        .show(Notification {
            tag: tag.into(),
            title: "Finished".into(),
            body: "Done".into(),
        })
        .unwrap();
    shown.recv_timeout(Duration::from_secs(5)).unwrap()
}

fn event(service: &Service) -> Event {
    service
        .worker
        .borrow()
        .as_ref()
        .unwrap()
        .events
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
}

#[test]
fn reuses_connection_replaces_notifications_and_releases_resources() {
    // This private bus never displays a notification or touches the desktop session.
    // Supply our own configuration so sandboxed builds need no host session.conf.
    let config = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        config.path(),
        r#"<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <policy context="default">
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
  </policy>
</busconfig>"#,
    )
    .unwrap();
    let mut bus = Bus(ProcessCommand::new("dbus-daemon")
        .arg("--config-file")
        .arg(config.path())
        .args(["--nofork", "--print-address=1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap());
    let mut address = String::new();
    BufReader::new(bus.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let address = address.trim().to_owned();
    let (shown_tx, shown) = mpsc::channel();
    let (closed_tx, closed) = mpsc::channel();
    let live = Arc::new(Mutex::new(HashSet::new()));
    let server = smol::block_on(async {
        zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .name(DESTINATION)
            .unwrap()
            .serve_at(
                PATH,
                FakeNotifications {
                    next_id: 0,
                    shown: shown_tx,
                    closed: closed_tx,
                    live: live.clone(),
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap()
    });
    let service = Service {
        address: Some(address),
        ..Default::default()
    };
    let first = show(&service, &shown, "one");
    assert_eq!(first.replaces, 0);
    for _ in 0..200 {
        let replacement = show(&service, &shown, "one");
        assert_eq!(replacement.replaces, first.id);
        assert_eq!(
            replacement.sender, first.sender,
            "notifications must reuse one D-Bus connection"
        );
    }
    assert_eq!(live.lock().unwrap().len(), 1);

    smol::block_on(server.emit_signal(
        None::<&str>,
        PATH,
        INTERFACE,
        "ActionInvoked",
        &(first.id, "unknown"),
    ))
    .unwrap();
    smol::block_on(server.emit_signal(
        None::<&str>,
        PATH,
        INTERFACE,
        "ActionInvoked",
        &(first.id, "default"),
    ))
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let actions = service.take_actions();
        if !actions.is_empty() {
            assert_eq!(actions, ["one"]);
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "notification click was lost"
        );
        thread::sleep(Duration::from_millis(1));
    }
    let after_click = show(&service, &shown, "one");
    assert_eq!(
        after_click.replaces, 0,
        "activation must release notification tracking"
    );

    smol::block_on(server.emit_signal(
        None::<&str>,
        PATH,
        INTERFACE,
        "NotificationClosed",
        &(after_click.id, 2_u32),
    ))
    .unwrap();
    assert!(matches!(event(&service), Event::Closed(tag) if tag == "one"));
    let after_close = show(&service, &shown, "one");
    assert_eq!(
        after_close.replaces, 0,
        "closing must release notification tracking"
    );
    service.dismiss("one");
    assert_eq!(
        closed.recv_timeout(Duration::from_secs(5)).unwrap(),
        after_close.id
    );

    let one = show(&service, &shown, "one");
    let two = show(&service, &shown, "two");
    service.dismiss_all();
    let dismissed: HashSet<_> = (0..2)
        .map(|_| closed.recv_timeout(Duration::from_secs(5)).unwrap())
        .collect();
    assert_eq!(dismissed, HashSet::from([one.id, two.id]));

    let before_restart = show(&service, &shown, "one");
    smol::block_on(server.release_name(DESTINATION)).unwrap();
    assert!(matches!(event(&service), Event::Closed(tag) if tag == "one"));
    smol::block_on(server.request_name(DESTINATION)).unwrap();
    // The worker keeps its connection across notification daemon restarts.
    let after_restart = show(&service, &shown, "two");
    assert_eq!(after_restart.replaces, 0);
    assert_eq!(after_restart.sender, before_restart.sender);
    assert_eq!(show(&service, &shown, "two").replaces, after_restart.id);

    // Dropping the service must wake its shared worker even with live notifications.
    drop(service);
    smol::block_on(async {
        let dbus = zbus::fdo::DBusProxy::new(&server).await.unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while dbus
            .name_has_owner(first.sender.as_str().try_into().unwrap())
            .await
            .unwrap()
        {
            assert!(
                std::time::Instant::now() < deadline,
                "notification worker retained its connection after shutdown"
            );
            smol::Timer::after(Duration::from_millis(10)).await;
        }
    });
}
