use crate::hook::{HookSpec, HookTopic, Filter, FilterValue, StreamEvent, EdgeEvent};
use crate::types::SubscriptionId;
use std::collections::HashMap;
use tracing::warn;

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

    pub fn unregister(&mut self, id: SubscriptionId) -> Option<HookSpec> {
        self.subscriptions.remove(&id)
    }

    pub fn dispatch_stream(&self, event: &StreamEvent) -> Vec<SubscriptionId> {
        self.matching_subscriptions(event.topic, Some(&event.payload))
    }

    pub fn dispatch_edge(&self, event: &EdgeEvent) -> Vec<SubscriptionId> {
        self.matching_subscriptions(event.topic, Some(&event.payload))
    }

    fn matching_subscriptions(
        &self,
        topic: HookTopic,
        payload: Option<&bytes::Bytes>,
    ) -> Vec<SubscriptionId> {
        let mut matches: Vec<_> = self.subscriptions
            .iter()
            .filter(|(_, spec)| spec.topic == topic)
            .filter(|(_, spec)| {
                match (&spec.filter, payload) {
                    (Some(filter), Some(payload_bytes)) => {
                        match serde_json::from_slice::<serde_json::Value>(payload_bytes) {
                            Ok(json) => matches_filter(filter, &json),
                            Err(e) => {
                                warn!(
                                    topic = ?topic,
                                    error = %e,
                                    "hook payload is not valid JSON, dropping event"
                                );
                                false
                            }
                        }
                    }
                    (Some(_), None) => false,
                    (None, _) => true,
                }
            })
            .map(|(id, _)| *id)
            .collect();
        matches.sort_by_key(|id| {
            std::cmp::Reverse(self.subscriptions.get(id).map(|s| s.priority).unwrap_or(0))
        });
        matches
    }

    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }
}

impl Default for HookDispatchCoordinator {
    fn default() -> Self {
        Self::new()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn make_filter(pairs: &[(&str, FilterValue)]) -> Filter {
        Filter {
            fields: pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        }
    }

    #[test]
    fn filter_string_match() {
        let filter = make_filter(&[("agent_state", FilterValue::Str("working".into()))]);
        let payload = serde_json::json!({"agent_state": "working"});
        assert!(matches_filter(&filter, &payload));
    }

    #[test]
    fn filter_string_no_match() {
        let filter = make_filter(&[("agent_state", FilterValue::Str("working".into()))]);
        let payload = serde_json::json!({"agent_state": "idle"});
        assert!(!matches_filter(&filter, &payload));
    }

    #[test]
    fn filter_wildcard() {
        let filter = make_filter(&[("any_field", FilterValue::Wildcard)]);
        let payload = serde_json::json!({"any_field": "anything"});
        assert!(matches_filter(&filter, &payload));
    }

    #[test]
    fn filter_regex() {
        let filter = make_filter(&[("pane_title", FilterValue::Regex("^vim".into()))]);
        let payload = serde_json::json!({"pane_title": "vim ~/.bashrc"});
        assert!(matches_filter(&filter, &payload));
    }

    #[test]
    fn filter_str_set() {
        let mut set = HashSet::new();
        set.insert("claude".to_string());
        set.insert("codex".to_string());
        let filter = make_filter(&[("agent_cli", FilterValue::StrSet(set))]);
        let payload = serde_json::json!({"agent_cli": "claude"});
        assert!(matches_filter(&filter, &payload));
    }

    #[test]
    fn priority_ordering() {
        let mut coord = HookDispatchCoordinator::new();
        coord.register(SubscriptionId(1), HookSpec {
            topic: HookTopic::PaneOutput,
            filter: None,
            mode: crate::hook::HookMode::Stream,
            delivery: crate::hook::Delivery::Async,
            buffer: None,
            priority: 10,
        });
        coord.register(SubscriptionId(2), HookSpec {
            topic: HookTopic::PaneOutput,
            filter: None,
            mode: crate::hook::HookMode::Stream,
            delivery: crate::hook::Delivery::Async,
            buffer: None,
            priority: 90,
        });
        let event = StreamEvent {
            topic: HookTopic::PaneOutput,
            payload: bytes::Bytes::from("hello"),
            timestamp_ms: 0,
            source_ctx: crate::types::ContextId::Session,
        };
        let matches = coord.dispatch_stream(&event);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0], SubscriptionId(2));
    }
}
