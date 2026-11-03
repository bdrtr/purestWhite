//! Wi-Fi through esp-radio (the `wifi` feature), for gate 2 (ROADMAP.md): a scan of the networks
//! around (`scan`), the network the C++ firmware saved (`saved`), joining it (`join`), and an
//! address from DHCP over embassy-net (`net`), a file fetched over HTTPS (`https`); the steps the
//! test images share (`steps`). The radio needs the scheduler started first (`esp_rtos`) and takes
//! its memory from the global heap (internal RAM). Nothing here notes a network's name, a
//! password or an address: the report goes on the card.

pub mod https;
pub mod join;
pub mod net;
pub mod saved;
pub mod scan;
pub mod steps;

pub use scan::{MOST, Scan, ScanError, scan};
