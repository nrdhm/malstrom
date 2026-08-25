//! Runtime contexts used by operators
use std::rc::Rc;

use indexmap::{IndexMap, IndexSet};
use itertools::Itertools;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::runtime::OperatorOperatorComm;
use crate::snapshot::{PersistenceClient, deserialize_state};
use crate::types::{OperatorId, WorkerId};

/// This is a type injected to logic function at runtime
/// and cotains context, whicht the logic generally can not change
/// but utilize
///
/// # Example
/// ```
/// use malstrom_core::stream::OperatorContext;
///
/// let ctx = OperatorContext::new(0, 7);
/// assert_eq!(ctx.worker_id, 0);
/// assert_eq!(ctx.operator_id, 7);
/// ```
pub struct OperatorContext {
    /// ID of this worker
    pub worker_id: WorkerId,
    /// ID of this operator
    pub operator_id: OperatorId,
}

impl OperatorContext {
    /// Create a context for the given worker/operator. External callers (e.g. the
    /// operator testkit) construct this to drive an operator's logic by hand.
    pub fn new(worker_id: WorkerId, operator_id: OperatorId) -> Self {
        Self {
            worker_id,
            operator_id,
        }
    }
}
