use crate::hook::{HookSpec, HookTopic, Filter, FilterValue, StreamEvent, EdgeEvent};
use crate::types::SubscriptionId;
use std::collections::HashMap;

pub struct HookDispatchCoordinator {
    subscriptions: HashMap<SubscriptionId, HookSpec>,
}

impl HookDispatchCoordinator {
    pub fn new() -> Self {
        Self {
            subscriptions: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: SubscriptionId, spec: HookSpec) {
        self.subscriptions.insert(id, spec);
    }

    pub fn dispatch_stream(&self, event: &StreamEvent) -> Vec<SubscriptionId> {
        self.matching_subscriptions(event.topic)
    }

    pub fn dispatch_edge(&self, event: &EdgeEvent) -> Vec<SubscriptionId> {
        self.matching_subscriptions(event.topic)
    }

    fn matching_subscriptions(&self, topic: HookTopic) -> Vec<SubscriptionId> {
        let mut matches: Vec<_> = self.subscriptions
            .iter()
            .filter(|(_, spec)| spec.topic == topic)
            .map(|(id, _)| *id)
            .collect();
        matches.sort_by_key(|id| {
            std::cmp::Reverse(self.subscriptions.get(id).map(|s| s.priority).unwrap_or(0))
        });
        matches
    }
}

pub fn matches_filter(filter: &Filter, payload: &serde_json::Value) -> bool {
    for (field, expected) in &filter.fields {
        let actual = payload.get(field);
        let ok = match (expected, actual) {
            (FilterValue::Str(s), Some(serde_json::Value::String(a))) => s == a,
            (FilterValue::StrSet(set), Some(serde_json::Value::String(a))) => set.contains(a),
            (FilterValue::Wildcard, _) => true,
            (FilterValue::Regex(re), Some(serde_json::Value::String(a))) => {
                regex::Regex::new(re).map(|r| r.is_match(a)).unwrap_or(false)
            }
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    true
}
