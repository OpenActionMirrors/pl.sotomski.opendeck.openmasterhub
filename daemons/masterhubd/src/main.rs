#[tokio::main]
async fn main() {
    if let Err(err) = masterhubd::run().await {
        eprintln!("masterhubd: {err}");
        std::process::exit(1);
    }
}
