use directioner_services::IntentBroker;

fn main() {
    let _broker = IntentBroker::new(0.7);
    println!("directioner-intentd initialized with min confidence 0.7");
}
