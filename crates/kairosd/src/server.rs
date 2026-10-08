use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use kairos_ipc::{decode_client, encode, ErrorBody, Event, Request, ServerMessage};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::daemon::{Daemon, Outcome};
use crate::{log, AppLauncher, Clock, Probes};

pub type TickAck = Option<oneshot::Sender<()>>;

pub struct Command {
    pub request: Request,
    pub reply: oneshot::Sender<Outcome>,
}

pub fn bind_socket(path: &Path) -> std::io::Result<UnixListener> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    if path.exists() {
        if std::os::unix::net::UnixStream::connect(path).is_ok() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "another kairosd is already listening",
            ));
        }
        std::fs::remove_file(path)?;
    }
    let previous = unsafe { libc::umask(0o177) };
    let listener = UnixListener::bind(path);
    unsafe { libc::umask(previous) };
    let listener = listener?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

pub async fn run<P, C, L>(
    mut daemon: Daemon<P, C, L>,
    listener: UnixListener,
    mut ticks: mpsc::Receiver<TickAck>,
    shutdown_signal: impl std::future::Future<Output = ()>,
) where
    P: Probes,
    C: Clock,
    L: AppLauncher,
{
    let (events_tx, _) = broadcast::channel::<ServerMessage>(64);
    let (commands_tx, mut commands) = mpsc::channel::<Command>(32);
    let uid = unsafe { libc::getuid() };
    let accept_events = events_tx.clone();
    let accept = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let peer_uid = stream.peer_cred().map(|c| c.uid()).ok();
                    if peer_uid != Some(uid) {
                        log!("rejected connection from uid {peer_uid:?}");
                        drop(stream);
                        continue;
                    }
                    tokio::spawn(serve_connection(
                        stream,
                        commands_tx.clone(),
                        accept_events.clone(),
                    ));
                }
                Err(err) => {
                    log!("accept failed: {err}");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    });
    tokio::pin!(shutdown_signal);
    loop {
        tokio::select! {
            tick = ticks.recv() => {
                let Some(ack) = tick else { break };
                let subscribers = events_tx.receiver_count();
                daemon.tick(subscribers);
                publish(&mut daemon, &events_tx);
                if subscribers > 0 {
                    let state = daemon.current_state();
                    let _ = events_tx.send(ServerMessage::new(Event::State(state)));
                }
                if let Some(ack) = ack {
                    let _ = ack.send(());
                }
            }
            Some(command) = commands.recv() => {
                let outcome = daemon.handle(command.request, events_tx.receiver_count());
                let shutdown = outcome.shutdown;
                let _ = command.reply.send(outcome);
                publish(&mut daemon, &events_tx);
                if shutdown {
                    log!("shutdown requested");
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    break;
                }
            }
            () = &mut shutdown_signal => {
                log!("termination signal received");
                break;
            }
        }
    }
    accept.abort();
    daemon.shutdown();
}

fn publish<P: Probes, C: Clock, L: AppLauncher>(
    daemon: &mut Daemon<P, C, L>,
    events: &broadcast::Sender<ServerMessage>,
) {
    for event in daemon.take_outbox() {
        let _ = events.send(ServerMessage::new(event));
    }
}

async fn serve_connection(
    stream: UnixStream,
    commands: mpsc::Sender<Command>,
    events: broadcast::Sender<ServerMessage>,
) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let mut subscription: Option<broadcast::Receiver<ServerMessage>> = None;
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Ok(Some(line)) = line else { break };
                if line.trim().is_empty() {
                    continue;
                }
                let outgoing = match decode_client(&line) {
                    Ok(message) => {
                        if matches!(message.request, Request::Subscribe) && subscription.is_none() {
                            subscription = Some(events.subscribe());
                        }
                        let (tx, rx) = oneshot::channel();
                        if commands.send(Command { request: message.request, reply: tx }).await.is_err() {
                            break;
                        }
                        let Ok(outcome) = rx.await else { break };
                        let mut out = vec![with_id(outcome.reply, message.id)];
                        out.extend(outcome.followups);
                        out
                    }
                    Err(err) => vec![Event::Reply {
                        id: None,
                        ok: false,
                        error: Some(ErrorBody {
                            code: "bad_request".into(),
                            message: err.to_string(),
                            fields: Vec::new(),
                        }),
                        state: None,
                        settings: None,
                    }],
                };
                for event in outgoing {
                    if writer.write_all(encode(&ServerMessage::new(event)).as_bytes()).await.is_err() {
                        return;
                    }
                }
            }
            message = recv(&mut subscription) => {
                match message {
                    Ok(message) => {
                        if writer.write_all(encode(&message).as_bytes()).await.is_err() {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn recv(
    subscription: &mut Option<broadcast::Receiver<ServerMessage>>,
) -> Result<ServerMessage, broadcast::error::RecvError> {
    match subscription {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

fn with_id(event: Event, id: Option<u64>) -> Event {
    match event {
        Event::Reply {
            ok,
            error,
            state,
            settings,
            ..
        } => Event::Reply {
            id,
            ok,
            error,
            state,
            settings,
        },
        other => other,
    }
}
