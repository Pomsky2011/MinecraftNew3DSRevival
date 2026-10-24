//! The integrated server (DESIGN.md §4.2): `n3dsr-core` driven at 20 ticks a second, exchanging
//! `n3dsr-protocol` messages with the client. Nothing here knows the 3DS.

// Not used yet, but already linked so the release feature reaches them (DESIGN.md §5.6).
use n3dsr_core as _;
use n3dsr_protocol as _;
