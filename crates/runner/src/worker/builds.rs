//! Bounded pending builds and explicit result cancellation, independent of native work.

use super::{WorkerError, transport::Message};
use oplab_core::protocol::{
    Artifact, BuildIdentity, Diagnostic, DiagnosticCode, Reply, Response, scalar::Counter,
};
use oplab_toolchain::assembly;
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const MAX_PENDING: usize = 8;

pub(super) struct Job {
    pub id: Counter,
    pub identity: BuildIdentity,
    pub source: String,
    cancelled: Arc<AtomicBool>,
}

struct Active {
    id: Counter,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub(super) struct Queue {
    pending: VecDeque<Job>,
    active: Option<Active>,
}

impl Queue {
    pub(super) fn push(
        &mut self,
        id: Counter,
        identity: BuildIdentity,
        source: String,
    ) -> Result<Option<Counter>, Diagnostic> {
        assembly::validate_build(&identity, &source)?;
        let previous = self
            .pending
            .iter()
            .position(|job| job.identity.document == identity.document);
        if previous.is_none() && self.pending.len() == MAX_PENDING {
            return Err(Diagnostic::new(DiagnosticCode::ResourceLimit));
        }
        let replaced = previous
            .and_then(|index| self.pending.remove(index))
            .map(|job| job.id);
        self.pending.push_back(Job {
            id,
            identity,
            source,
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        Ok(replaced)
    }

    pub(super) fn next(&mut self) -> Option<Job> {
        if self.active.is_some() {
            return None;
        }
        let job = self.pending.pop_front()?;
        self.active = Some(Active {
            id: job.id,
            cancelled: Arc::clone(&job.cancelled),
        });
        Some(job)
    }

    pub(super) fn cancel(&mut self, id: Counter) -> bool {
        if let Some(active) = &self.active
            && active.id == id
        {
            return !active.cancelled.swap(true, Ordering::Relaxed);
        }
        self.pending
            .iter()
            .position(|job| job.id == id)
            .and_then(|index| self.pending.remove(index))
            .is_some()
    }

    pub(super) fn finish(&mut self, id: Counter) -> Result<bool, WorkerError> {
        let active = self
            .active
            .take()
            .filter(|active| active.id == id)
            .ok_or(WorkerError::BuildLost)?;
        Ok(!active.cancelled.load(Ordering::Relaxed))
    }

    pub(super) fn idle(&self) -> bool {
        self.active.is_none() && self.pending.is_empty()
    }

    pub(super) const fn busy(&self) -> bool {
        self.active.is_some()
    }
}

/// Assemble on the build thread, checking cancellation between native stages.
///
/// The cancellation flag publishes no other state; relaxed loads suffice.
pub(super) fn run(job: Job) -> Message {
    let result = assembly::assemble_cancellable(job.identity, &job.source, || {
        job.cancelled.load(Ordering::Relaxed)
    });
    build_message(job.id, result)
}

pub(super) fn build_message(
    id: Counter,
    result: Result<assembly::BuildArtifact, Diagnostic>,
) -> Message {
    match result {
        Ok(build) => {
            let image = match assembly::describe(&build.image, build.identity.target) {
                Ok(image) => image,
                Err(diagnostic) => return error(id, diagnostic),
            };
            let (Ok(object_bytes), Ok(image_bytes)) = (
                u32::try_from(build.object.len()),
                u32::try_from(build.image.len()),
            ) else {
                return error(id, Diagnostic::new(DiagnosticCode::ResourceLimit));
            };
            Message {
                response: Response {
                    id,
                    result: Reply::Assembled(Artifact {
                        identity: build.identity,
                        object_bytes,
                        image_bytes,
                        image,
                    }),
                },
                payloads: vec![build.object, build.image],
            }
        }
        Err(diagnostic) => error(id, diagnostic),
    }
}

pub(super) const fn error(id: Counter, diagnostic: Diagnostic) -> Message {
    Message {
        response: Response {
            id,
            result: Reply::Error(diagnostic),
        },
        payloads: Vec::new(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/builds.rs"]
mod tests;
