//! The worker child: one process, requests in order, a timeout that kills it. A dead worker stays
//! dead; the caller restarts by calling `AsrEngine::spawn` again.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use crate::corrector::Corrector;
use crate::error::AsrError;
use crate::lexicon;
use crate::limits::{
    DEFAULT_REQUEST_TIMEOUT, DEFAULT_STARTUP_TIMEOUT, MAX_REPLY_LINE_BYTES, MAX_REQUEST_SAMPLES,
};
use crate::vad;
use crate::wire::{self, Reply};

const CUDA_PROVIDER: &str = "CUDAExecutionProvider";

#[derive(Debug, Clone)]
pub struct AsrConfig {
    pub python: PathBuf,
    pub script: PathBuf,
    pub model: PathBuf,
    pub request_timeout: Duration,
    pub startup_timeout: Duration,
    pub corrector: Corrector,
}

impl AsrConfig {
    pub fn new(
        python: impl Into<PathBuf>,
        script: impl Into<PathBuf>,
        model: impl Into<PathBuf>,
    ) -> Self {
        Self {
            python: python.into(),
            script: script.into(),
            model: model.into(),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            startup_timeout: DEFAULT_STARTUP_TIMEOUT,
            corrector: Corrector::default(),
        }
    }
}

pub struct AsrEngine {
    config: AsrConfig,
    worker: Option<Worker>,
    next_id: u64,
}

impl AsrEngine {
    /// Starts the worker and waits for its ready line, which only comes once CUDA is confirmed.
    pub fn spawn(config: AsrConfig) -> Result<Self, AsrError> {
        let worker = Worker::start(&config)?;
        Ok(Self {
            config,
            worker: Some(worker),
            next_id: 0,
        })
    }

    pub fn transcribe_pcm(&mut self, pcm: &[f32]) -> Result<String, AsrError> {
        // The worker exits on an empty or oversized request, so neither is sent.
        if pcm.is_empty() {
            return Ok(String::new());
        }
        if pcm.len() > MAX_REQUEST_SAMPLES {
            return Err(AsrError::AudioTooLong);
        }
        let id = self.next_id;
        self.next_id += 1;
        let worker = self.worker.as_ref().ok_or(AsrError::WorkerExited)?;
        let answer = worker.ask(wire::encode_request(id, pcm), self.config.request_timeout);
        match answer.and_then(|reply| wire::text_of(reply, id)) {
            // The worker answered in protocol, so it is still healthy.
            Err(error @ AsrError::Engine(_)) => Err(error),
            Err(error) => {
                self.worker = None;
                Err(error)
            }
            Ok(text) => Ok(text),
        }
    }

    /// Speech chunks go to the worker one by one; silence never reaches it.
    pub fn transcribe_audio(&mut self, pcm: &[f32]) -> Result<String, AsrError> {
        let mut texts = Vec::new();
        for range in vad::segment(pcm).chunks {
            let text = self.transcribe_pcm(&pcm[range])?;
            if !text.is_empty() {
                texts.push(text);
            }
        }
        let text = lexicon::fix_terms(&texts.join(" "));
        Ok(self.config.corrector.apply(&text))
    }
}

/// Owns the child. Dropping it kills the child; closing its stdin first would let a clean exit
/// happen, but a hung model must not block shutdown.
struct Worker {
    child: Child,
    requests: Sender<Vec<u8>>,
    replies: Receiver<Result<Reply, AsrError>>,
}

impl Worker {
    fn start(config: &AsrConfig) -> Result<Self, AsrError> {
        let mut child = Command::new(&config.python)
            .arg(&config.script)
            .arg(&config.model)
            .env("HF_HUB_OFFLINE", "1")
            .env_remove("HF_TOKEN")
            .env_remove("HUGGING_FACE_HUB_TOKEN")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|_| AsrError::WorkerStart)?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(AsrError::WorkerStart);
        };
        let (requests, outgoing) = mpsc::channel();
        let (sender, replies) = mpsc::channel();
        thread::spawn(move || write_requests(stdin, outgoing));
        thread::spawn(move || read_replies(stdout, sender));
        let worker = Self {
            child,
            requests,
            replies,
        };
        match worker.replies.recv_timeout(config.startup_timeout) {
            Ok(Ok(Reply::Ready { provider })) if provider == CUDA_PROVIDER => Ok(worker),
            Ok(Ok(Reply::Ready { .. })) => Err(AsrError::NoCuda),
            Ok(Ok(Reply::Error { error })) => Err(wire::engine_error(error)),
            Ok(Ok(Reply::Text { .. }) | Err(_)) => Err(AsrError::WorkerStart),
            Err(RecvTimeoutError::Timeout) => Err(AsrError::Timeout),
            Err(RecvTimeoutError::Disconnected) => Err(AsrError::WorkerStart),
        }
    }

    fn ask(&self, frame: Vec<u8>, timeout: Duration) -> Result<Reply, AsrError> {
        self.requests
            .send(frame)
            .map_err(|_| AsrError::WorkerExited)?;
        self.replies
            .recv_timeout(timeout)
            .map_err(|error| match error {
                RecvTimeoutError::Timeout => AsrError::Timeout,
                RecvTimeoutError::Disconnected => AsrError::WorkerExited,
            })?
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Writes happen off the caller's thread so a worker that stops reading cannot block the timeout.
fn write_requests(mut stdin: ChildStdin, requests: Receiver<Vec<u8>>) {
    for frame in requests {
        if stdin
            .write_all(&frame)
            .and_then(|()| stdin.flush())
            .is_err()
        {
            return;
        }
    }
}

fn read_replies(stdout: ChildStdout, sender: Sender<Result<Reply, AsrError>>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let reply = read_reply(&mut reader);
        let failed = reply.is_err();
        if sender.send(reply).is_err() || failed {
            return;
        }
    }
}

fn read_reply(reader: &mut BufReader<ChildStdout>) -> Result<Reply, AsrError> {
    let mut line = Vec::new();
    let read = reader
        .by_ref()
        .take(MAX_REPLY_LINE_BYTES)
        .read_until(b'\n', &mut line)
        .map_err(|_| AsrError::WorkerExited)?;
    if read == 0 {
        return Err(AsrError::WorkerExited);
    }
    serde_json::from_slice(&line).map_err(|_| AsrError::InvalidReply)
}
