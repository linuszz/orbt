use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use crate::error::PluginError;
use crate::types::PluginId;

pub struct NativeThreadPool {
    threads: Vec<NativeThread>,
    assignment: HashMap<PluginId, usize>,
}

struct NativeThread {
    name: String,
    handle: Option<std::thread::JoinHandle<()>>,
    queue: Sender<Box<dyn FnOnce() + Send>>,
}

impl NativeThreadPool {
    pub fn new(thread_count: usize) -> Self {
        let threads = (0..thread_count)
            .map(|i| {
                let name = format!("native-thread-{}", i);
                let (tx, rx) = channel::<Box<dyn FnOnce() + Send>>();
                let handle = std::thread::Builder::new()
                    .name(name.clone())
                    .spawn(move || {
                        while let Ok(f) = rx.recv() {
                            f();
                        }
                    })
                    .expect("failed to spawn native plugin thread");
                NativeThread {
                    name,
                    handle: Some(handle),
                    queue: tx,
                }
            })
            .collect();
        Self {
            threads,
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
        self.assignment
            .get(plugin_id)
            .map(|&i| self.threads[i].name.as_str())
    }

    pub fn execute_on(
        &self,
        plugin_id: &PluginId,
        f: Box<dyn FnOnce() + Send>,
    ) -> Result<(), PluginError> {
        let idx = self
            .assignment
            .get(plugin_id)
            .copied()
            .ok_or_else(|| PluginError::Panicked(format!("plugin {} not assigned", plugin_id)))?;
        self.threads[idx]
            .queue
            .send(f)
            .map_err(|e| PluginError::Panicked(format!("thread queue error: {}", e)))
    }

    pub fn shutdown(mut self) {
        for mut thread in self.threads.drain(..) {
            drop(thread.queue);
            if let Some(handle) = thread.handle.take() {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for NativeThreadPool {
    fn drop(&mut self) {
        for thread in &mut self.threads {
            drop(thread.queue.clone());
        }
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
