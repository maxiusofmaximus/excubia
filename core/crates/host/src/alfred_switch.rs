//! The switch between Alfred's two engines, behind `/v1/alfred/backend`. The phone asks; the change
//! runs here in the background, because a cold start of the local model takes minutes. What is
//! true is on disk: the `alfred-local` and `local-mode` markers and Strata's own status. This
//! process keeps only the change it runs and the last one that failed. The contract is
//! `docs/contracts/alfred-backend.md`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use async_trait::async_trait;
use hn_core_api::{AlfredBackend, BackendStatus, EngineListing, Requested};
use tokio::sync::Mutex as AsyncMutex;

use crate::pairing_notice::PairingNotice;
use crate::strata::{self, State};
use crate::{alfred_backend, local_mode};

/// What the switch needs from the local model. Production is Strata; a test puts a double here.
pub trait Engine: Send + Sync + 'static {
    /// Where the model stands. `Err` when there is no Strata to ask.
    fn state(&self) -> Result<State>;
    /// Launches the model and returns once it answers. Blocks for minutes; a failure leaves no marker.
    fn start(&self, data_dir: &Path) -> Result<()>;
    /// Stops the model and removes its markers.
    fn stop(&self, data_dir: &Path) -> Result<()>;
}

/// Strata, through the functions `host strata` and `host alfred local` run.
pub struct Strata;

impl Engine for Strata {
    fn state(&self) -> Result<State> {
        strata::state(&strata::binary()?)
    }

    fn start(&self, data_dir: &Path) -> Result<()> {
        let bin = strata::binary()?;
        // The launcher's own errors are in English; a missing binary is said here in Spanish.
        if !bin.is_file() {
            bail!(
                "no encuentro Strata en {}; define STRATA_BIN",
                bin.display()
            );
        }
        strata::start(&bin, data_dir)
    }

    fn stop(&self, data_dir: &Path) -> Result<()> {
        strata::stop(&strata::binary()?, data_dir)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Local,
    Claude,
}

/// What this process knows beyond the markers: the change it runs and the last one that failed.
#[derive(Default)]
struct Job {
    running: Option<Target>,
    /// Cleared by the next request, so a retry starts from a clean state.
    failed: Option<String>,
    /// Unix seconds of the last change this process started or finished; its start until then.
    since: u64,
}

pub struct Switch {
    data_dir: PathBuf,
    notices: Arc<PairingNotice>,
    engine: Arc<dyn Engine>,
    /// Held while a request decides and starts its change, so two requests never start two.
    gate: AsyncMutex<()>,
    job: Arc<Mutex<Job>>,
    /// The CLI engines that are `Ready` on this PC, fixed at startup. Listed, never selectable yet.
    engines: Vec<EngineListing>,
}

impl Switch {
    pub fn new(data_dir: PathBuf, notices: Arc<PairingNotice>, engine: Arc<dyn Engine>) -> Self {
        Self {
            data_dir,
            notices,
            engine,
            gate: AsyncMutex::new(()),
            job: Arc::new(Mutex::new(Job {
                since: now(),
                ..Job::default()
            })),
            engines: Vec::new(),
        }
    }

    pub fn with_engines(mut self, engines: Vec<EngineListing>) -> Self {
        self.engines = engines;
        self
    }

    fn lock_job(&self) -> MutexGuard<'_, Job> {
        self.job.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs the change on a blocking thread: a cold start takes minutes and holds no runtime worker.
    fn spawn(&self, target: Target) {
        {
            let mut job = self.lock_job();
            job.running = Some(target);
            job.failed = None;
            job.since = now();
        }
        let engine = Arc::clone(&self.engine);
        let data_dir = self.data_dir.clone();
        let notices = Arc::clone(&self.notices);
        let job = Arc::clone(&self.job);
        tokio::task::spawn_blocking(move || {
            let outcome = match target {
                // The marker is set only once the model answers, so no turn sees a half-started one.
                Target::Local => engine
                    .start(&data_dir)
                    .and_then(|()| alfred_backend::set(&data_dir).map_err(Into::into)),
                // The marker goes first: a turn that starts now answers with Claude.
                Target::Claude => alfred_backend::clear(&data_dir)
                    .map_err(Into::into)
                    .and_then(|()| engine.stop(&data_dir)),
            };
            let failure = outcome.err().map(|error| format!("{error:#}"));
            {
                let mut job = job.lock().unwrap_or_else(PoisonError::into_inner);
                job.running = None;
                job.since = now();
                job.failed.clone_from(&failure);
            }
            if let Some(text) = failure {
                let message = match target {
                    Target::Local => format!("No pude arrancar el modelo local: {text}"),
                    Target::Claude => format!("No pude parar el modelo local: {text}"),
                };
                if let Err(error) = notices.notify(&message) {
                    eprintln!("host: could not write the alfred-backend notice: {error}");
                }
            }
        });
    }
}

/// The state from the markers and from Strata. The job adds only what is running and what failed.
fn describe(job: &Job, model: State, alfred_on: bool, voice_on: bool) -> BackendStatus {
    let backend = if alfred_on { "local" } else { "claude" };
    let (state, detail) = match (job.running, job.failed.as_deref(), model) {
        (Some(Target::Local), _, _) => ("starting", None),
        (Some(Target::Claude), _, _) => ("stopping", None),
        (None, Some(text), _) => ("failed", Some(text.to_owned())),
        (None, None, State::Ready) if alfred_on => ("local", None),
        (None, None, State::Loading) => ("starting", None),
        (None, None, State::Stopped) if alfred_on => (
            "failed",
            Some("Alfred usa el modelo local, pero Strata no está en marcha".to_owned()),
        ),
        (None, None, State::Ready) => (
            "claude",
            Some("el modelo local está en marcha y Alfred no lo usa".to_owned()),
        ),
        (None, None, _) => ("claude", None),
    };
    BackendStatus {
        backend,
        state,
        detail,
        since: job.since,
        // A local start takes the voice down first, so it counts from the moment it runs.
        voice_suspended: voice_on || job.running == Some(Target::Local),
        engines: Vec::new(),
    }
}

#[async_trait]
impl AlfredBackend for Switch {
    async fn status(&self) -> BackendStatus {
        let engine = Arc::clone(&self.engine);
        let data_dir = self.data_dir.clone();
        // Strata answers over the loopback and may read the process table: off the runtime.
        let (model, alfred_on, voice_on) = tokio::task::spawn_blocking(move || {
            (
                engine.state().unwrap_or(State::Stopped),
                alfred_backend::is_on(&data_dir),
                local_mode::is_on(&data_dir),
            )
        })
        .await
        .unwrap_or((State::Stopped, false, false));
        let mut status = describe(&self.lock_job(), model, alfred_on, voice_on);
        status.engines.clone_from(&self.engines);
        status
    }

    async fn request(&self, backend: &str) -> Result<Requested, String> {
        let target = match backend {
            "local" => Target::Local,
            "claude" => Target::Claude,
            other => return Err(format!("no sé cambiar a {other}")),
        };
        let _gate = self.gate.lock().await;
        if let Some(current) = self.lock_job().running {
            return if current == target {
                Ok(Requested::Started)
            } else {
                Err("Alfred está cambiando de modelo; espera a que termine".to_owned())
            };
        }
        let now = self.status().await;
        let done = match target {
            Target::Local => now.state == "local",
            // With voice still down the local model may run without Alfred: Claude is not done yet.
            Target::Claude => now.state == "claude" && !now.voice_suspended,
        };
        if done {
            return Ok(Requested::Unchanged);
        }
        self.spawn(target);
        Ok(Requested::Started)
    }
}

/// Seconds since the Unix epoch.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}
