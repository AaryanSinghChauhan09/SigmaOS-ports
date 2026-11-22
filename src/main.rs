//! SigmaOS component binary: ports

use sigmaos_ports::Component;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Component::new().operate()?;
    println!("sigmaos-ports: ok");
    Ok(())
}
