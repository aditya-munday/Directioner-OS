use directioner_services::CloudSyncEngine;

fn main() {
    let engine = CloudSyncEngine::new();
    println!("directioner-cloudd initialized. Pending sync: {}", engine.pending_count());
}
