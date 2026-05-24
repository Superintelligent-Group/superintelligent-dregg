/// Selector column indices.
pub const NOOP: usize = 0;
pub const TRANSFER: usize = 1;
pub const SET_FIELD: usize = 2;
pub const GRANT_CAP: usize = 3;
pub const NOTE_SPEND: usize = 4;
pub const NOTE_CREATE: usize = 5;
pub const CREATE_OBLIGATION: usize = 6;
pub const FULFILL_OBLIGATION: usize = 7;
/// Custom cell program dispatch: state flows normally, but domain-specific
/// constraints are proven externally. The Effect VM binds to the external
/// proof via `custom_proof_commitment` in the params.
pub const CUSTOM: usize = 8;
/// Slash an expired obligation: transfer locked stake to beneficiary.
pub const SLASH_OBLIGATION: usize = 9;
/// Seal: lock a field against mutation via sealed_field_mask.
pub const SEAL: usize = 10;
/// Unseal: unlock a sealed field (requires brand matching).
pub const UNSEAL: usize = 11;
/// MakeSovereign: transition cell mode_flag from 0 to 1.
pub const MAKE_SOVEREIGN: usize = 12;
/// CreateCellFromFactory: record factory VK hash + provenance.
pub const CREATE_CELL_FROM_FACTORY: usize = 13;
/// ExportSturdyRef: export a cell as a sturdy reference (creates swiss entry).
pub const EXPORT_STURDY_REF: usize = 14;
/// EnlivenRef: enliven a sturdy ref (validate swiss, create routing).
pub const ENLIVEN_REF: usize = 15;
/// DropRef: drop a remote reference (GC decrement).
pub const DROP_REF: usize = 16;
/// ValidateHandoff: validate a handoff certificate (check cert hash membership).
pub const VALIDATE_HANDOFF: usize = 17;
/// AllocateQueue: create a new MerkleQueue (storage Phase 2).
pub const ALLOCATE_QUEUE: usize = 18;
/// EnqueueMessage: append a message to a queue (storage Phase 2).
pub const ENQUEUE_MESSAGE: usize = 19;
/// DequeueMessage: advance queue head, reveal message (storage Phase 2).
pub const DEQUEUE_MESSAGE: usize = 20;
/// ResizeQueue: change queue capacity (storage Phase 2).
pub const RESIZE_QUEUE: usize = 21;
/// AtomicQueueTx: prove an atomic cross-queue transaction (storage Phase 3).
pub const ATOMIC_QUEUE_TX: usize = 22;
/// PipelineStep: prove a pipeline step correctly routed a message (storage Phase 3).
pub const PIPELINE_STEP: usize = 23;
