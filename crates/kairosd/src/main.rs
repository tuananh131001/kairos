use std::os::fd::AsRawFd;
use std::time::Duration;

use kairosd::{bind_socket, log, run, Daemon, OpenAppLauncher, Paths, SystemClock, APP_BUNDLE_ID};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc;

fn redirect_logs() {
    if unsafe { libc::isatty(2) } == 1 {
        return;
    }
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let dir = std::path::PathBuf::from(home).join("Library/Logs/Kairos");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("kairosd.log"))
    {
        unsafe { libc::dup2(file.as_raw_fd(), 2) };
    }
}

fn parse_paths() -> Paths {
    let mut paths = Paths::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--socket" => {
                if let Some(v) = args.next() {
                    paths.socket = v.into();
                }
            }
            "--db" => {
                if let Some(v) = args.next() {
                    paths.db = v.into();
                }
            }
            "--version" => {
                println!("kairosd {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(64);
            }
        }
    }
    paths
}

fn main() {
    redirect_logs();
    let paths = parse_paths();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let code = runtime.block_on(async_main(paths));
    std::process::exit(code);
}

async fn async_main(paths: Paths) -> i32 {
    log!(
        "starting (db={}, socket={})",
        paths.db.display(),
        paths.socket.display()
    );
    let listener = match bind_socket(&paths.socket) {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
            log!("{err}; exiting");
            return 0;
        }
        Err(err) => {
            log!("cannot bind socket: {err}");
            return 1;
        }
    };
    let store = match kairos_store::Store::open(&paths.db) {
        Ok(store) => store,
        Err(err) => {
            log!("cannot open database: {err}");
            return 1;
        }
    };
    let probes = kairosd::mac::MacProbes::new(vec![APP_BUNDLE_ID.to_string()]);
    let launcher = OpenAppLauncher {
        bundle_id: APP_BUNDLE_ID.to_string(),
    };
    let daemon = match Daemon::new(store, probes, SystemClock, launcher) {
        Ok(daemon) => daemon,
        Err(err) => {
            log!("cannot start: {err}");
            return 1;
        }
    };
    let (tick_tx, tick_rx) = mpsc::channel(4);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if tick_tx.send(None).await.is_err() {
                break;
            }
        }
    });
    let shutdown = async {
        let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
        let mut int = signal(SignalKind::interrupt()).expect("SIGINT handler");
        tokio::select! {
            _ = term.recv() => {}
            _ = int.recv() => {}
        }
    };
    run(daemon, listener, tick_rx, shutdown).await;
    let _ = std::fs::remove_file(&paths.socket);
    0
}
