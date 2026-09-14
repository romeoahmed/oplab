use super::*;
use oplab_core::{address::Address, protocol::scalar::HexAddress, target::Target};
use proptest::prelude::*;
use std::collections::BTreeMap;

fn identity(document: &str) -> BuildIdentity {
    BuildIdentity {
        document: document.into(),
        revision: Counter::new(0),
        target: Target::X86_64,
        base: HexAddress::new(Address::new(0x1000)),
        assembler: assembly::identity(),
    }
}

proptest! {
    #[test]
    fn pending_edits_follow_latest_valid_document_and_cancellation(
        actions in prop::collection::vec((0..MAX_PENDING + 2, any::<bool>(), any::<bool>()), 0..80),
    ) {
        let mut queue = Queue::default();
        prop_assert_eq!(queue.push(Counter::new(0), identity("active"), "nop".into()), Ok(None));
        let active = queue.next().ok_or_else(|| TestCaseError::fail("active job missing"))?;
        let mut expected = BTreeMap::new();
        for (index, (document, valid, cancel)) in actions.into_iter().enumerate() {
            let document = format!("doc{document}");
            if cancel {
                if let Some(id) = expected.remove(&document) {
                    prop_assert!(queue.cancel(id));
                    prop_assert!(!queue.cancel(id));
                }
            } else {
                let id = Counter::new(u64::try_from(index)? + 1);
                let previous = expected.get(&document).copied();
                let result = queue.push(id, identity(&document), if valid { "nop" } else { "" }.into());
                if !valid {
                    prop_assert_eq!(result, Err(Diagnostic::new(DiagnosticCode::InvalidInput)));
                } else if previous.is_none() && expected.len() == MAX_PENDING {
                    prop_assert_eq!(result, Err(Diagnostic::new(DiagnosticCode::ResourceLimit)));
                } else {
                    prop_assert_eq!(result, Ok(previous));
                    expected.insert(document, id);
                }
            }
            prop_assert!(queue.next().is_none(), "pending edits displaced active work");
        }
        prop_assert!(queue.finish(active.id)?);
        let mut delivered = BTreeMap::new();
        while let Some(job) = queue.next() {
            prop_assert!(queue.finish(job.id)?);
            prop_assert!(delivered.insert(job.identity.document, job.id).is_none());
        }
        prop_assert_eq!(delivered, expected);
        prop_assert!(queue.idle());
    }
}

#[test]
fn cancelled_native_work_never_publishes_a_late_result() -> Result<(), WorkerError> {
    let mut queue = Queue::default();
    queue
        .push(Counter::new(1), identity("active"), "nop".into())
        .map_err(|_| WorkerError::BuildLost)?;
    let job = queue.next().ok_or(WorkerError::BuildLost)?;
    assert!(queue.cancel(job.id));
    assert!(!queue.cancel(job.id));
    let result = run(job);
    assert!(
        matches!(result.response.result, Reply::Error(ref error) if error.code == DiagnosticCode::Cancelled)
    );
    assert!(!queue.finish(result.response.id)?);
    assert!(queue.idle());
    Ok(())
}
