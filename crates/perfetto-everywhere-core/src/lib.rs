#![doc = "Platform-neutral tracing semantics and the compact browser record protocol."]
#![forbid(unsafe_code)]
#![no_std]

mod api;
mod metadata;
mod protocol;

pub use api::{
    EmitStatus, Field, FieldValue, FlowAttachment, FlowId, NoopBackend, Severity, SpanGuard,
    TraceBackend, Tracer, TrackId,
};
pub use metadata::{
    Category, FieldName, MetadataCollision, MetadataDef, MetadataId, StaticName, validate_metadata,
};
pub use protocol::{
    FLAG_FLOW_STEP, FLAG_FLOW_TERMINATE, FLAG_GROUP_END, FLAG_GROUP_START, ProtocolError,
    RECORD_SIZE, RECORD_VERSION, Record, RecordKind, validate_record_groups,
};
