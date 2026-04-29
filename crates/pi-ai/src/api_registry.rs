use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};

use crate::provider::ApiProvider;

struct Registry {
    providers: HashMap<&'static str, Arc<dyn ApiProvider + Send + Sync>>,
    // source_id -> source api slugs
    by_source: HashMap<&'static str, Vec<&'static str>>,
}

fn global_registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        Mutex::new(Registry {
            providers: HashMap::new(),
            by_source: HashMap::new(),
        })
    })
}

pub fn register_api_provider(
    provider: Arc<dyn ApiProvider + Send + Sync>,
    source_id: Option<&'static str>,
) {
    let registry = global_registry();
    let mut guard = registry.lock().unwrap();

    let api = provider.api();
    guard.providers.insert(api, Arc::clone(&provider));

    if let Some(source_id) = source_id {
        guard.by_source.entry(source_id).or_default().push(api);
    }
}

pub fn get_api_provider(api: &str) -> Option<Arc<dyn ApiProvider + Send + Sync>> {
    let registry = global_registry();
    let guard = registry.lock().unwrap();

    guard.providers.get(api).cloned()
}

pub fn unregister_api_providers(source_id: &'static str) {
    let registry = global_registry();
    let mut guard = registry.lock().unwrap();

    if let Some(apis) = guard.by_source.remove(source_id) {
        for api in apis {
            guard.providers.remove(api);
        }
    }
}

pub fn clear_api_providers() {
    let registry = global_registry();
    let mut guard = registry.lock().unwrap();

    guard.providers.clear();
    guard.by_source.clear();
}
