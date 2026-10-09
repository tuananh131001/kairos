use crate::types::{ActivityKind, Timestamp};

#[derive(Clone, Debug, PartialEq)]
pub enum SegmentOp {
    Open { kind: ActivityKind, at: Timestamp },
    Close { at: Timestamp },
}

#[derive(Clone, Debug, Default)]
pub struct ActivityRecorder {
    open: Option<ActivityKind>,
}

impl ActivityRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> Option<ActivityKind> {
        self.open
    }

    pub fn record(&mut self, span_start: Timestamp, kind: ActivityKind) -> Vec<SegmentOp> {
        match self.open {
            Some(open) if open == kind => Vec::new(),
            Some(_) => {
                self.open = Some(kind);
                vec![
                    SegmentOp::Close { at: span_start },
                    SegmentOp::Open {
                        kind,
                        at: span_start,
                    },
                ]
            }
            None => {
                self.open = Some(kind);
                vec![SegmentOp::Open {
                    kind,
                    at: span_start,
                }]
            }
        }
    }

    pub fn stop(&mut self, at: Timestamp) -> Vec<SegmentOp> {
        match self.open.take() {
            Some(_) => vec![SegmentOp::Close { at }],
            None => Vec::new(),
        }
    }
}
