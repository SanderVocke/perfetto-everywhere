use perfetto_everywhere::{
    Category, Field, FieldName, FieldValue, FlowAttachment, NoopBackend, StaticName, TraceBackend,
    Tracer, TrackId,
};

const APP: Category = Category::new("application");
const COMPILE: StaticName = StaticName::new("compile graph");
const READY: StaticName = StaticName::new("graph ready");
const NODES: FieldName = FieldName::new("nodes");

// This instrumentation function has no native/browser conditionals.
fn instrument<B: TraceBackend>(tracer: &Tracer<B>) {
    let flow = tracer.new_flow();
    let fields = [Field::new(NODES, FieldValue::U64(12))];
    {
        let _span = tracer.span_on(
            APP,
            COMPILE,
            TrackId::CURRENT,
            &fields,
            FlowAttachment::Step(flow),
        );
        let _ = tracer.counter_f64(StaticName::new("load"), TrackId(5), 0.75);
    }
    let _ = tracer.event_on(
        APP,
        READY,
        TrackId::CURRENT,
        &[],
        FlowAttachment::Terminate(flow),
    );
}

fn main() {
    instrument(&Tracer::new(NoopBackend));
}
