use crate::CoreError;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

// Runtime publication identities are deliberately outside canonical replay. A checked
// process-wide sequence prevents repeat jobs from reusing the old Batch hash identity;
// exclusion also handles documents loaded with an identity in this namespace.
pub(super) fn allocate_script_document_identity(
    excluded: impl IntoIterator<Item = u128>,
) -> Result<u128, CoreError> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let excluded = excluded.into_iter().collect::<BTreeSet<_>>();
    loop {
        let sequence = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| CoreError::InvalidState("script document identity space exhausted"))?;
        let candidate = (u128::from(0x494e_4b53_4352_4950_u64) << 64) | u128::from(sequence);
        if !excluded.contains(&candidate) {
            return Ok(candidate);
        }
    }
}
