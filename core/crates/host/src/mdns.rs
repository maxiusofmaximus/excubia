//! F2c: the host announces itself on the LAN with mDNS only while a PAKE pairing code is alive
//! (`docs/contracts/pairing.md`, *F2c*).
//!
//! The announcement is not a secret: it lets the app find the host without typing an IP. It carries the service
//! type, a random instance name of this run, the port of the LAN TLS listener and `v=1`. It never carries the code,
//! the SPKI or any device id, and the PAKE protects the pairing whether or not anyone saw the announcement.
//!
//! [`Announcer`] holds the transitions and [`MdnsBackend`] talks to `mdns-sd`; [`spawn`] polls the slot every
//! second. If mDNS cannot start, pairing by IP and PAKE keeps working: the host only logs one line.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use hn_core_api::PakeSlot;
use mdns_sd::{IfKind, ServiceDaemon, ServiceInfo};
use rand_core::{OsRng, RngCore};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// The service type the app browses for.
pub const SERVICE_TYPE: &str = "_excubia-pair._tcp.local.";

/// How often the slot is looked at.
const TICK: Duration = Duration::from_secs(1);

/// How long the daemon is given to send its goodbye and stop.
const STOP_WAIT: Duration = Duration::from_secs(2);

/// What the announcer needs from the network side, so the transitions can be tested with a fake.
pub trait AnnounceBackend: Send + 'static {
    /// Starts announcing.
    fn publish(&mut self) -> Result<(), String>;
    /// Stops announcing, with a goodbye packet.
    fn withdraw(&mut self) -> Result<(), String>;
    /// Releases the network side for good.
    fn close(&mut self);
}

/// Turns "a code is live" into announce / withdraw, once per change.
pub struct Announcer<B> {
    backend: B,
    on: bool,
    /// The last publish failed during this live period: do not retry (or log) every tick.
    failed: bool,
}

impl<B: AnnounceBackend> Announcer<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            on: false,
            failed: false,
        }
    }

    /// Announces; does nothing if it already does. A failure leaves it off.
    pub fn register(&mut self) -> Result<(), String> {
        if self.on {
            return Ok(());
        }
        self.backend.publish()?;
        self.on = true;
        Ok(())
    }

    /// Withdraws; does nothing if it does not announce. It counts as off even if the goodbye failed: the
    /// records then expire by their TTL.
    pub fn unregister(&mut self) -> Result<(), String> {
        if !self.on {
            return Ok(());
        }
        self.on = false;
        self.backend.withdraw()
    }

    /// One tick: an `Err` is returned once per failed transition, never repeated while nothing changes.
    pub fn sync(&mut self, live: bool) -> Result<(), String> {
        if !live {
            self.failed = false;
            return self.unregister();
        }
        if self.failed {
            return Ok(());
        }
        self.register().inspect_err(|_| self.failed = true)
    }

    /// Withdraws and releases the backend.
    pub fn shutdown(&mut self) -> Result<(), String> {
        let withdrawn = self.unregister();
        self.backend.close();
        withdrawn
    }
}

/// `mdns-sd` restricted to the interface of the LAN TLS listener.
pub struct MdnsBackend {
    daemon: ServiceDaemon,
    info: ServiceInfo,
}

impl MdnsBackend {
    /// `listener` is the address the LAN TLS listener is bound to: its port is announced, and its interface is
    /// the only one used. The instance and host names are random for this run, never the user's or the PC's.
    pub fn new(listener: SocketAddr) -> Result<Self, String> {
        let mut random = [0u8; 4];
        OsRng
            .try_fill_bytes(&mut random)
            .map_err(|_| "no system randomness".to_owned())?;
        let instance = format!("excubia-{}", hex::encode(random));
        let ip = listener.ip();
        let mut info = ServiceInfo::new(
            SERVICE_TYPE,
            &instance,
            &format!("{instance}.local."),
            ip,
            listener.port(),
            &[("v", "1")][..],
        )
        .map_err(|error| error.to_string())?;
        info.set_interfaces(vec![IfKind::Addr(ip)]);
        let daemon = ServiceDaemon::new().map_err(|error| error.to_string())?;
        for result in [
            daemon.disable_interface(IfKind::All),
            daemon.enable_interface(IfKind::Addr(ip)),
        ] {
            if let Err(error) = result {
                // The daemon is not needed any more; do not leave its thread running.
                let _ = daemon.shutdown();
                return Err(error.to_string());
            }
        }
        Ok(Self { daemon, info })
    }
}

impl AnnounceBackend for MdnsBackend {
    fn publish(&mut self) -> Result<(), String> {
        let info = self.info.clone();
        self.daemon
            .register(info)
            .map_err(|error| error.to_string())
    }

    fn withdraw(&mut self) -> Result<(), String> {
        // The daemon sends the goodbye on its own thread; the status receiver is not needed.
        self.daemon
            .unregister(self.info.get_fullname())
            .map(drop)
            .map_err(|error| error.to_string())
    }

    fn close(&mut self) {
        // Commands run in order, so the goodbye of `withdraw` goes out before the daemon stops.
        if let Ok(status) = self.daemon.shutdown() {
            let _ = status.recv_timeout(STOP_WAIT);
        }
    }
}

/// The polling task. Dropping it without [`AnnounceTask::stop`] stops it too, but without waiting.
pub struct AnnounceTask {
    stop: oneshot::Sender<()>,
    join: JoinHandle<()>,
}

impl AnnounceTask {
    /// Stops the polling, withdraws the announcement and shuts the daemon down.
    pub async fn stop(self) {
        let _ = self.stop.send(());
        let _ = tokio::time::timeout(STOP_WAIT * 2, self.join).await;
    }
}

/// Starts announcing `listener` while `slot` is live. `None` (after one log line) if mDNS cannot start.
pub fn start(slot: Arc<PakeSlot>, listener: SocketAddr) -> Option<AnnounceTask> {
    match MdnsBackend::new(listener) {
        Ok(backend) => Some(spawn(slot, Announcer::new(backend))),
        Err(error) => {
            println!("host: mDNS no disponible: {error}; el emparejamiento sigue por IP");
            None
        }
    }
}

pub fn spawn<B: AnnounceBackend>(slot: Arc<PakeSlot>, announcer: Announcer<B>) -> AnnounceTask {
    let (stop, stopped) = oneshot::channel();
    let join = tokio::spawn(run(slot, announcer, stopped));
    AnnounceTask { stop, join }
}

async fn run<B: AnnounceBackend>(
    slot: Arc<PakeSlot>,
    mut announcer: Announcer<B>,
    mut stopped: oneshot::Receiver<()>,
) {
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if let Err(error) = announcer.sync(slot.is_live()) {
                    println!("host: mDNS: {error}; el emparejamiento sigue por IP");
                }
            }
            _ = &mut stopped => break,
        }
    }
    // `close` waits for the daemon: keep it off the async threads.
    let closed = tokio::task::spawn_blocking(move || announcer.shutdown()).await;
    if let Ok(Err(error)) = closed {
        println!("host: mDNS: no pude retirar el anuncio: {error}");
    }
}

#[cfg(test)]
mod tests;
