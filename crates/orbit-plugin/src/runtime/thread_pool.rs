use std::collections::HashMap;
use crate::types::PluginId;

pub struct NativeThreadPool {
    threads: Vec<String>,
    assignment: HashMap<PluginId, usize>,
}

impl NativeThreadPool {
    pub fn new(thread_count: usize) -> Self {
        Self {
            threads: (0..thread_count).map(|i| format!("native-thread-{}", i)).collect(),
            assignment: HashMap::new(),
        }
    }

    pub fn assign(&mut self, plugin_id: &PluginId) -> usize {
        let hash = fnv_hash(&plugin_id.0);
        let idx = (hash % self.threads.len() as u64) as usize;
        self.assignment.insert(plugin_id.clone(), idx);
        idx
    }

    pub fn thread_for(&self, plugin_id: &PluginId) -> Option<&str> {
        self.assignment.get(plugin_id).map(|&i| self.threads[i].as_str())
    }
}

fn fnv_hash(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
