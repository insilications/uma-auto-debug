use std::net::{Ipv4Addr, SocketAddrV4};

#[derive(Debug)]
pub struct AdbOptions {
    pub address: SocketAddrV4,
}

// Manual implementation of the Default trait
impl Default for AdbOptions {
    fn default() -> Self {
        // The standard ADB address is 127.0.0.1:5037
        const ADB_DEFAULT_PORT: u16 = 5037;
        Self {
            address: SocketAddrV4::new(Ipv4Addr::LOCALHOST, ADB_DEFAULT_PORT),
        }
    }
}
