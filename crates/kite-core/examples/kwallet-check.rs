//! Developer tool: is KWallet reachable? (read-only: does not open the wallet)
fn main() {
    println!("KWallet available: {}", kite_core::kwallet::KWallet::available());
}
