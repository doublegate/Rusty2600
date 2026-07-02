//! A minimal RFC 5389 STUN client, used to discover this machine's public
//! (NAT-mapped) `ip:port` before starting a [`crate::RollbackSession`].
//!
//! **Scope, stated plainly**: this module gets the local peer as far as
//! "here is the address a STUN server sees me as" plus a best-effort UDP
//! hole-punch (a handful of packets sent toward the remote peer's own
//! STUN-discovered address before the GGRS handshake begins). It does
//! **not** implement WebRTC (browser or native) — that remains its own,
//! separately-scoped future release, since it needs either browser-only
//! `web-sys` bindings or a native Rust WebRTC stack that would pull an
//! async runtime into a codebase that is otherwise 100% synchronous `std`.
//!
//! **Verification boundary**: the STUN round-trip itself is genuinely
//! live-tested against a real public STUN server (see the
//! `discovers_a_real_public_address` test below) — a single outbound UDP
//! request/response this machine can complete entirely on its own. Real
//! NAT traversal between two independently-NATed peers is a different
//! claim this module does **not** verify: that needs two machines on two
//! different networks, which a single-host sandbox cannot provide (a
//! loopback test never crosses a NAT boundary at all). Mirrors this
//! project's own `docs/mobile.md` "iOS Verification" precedent for
//! infrastructure that can be built and partially verified, but not fully
//! proven, in this environment.

use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use bytecodec::{DecodeExt, EncodeExt};
use stun_codec::rfc5389::Attribute;
use stun_codec::rfc5389::attributes::XorMappedAddress;
use stun_codec::rfc5389::methods::BINDING;
use stun_codec::{Message, MessageClass, MessageDecoder, MessageEncoder, TransactionId};

/// Google's public STUN server — widely used, free, no auth required. A
/// reasonable default; callers may supply a different server via
/// [`discover_public_address`]'s `stun_server` parameter.
pub const DEFAULT_STUN_SERVER: &str = "stun.l.google.com:19302";

/// How long to wait for a STUN Binding Response before giving up.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(3);

/// Everything that can go wrong discovering this machine's public address.
#[derive(Debug)]
pub enum StunError {
    /// A socket operation (send/receive/set timeout) failed.
    Io(std::io::Error),
    /// The STUN message failed to encode or decode.
    Codec(String),
    /// The server replied, but not with a class/attribute this client
    /// understands (e.g. an `ErrorResponse`, or a `SuccessResponse` with no
    /// `XOR-MAPPED-ADDRESS` attribute).
    UnexpectedResponse,
}

impl core::fmt::Display for StunError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "STUN socket I/O error: {e}"),
            Self::Codec(e) => write!(f, "STUN message codec error: {e}"),
            Self::UnexpectedResponse => {
                write!(f, "STUN server response was not a usable success response")
            }
        }
    }
}

/// Send a STUN Binding Request to `stun_server` over `socket` and return the
/// public `ip:port` the server observed the request arrive FROM.
///
/// This is the `XOR-MAPPED-ADDRESS` attribute in the server's response —
/// this machine's own NAT-mapped address for `socket`'s current local port.
///
/// `socket` should already be bound to the local port the caller intends to
/// use for the netplay session itself — the discovered mapping is only
/// valid for the exact `(local port, external server)` pair that produced
/// it, so this MUST run on the same socket [`crate::RollbackSession`] will
/// later use, not a throwaway one.
///
/// Temporarily puts `socket` into blocking mode with a read timeout for the
/// duration of the round trip; callers driving a non-blocking GGRS session
/// should call `socket.set_nonblocking(true)` again afterward (before
/// wrapping it for GGRS — see `PunchedUdpSocket`).
///
/// # Errors
///
/// See [`StunError`].
pub fn discover_public_address(
    socket: &UdpSocket,
    stun_server: SocketAddr,
) -> Result<SocketAddr, StunError> {
    let request = Message::<Attribute>::new(
        MessageClass::Request,
        BINDING,
        TransactionId::new(fresh_transaction_id()),
    );

    let mut encoder = MessageEncoder::new();
    let bytes = encoder
        .encode_into_bytes(request)
        .map_err(|e| StunError::Codec(e.to_string()))?;

    socket
        .set_read_timeout(Some(RESPONSE_TIMEOUT))
        .map_err(StunError::Io)?;
    socket.send_to(&bytes, stun_server).map_err(StunError::Io)?;

    let mut buf = [0u8; 512];
    let (n, _from) = socket.recv_from(&mut buf).map_err(StunError::Io)?;

    let mut decoder = MessageDecoder::<Attribute>::new();
    let response: Message<Attribute> = decoder
        .decode_from_bytes(&buf[..n])
        .map_err(|e| StunError::Codec(e.to_string()))?
        .map_err(|e| StunError::Codec(format!("{e:?}")))?;

    if response.class() != MessageClass::SuccessResponse {
        return Err(StunError::UnexpectedResponse);
    }

    response
        .get_attribute::<XorMappedAddress>()
        .map(XorMappedAddress::address)
        .ok_or(StunError::UnexpectedResponse)
}

/// A 96-bit value reasonably unique enough to correlate one STUN request
/// with its response over a single round trip this process fully controls
/// end-to-end. Deliberately NOT cryptographically random — this is a
/// protocol nonce for local request/response matching, not a security
/// boundary, so pulling in the `rand` crate as a new direct dependency
/// (ggrs already depends on it transitively, but not in a way this crate
/// can reuse without its own `Cargo.toml` entry) isn't warranted.
fn fresh_transaction_id() -> [u8; 12] {
    // Truncating the 128-bit nanosecond count to its low 64 bits is fine
    // here: only enough entropy to make one request/response pair locally
    // distinguishable is needed, not the full timestamp.
    #[allow(clippy::cast_possible_truncation)]
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let pid = std::process::id();
    let mut id = [0u8; 12];
    id[0..8].copy_from_slice(&nanos.to_be_bytes());
    id[8..12].copy_from_slice(&pid.to_be_bytes());
    id
}

/// Send a handful of best-effort UDP packets toward `remote_addr`, opening
/// this NAT's outbound mapping before the GGRS handshake begins.
///
/// This is deliberately simple — a fixed burst of empty datagrams, not a
/// punch-and-retry state machine — matching this feature's scope: getting
/// the plumbing real and testable, not building a production-grade NAT
/// traversal library. **Its actual effectiveness against a real NAT is
/// unverified** (see the module doc's "Verification boundary").
pub fn hole_punch(socket: &UdpSocket, remote_addr: SocketAddr) {
    const PUNCH_PACKETS: u8 = 5;
    for _ in 0..PUNCH_PACKETS {
        // Best-effort: a dropped punch packet is expected and harmless (the
        // GGRS handshake that follows will retry at the protocol level).
        let _ = socket.send_to(&[], remote_addr);
    }
}

#[cfg(test)]
mod tests {
    use std::net::ToSocketAddrs;

    use super::*;

    /// The test that actually proves the STUN client works, not just that
    /// it compiles: a real Binding Request/Response round trip against a
    /// real public STUN server. **Confirmed working in this development
    /// sandbox** (this exact test passed against `stun.l.google.com:19302`
    /// during implementation) — but `#[ignore]`d by default anyway, since a
    /// CI runner's outbound UDP access is a separate, less certain
    /// question than this sandbox's, and a flaky external-network
    /// dependency has no business gating `cargo test --workspace`'s normal
    /// green/red signal. Run explicitly (`cargo test -- --ignored`) to
    /// prove the client for real.
    #[test]
    #[ignore = "requires live outbound UDP access to a public STUN server; run explicitly with --ignored"]
    fn discovers_a_real_public_address() {
        let socket = UdpSocket::bind("0.0.0.0:0").expect("bind an ephemeral local UDP port");
        let stun_server: SocketAddr = DEFAULT_STUN_SERVER
            .to_socket_addrs()
            .expect("resolve the default STUN server's hostname")
            .next()
            .expect("at least one resolved address");

        let addr = discover_public_address(&socket, stun_server)
            .expect("a real STUN round trip against a real public server should succeed");

        // A real public IPv4/IPv6 address was returned — not a loopback or
        // unspecified placeholder, confirming the XOR-MAPPED-ADDRESS
        // attribute actually decoded to something meaningful.
        assert!(!addr.ip().is_loopback());
        assert!(!addr.ip().is_unspecified());
        assert_ne!(addr.port(), 0);
    }

    #[test]
    fn fresh_transaction_ids_are_not_all_zero() {
        // A cheap sanity check that the nonce generator isn't degenerately
        // constant (e.g. always [0; 12] on a `duration_since` underflow) -
        // does not claim cryptographic quality, see the function's own doc.
        let id = fresh_transaction_id();
        assert_ne!(id, [0u8; 12]);
    }
}
