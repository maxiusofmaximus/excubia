//! The switch between Alfred's engines, behind `/v1/alfred/backend`: Claude, the local model, or a
//! CLI engine the host lists as `Ready`. The phone asks; the change runs here in the background,
//! because a cold start of the local model takes minutes. What is true is on disk: the
//! `alfred-local`, `alfred-engine` and `local-mode` markers and Strata's own status. This process
//! keeps only the change it runs and the last one that failed. The contract is
//! `docs/contracts/alfred-backend.md`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use async_trait::async_trait;
use excubia_core_api::{AlfredBackend, BackendStatus, EngineListing, Requested};
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

/// A change the switch runs. `Engine` names a CLI engine that is listed.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Local,
    Claude,
    Engine(String),
}

/// Which engine answers Alfred's turns, as the markers say it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Answering {
    Claude,
    Local,
    Engine(String),
}

/// What this process knows beyond the markers: the change it runs and the last one that failed.
#[derive(Clone, Default)]
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
    /// The CLI engines that are `Ready` on this PC, fixed at startup. Alfred may choose them.
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
            job.running = Some(target.clone());
            job.failed = None;
            job.since = now();
        }
        let engine = Arc::clone(&self.engine);
        let data_dir = self.data_dir.clone();
        let notices = Arc::clone(&self.notices);
        let job = Arc::clone(&self.job);
        tokio::task::spawn_blocking(move || {
            let outcome = match &target {
                // The marker is set only once the model answers, so no turn sees a half-started one.
                Target::Local => engine
                    .start(&data_dir)
                    .and_then(|()| alfred_backend::clear_engine(&data_dir).map_err(Into::into))
                    .and_then(|()| alfred_backend::set(&data_dir).map_err(Into::into)),
                // The marker goes first: a turn that starts now answers with Claude.
                Target::Claude => alfred_backend::clear(&data_dir)
                    .map_err(Into::into)
                    .and_then(|()| alfred_backend::clear_engine(&data_dir).map_err(Into::into))
                    .and_then(|()| engine.stop(&data_dir)),
                // The engine answers at once, so its marker goes first; Strata stops after it.
                Target::Engine(name) => alfred_backend::clear(&data_dir)
                    .map_err(Into::into)
                    .and_then(|()| alfred_backend::set_engine(&data_dir, name).map_err(Into::into))
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
                let message = match &target {
                    Target::Local => format!("No pude arrancar el modelo local: {text}"),
                    Target::Claude => format!("No pude parar el modelo local: {text}"),
                    Target::Engine(name) => format!("No pude cambiar a {name}: {text}"),
                };
                if let Err(error) = notices.notify(&message) {
                    eprintln!("host: could not write the alfred-backend notice: {error}");
                }
            }
        });
    }
}

/// The state from the markers and from Strata. The job adds only what is running and what failed.
fn describe(
    job: &Job,
    model: State,
    answering: &Answering,
    engines: &[EngineListing],
    voice_on: bool,
) -> BackendStatus {
    let (state, detail) = match (&job.running, job.failed.as_deref()) {
        (Some(Target::Claude), _) => ("stopping", None),
        (Some(_), _) => ("starting", None),
        (None, Some(text)) => ("failed", Some(text.to_owned())),
        (None, None) => at_rest(answering, engines, model),
    };
    let backend = match answering {
        Answering::Claude => "claude".to_owned(),
        Answering::Local => "local".to_owned(),
        Answering::Engine(name) => name.clone(),
    };
    BackendStatus {
        backend,
        state,
        detail,
        since: job.since,
        // A local start takes the voice down first, so it counts from the moment it runs.
        voice_suspended: voice_on || job.running == Some(Target::Local),
        engines: engines.to_vec(),
    }
}

/// The state when no change runs and none failed.
fn at_rest(
    answering: &Answering,
    engines: &[EngineListing],
    model: State,
) -> (&'static str, Option<String>) {
    match (answering, model) {
        (Answering::Engine(name), _) if engines.iter().any(|engine| engine.name == *name) => {
            ("engine", None)
        }
        // A marker naming an engine that is no longer listed: the turns fail closed, and so does the status.
        (Answering::Engine(name), _) => {
            ("failed", Some(format!("{name} no está listo en este PC")))
        }
        (Answering::Local, State::Ready) => ("local", None),
        (Answering::Local | Answering::Claude, State::Loading) => ("starting", None),
        (Answering::Local, State::Stopped) => (
            "failed",
            Some("Alfred usa el modelo local, pero Strata no está en marcha".to_owned()),
        ),
        (Answering::Claude, State::Ready) => (
            "claude",
            Some("el modelo local está en marcha y Alfred no lo usa".to_owned()),
        ),
        (Answering::Claude, State::Stopped) => ("claude", None),
    }
}

#[async_trait]
impl AlfredBackend for Switch {
    async fn status(&self) -> BackendStatus {
        // The job is read before the markers: a change writes its markers before it clears the job,
        // so this order never pairs an answer the markers have already left with a finished job.
        let job = self.lock_job().clone();
        let engine = Arc::clone(&self.engine);
        let data_dir = self.data_dir.clone();
        // Strata answers over the loopback and may read the process table: off the runtime.
        let (model, answering, voice_on) = tokio::task::spawn_blocking(move || {
            let answering = match alfred_backend::engine(&data_dir) {
                Some(name) => Answering::Engine(name),
                None if alfred_backend::is_on(&data_dir) => Answering::Local,
                None => Answering::Claude,
            };
            (
                engine.state().unwrap_or(State::Stopped),
                answering,
                local_mode::is_on(&data_dir),
            )
        })
        .await
        .unwrap_or((State::Stopped, Answering::Claude, false));
        describe(&job, model, &answering, &self.engines, voice_on)
    }

    async fn request(&self, backend: &str) -> Result<Requested, String> {
        let target = match backend {
            "local" => Target::Local,
            "claude" => Target::Claude,
            name if self.engines.iter().any(|engine| engine.name == name) => {
                Target::Engine(name.to_owned())
            }
            other => return Err(format!("no sé cambiar a {other}")),
        };
        let _gate = self.gate.lock().await;
        if let Some(current) = self.lock_job().running.clone() {
            return if current == target {
                Ok(Requested::Started)
            } else {
                Err("Alfred está cambiando de modelo; espera a que termine".to_owned())
            };
        }
        let now = self.status().await;
        let done = match &target {
            Target::Local => now.state == "local",
            // With voice still down the local model may run without Alfred: Claude is not done yet.
            Target::Claude => now.state == "claude" && !now.voice_suspended,
            Target::Engine(name) => now.state == "engine" && now.backend == *name,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(name: &str) -> Vec<EngineListing> {
        vec![EngineListing {
            name: name.to_owned(),
            version: "2.2.6".to_owned(),
        }]
    }

    fn at_rest_status(
        answering: &Answering,
        engines: &[EngineListing],
        model: State,
    ) -> BackendStatus {
        describe(&Job::default(), model, answering, engines, false)
    }

    #[test]
    fn claude_answers_when_no_marker_is_set() {
        let status = at_rest_status(&Answering::Claude, &[], State::Stopped);
        assert_eq!(
            (status.backend.as_str(), status.state),
            ("claude", "claude")
        );
        assert_eq!(status.detail, None);
    }

    #[test]
    fn a_listed_engine_with_its_marker_is_the_engine_state() {
        let status = at_rest_status(
            &Answering::Engine("opencode".to_owned()),
            &listed("opencode"),
            State::Stopped,
        );
        assert_eq!(
            (status.backend.as_str(), status.state),
            ("opencode", "engine")
        );
        assert_eq!(status.detail, None);
        assert_eq!(status.engines, listed("opencode"));
    }

    #[test]
    fn a_marker_for_an_unlisted_engine_fails_visibly() {
        let status = at_rest_status(
            &Answering::Engine("opencode".to_owned()),
            &[],
            State::Stopped,
        );
        assert_eq!(
            (status.backend.as_str(), status.state),
            ("opencode", "failed")
        );
        assert_eq!(
            status.detail.as_deref(),
            Some("opencode no está listo en este PC")
        );
    }

    #[test]
    fn a_running_engine_change_reads_as_starting() {
        let job = Job {
            running: Some(Target::Engine("opencode".to_owned())),
            ..Job::default()
        };
        let status = describe(
            &job,
            State::Ready,
            &Answering::Claude,
            &listed("opencode"),
            false,
        );
        assert_eq!(status.state, "starting");
    }

    #[test]
    fn a_failed_change_is_kept_until_the_next_request() {
        let job = Job {
            failed: Some("No pude cambiar a opencode: boom".to_owned()),
            ..Job::default()
        };
        let status = describe(&job, State::Stopped, &Answering::Claude, &[], false);
        assert_eq!(status.state, "failed");
        assert_eq!(
            status.detail.as_deref(),
            Some("No pude cambiar a opencode: boom")
        );
    }

    #[test]
    fn local_stopped_with_its_marker_is_failed() {
        let status = at_rest_status(&Answering::Local, &[], State::Stopped);
        assert_eq!((status.backend.as_str(), status.state), ("local", "failed"));
    }
}
