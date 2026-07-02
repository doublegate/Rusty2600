//! Browser WebRTC transport (`[2.6.0]`, ADR 0008) — `wasm32`-only.
//!
//! Two pieces:
//!
//! - [`WebRtcSocket`] — a [`ggrs::NonBlockingSocket<SocketAddr>`] over an
//!   **already-open** [`web_sys::RtcDataChannel`], the WebRTC analogue of
//!   [`crate::session`]'s `PunchedUdpSocket`. Constructed from an open
//!   channel; `send_to`/`receive_all_messages` are fully synchronous — no
//!   `await` anywhere in this struct, per ADR 0008's binding decision.
//! - [`WebRtcPeer`] — the one-time, inherently-async connection
//!   ESTABLISHMENT dance (create the peer connection + data channel,
//!   generate/apply an SDP offer or answer, wait for ICE gathering to
//!   finish). This is the part ADR 0008 allows to be async, contained here
//!   and never touching [`WebRtcSocket`]'s synchronous hot path. Exposed
//!   via `#[wasm_bindgen]` so both `rusty2600-frontend`'s real netplay UI
//!   and this crate's own standalone `web/` test harness (`docs/netplay.md`)
//!   can drive it.
//!
//! # Why a sentinel `SocketAddr`
//!
//! [`crate::config::RustyConfig::Address`] is a crate-wide fixed
//! `std::net::SocketAddr` (not a generic [`RollbackSession`](crate::RollbackSession)
//! parameter), and this crate is deliberately 2-player-only (see
//! `session.rs`'s module doc), so a WebRTC socket only ever has exactly one
//! peer. [`SENTINEL_PEER_ADDR`] stands in for "the one WebRTC peer" — it is
//! NEVER a real IP address and must never be surfaced anywhere in the UI/API
//! as if it were one (ADR 0008's own consequence note).
//!
//! # Why "unreliable + unordered"
//!
//! The data channel is created with `maxRetransmits: 0` and `ordered:
//! false` — matching RustyNES's own `WebRtcTransport` reasoning
//! (`rustynes-netplay::webrtc`, read directly as this module's concrete
//! web-sys API pattern reference): the rollback protocol already tolerates
//! lossy, out-of-order delivery (that's what GGRS's own resend/resimulate
//! logic is FOR), so this matches native UDP's delivery semantics rather
//! than fighting them with reliable, ordered, TCP-like delivery, which
//! would add head-of-line-blocking latency the rollback protocol doesn't
//! need and was never designed around.
//!
//! # `clippy::future_not_send`
//!
//! Every `async fn` in this module is inherently `!Send`: `web_sys`/
//! `wasm_bindgen_futures::JsFuture` wrap `Rc<RefCell<_>>`-based JS bindings
//! that only ever run on the browser's single JS thread (there is no
//! `Send` requirement to satisfy on `wasm32-unknown-unknown` — nothing here
//! is ever moved across a thread boundary). Allowed module-wide rather than
//! per-function, since it applies uniformly to every async item below.

#![allow(clippy::future_not_send)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::rc::Rc;

use ggrs::{Message, NonBlockingSocket};
use js_sys::Reflect;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    MessageEvent, RtcConfiguration, RtcDataChannel, RtcDataChannelInit, RtcDataChannelType,
    RtcIceGatheringState, RtcPeerConnection, RtcSdpType, RtcSessionDescriptionInit,
};

/// Stands in for "the one WebRTC peer" in [`ggrs::NonBlockingSocket`]'s
/// address-keyed API. Never a real IP — see this module's doc comment.
pub const SENTINEL_PEER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);

/// The shared inbound queue the data channel's `onmessage` callback fills
/// and [`WebRtcSocket::receive_all_messages`] drains — same shape as
/// `RustyNES`'s `webrtc::Inbox`.
type Inbox = Rc<RefCell<VecDeque<Vec<u8>>>>;

/// A [`ggrs::NonBlockingSocket<SocketAddr>`] over a WebRTC
/// [`RtcDataChannel`] — see this module's doc comment.
pub struct WebRtcSocket {
    channel: RtcDataChannel,
    inbox: Inbox,
    /// Kept alive for the socket's lifetime: dropping this would silence
    /// the channel's `onmessage` callback (the browser holds only a raw JS
    /// reference into it, not an owning Rust reference).
    _on_message: Closure<dyn FnMut(MessageEvent)>,
}

impl WebRtcSocket {
    /// Wrap an **already-open**, unreliable+unordered [`RtcDataChannel`].
    ///
    /// Sets the channel's binary type to `arraybuffer` (so `onmessage`
    /// yields raw bytes synchronously, never a `Blob`, which would need an
    /// async read) and installs the `onmessage` handler that decodes each
    /// inbound frame with `bincode` — the exact wire format
    /// `PunchedUdpSocket` already uses for `ggrs::Message`, so
    /// `RollbackSession::with_webrtc_socket` and `RollbackSession::
    /// with_socket` are wire-compatible with each other (not that they'd
    /// ever talk to each other directly, but it means there is exactly ONE
    /// `ggrs::Message` wire format in this crate, not two).
    #[must_use]
    pub fn new(channel: RtcDataChannel) -> Self {
        channel.set_binary_type(RtcDataChannelType::Arraybuffer);
        let inbox: Inbox = Rc::new(RefCell::new(VecDeque::new()));
        let inbox_cb = Rc::clone(&inbox);

        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |evt: MessageEvent| {
            if let Ok(buf) = evt.data().dyn_into::<js_sys::ArrayBuffer>() {
                let bytes = js_sys::Uint8Array::new(&buf).to_vec();
                inbox_cb.borrow_mut().push_back(bytes);
            }
            // A non-ArrayBuffer payload (shouldn't happen given
            // `set_binary_type` above) is silently dropped — same policy
            // as a malformed UDP datagram.
        });
        channel.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        Self {
            channel,
            inbox,
            _on_message: on_message,
        }
    }

    /// The underlying data channel (e.g. to inspect `ready_state()`).
    #[must_use]
    pub const fn channel(&self) -> &RtcDataChannel {
        &self.channel
    }
}

impl Drop for WebRtcSocket {
    /// Detaches the `onmessage` handler before `_on_message` deallocates.
    ///
    /// Without this, a message arriving on `channel` after this socket is
    /// dropped (e.g. a shutdown/reconnect path where the channel itself
    /// outlives the socket) would have the browser invoke a JS reference to
    /// an already-freed `Closure` — a wasm-bindgen trap/panic, not a benign
    /// no-op.
    fn drop(&mut self) {
        self.channel.set_onmessage(None);
    }
}

impl NonBlockingSocket<SocketAddr> for WebRtcSocket {
    fn send_to(&mut self, msg: &Message, _addr: &SocketAddr) {
        // `_addr` is always `SENTINEL_PEER_ADDR` in practice (the only
        // address this crate's `SessionBuilder` ever registers a WebRTC
        // peer under) — the data channel IS the point-to-point connection,
        // so there is nothing to route on.
        let Ok(buf) = bincode::serialize(msg) else {
            return;
        };
        // A send to a not-yet-open or already-closed channel returns a JS
        // error; swallowed exactly like `PunchedUdpSocket::send_to` swallows
        // a `UdpSocket::send_to` I/O error — the rollback protocol
        // tolerates the loss and resends.
        let _ = self.channel.send_with_u8_array(&buf);
    }

    fn receive_all_messages(&mut self) -> Vec<(SocketAddr, Message)> {
        self.inbox
            .borrow_mut()
            .drain(..)
            .filter_map(|bytes| bincode::deserialize(&bytes).ok())
            .map(|msg| (SENTINEL_PEER_ADDR, msg))
            .collect()
    }
}

/// The one-time, async connection-establishment dance — see this module's
/// doc comment for why this (and only this) part of WebRTC netplay is
/// async, per ADR 0008.
///
/// `#[wasm_bindgen]`-exported so both a real netplay UI and this crate's
/// standalone `web/` test harness can drive it from JS/the browser
/// dev-tools console without needing their own copy of this logic.
#[wasm_bindgen]
pub struct WebRtcPeer {
    pc: RtcPeerConnection,
    channel: RtcDataChannel,
}

#[wasm_bindgen]
impl WebRtcPeer {
    /// Host side: create the peer connection + an unreliable/unordered data
    /// channel, generate an SDP offer, wait for ICE candidate gathering to
    /// finish, and return the complete local description — a plain SDP
    /// string (the standard `RTCSessionDescription.sdp` text format, with
    /// every gathered ICE candidate already folded in as `a=candidate`
    /// lines, since gathering has finished by the time this returns — NOT
    /// a JSON envelope) ready to hand to the joining peer out-of-band
    /// (copy-paste, per ADR 0008 — no signaling server).
    ///
    /// Waiting for `icegatheringstate == "complete"` before returning (a
    /// "trickle-less" exchange) is a deliberate simplification: it turns a
    /// multi-message ICE-candidate stream into ONE copy-pasteable blob,
    /// which is what a minimal manual exchange needs — a real production
    /// signaling server would stream candidates individually for lower
    /// connection latency, but that's not this release's job.
    ///
    /// # Errors
    /// Returns a `JsValue` (a raw JS exception) if peer-connection/data-
    /// channel/offer creation fails at the browser API level.
    #[wasm_bindgen(js_name = createOffer)]
    pub async fn create_offer() -> Result<CreateOfferResult, JsValue> {
        let pc = new_peer_connection()?;
        let channel = create_data_channel(&pc);

        let offer = JsFuture::from(pc.create_offer()).await?;
        // `create_offer()`'s Promise resolves to a plain JS object shaped
        // like `RTCSessionDescriptionInit` (`{type, sdp}`), not a
        // wasm-bindgen-typed value — `type` is known statically here (this
        // IS the offer side), so only `sdp` needs reading back via
        // `Reflect`.
        let sdp = Reflect::get(&offer, &JsValue::from_str("sdp"))?;
        let desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        desc.set_sdp(&sdp.as_string().unwrap_or_default());
        JsFuture::from(pc.set_local_description(&desc)).await?;

        wait_for_ice_gathering_complete(&pc).await?;

        let local_desc = pc
            .local_description()
            .ok_or_else(|| JsValue::from_str("no local description after set_local_description"))?;
        let blob = local_desc.sdp();

        let peer = Self { pc, channel };
        Ok(CreateOfferResult { peer, offer: blob })
    }

    /// Joiner side: accept the host's offer blob (from [`Self::create_offer`],
    /// copy-pasted out-of-band), create the peer connection, generate an
    /// SDP answer, wait for ICE gathering to finish, and return the answer
    /// blob to hand back to the host.
    ///
    /// # Errors
    /// See [`Self::create_offer`].
    #[wasm_bindgen(js_name = createAnswer)]
    pub async fn create_answer(offer_sdp: String) -> Result<CreateAnswerResult, JsValue> {
        let pc = new_peer_connection()?;

        // The joiner does NOT create its own data channel — WebRTC data
        // channels are negotiated by whichever side calls `createDataChannel`;
        // the joiner instead receives it via `ondatachannel` once the
        // connection completes.
        let channel_cell: Rc<RefCell<Option<RtcDataChannel>>> = Rc::new(RefCell::new(None));
        let channel_cb = Rc::clone(&channel_cell);
        let on_datachannel = Closure::<dyn FnMut(web_sys::RtcDataChannelEvent)>::new(
            move |evt: web_sys::RtcDataChannelEvent| {
                *channel_cb.borrow_mut() = Some(evt.channel());
            },
        );
        pc.set_ondatachannel(Some(on_datachannel.as_ref().unchecked_ref()));

        let remote_desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        remote_desc.set_sdp(&offer_sdp);
        JsFuture::from(pc.set_remote_description(&remote_desc)).await?;

        let answer = JsFuture::from(pc.create_answer()).await?;
        // See `create_offer`'s matching comment — `type` is known statically
        // (this IS the answer side), only `sdp` needs reading back.
        let sdp = Reflect::get(&answer, &JsValue::from_str("sdp"))?;
        let desc = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
        desc.set_sdp(&sdp.as_string().unwrap_or_default());
        JsFuture::from(pc.set_local_description(&desc)).await?;

        wait_for_ice_gathering_complete(&pc).await?;

        let local_desc = pc
            .local_description()
            .ok_or_else(|| JsValue::from_str("no local description after set_local_description"))?;
        let blob = local_desc.sdp();

        // The data channel arrives asynchronously via `ondatachannel` — by
        // the time ICE gathering (and therefore the whole handshake-so-far)
        // completes, it should already be present; if not, `channel()` on
        // the returned peer will find it `None` and the caller should
        // treat that as "not ready yet, poll again" (matching this crate's
        // own no-panic-on-not-ready-yet convention elsewhere).
        Ok(CreateAnswerResult {
            pc,
            channel_cell,
            answer: blob,
            _on_datachannel: on_datachannel,
        })
    }

    /// Host side: apply the joiner's answer blob (from
    /// [`Self::create_answer`]) to complete the handshake.
    ///
    /// # Errors
    /// See [`Self::create_offer`].
    #[wasm_bindgen(js_name = acceptAnswer)]
    pub async fn accept_answer(&self, answer_sdp: String) -> Result<(), JsValue> {
        let remote_desc = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
        remote_desc.set_sdp(&answer_sdp);
        JsFuture::from(self.pc.set_remote_description(&remote_desc)).await?;
        Ok(())
    }

    /// Whether the data channel has reached `"open"` — the caller should
    /// poll this (e.g. once per render frame) before handing the channel to
    /// [`WebRtcSocket::new`]/`RollbackSession::with_webrtc_socket`.
    #[wasm_bindgen(js_name = isChannelOpen)]
    #[must_use]
    pub fn is_channel_open(&self) -> bool {
        self.channel.ready_state() == web_sys::RtcDataChannelState::Open
    }

    /// The underlying `RTCPeerConnection`'s `iceConnectionState` (e.g.
    /// `"new"`, `"checking"`, `"connected"`, `"failed"`) — a real-time
    /// connection-establishment status string for a netplay UI to surface
    /// to the user (e.g. "Connecting…" / "Connection failed, check your
    /// network"), and useful diagnostic signal beyond the coarser
    /// [`Self::is_channel_open`].
    #[wasm_bindgen(js_name = iceConnectionState)]
    #[must_use]
    pub fn ice_connection_state(&self) -> String {
        format!("{:?}", self.pc.ice_connection_state())
    }

    /// Consumes this peer, handing back its open data channel — the bridge
    /// into [`WebRtcSocket::new`] for real netplay use.
    #[must_use]
    pub fn into_channel(self) -> RtcDataChannel {
        self.channel
    }
}

/// [`WebRtcPeer::create_offer`]'s return value: the (host-side) peer, plus
/// the SDP offer blob to hand to the joining peer out-of-band.
#[wasm_bindgen]
pub struct CreateOfferResult {
    peer: WebRtcPeer,
    offer: String,
}

#[wasm_bindgen]
impl CreateOfferResult {
    /// The SDP offer blob to hand to the joining peer out-of-band.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn offer(&self) -> String {
        self.offer.clone()
    }

    /// Consumes this result, handing back the host-side [`WebRtcPeer`] —
    /// call [`WebRtcPeer::accept_answer`] on it once the joiner's answer
    /// blob is available.
    #[must_use]
    pub fn into_peer(self) -> WebRtcPeer {
        self.peer
    }
}

/// [`WebRtcPeer::create_answer`]'s return value: the (joiner-side)
/// connection state, plus the SDP answer blob to hand back to the host.
///
/// Not a plain [`WebRtcPeer`] because the joiner's data channel arrives
/// asynchronously via `ondatachannel`, potentially after this function
/// already returned — [`Self::try_into_peer`] converts once it has arrived.
#[wasm_bindgen]
pub struct CreateAnswerResult {
    pc: RtcPeerConnection,
    channel_cell: Rc<RefCell<Option<RtcDataChannel>>>,
    answer: String,
    /// Kept alive until [`Self::try_into_peer`] consumes this result (or
    /// this result itself is dropped) — dropping this closure without
    /// first clearing `pc`'s `ondatachannel` handler would leave a raw JS
    /// reference into deallocated memory; `try_into_peer` clears the
    /// handler before dropping this field for exactly that reason.
    _on_datachannel: Closure<dyn FnMut(web_sys::RtcDataChannelEvent)>,
}

#[wasm_bindgen]
impl CreateAnswerResult {
    /// The SDP answer blob to hand back to the host out-of-band.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn answer(&self) -> String {
        self.answer.clone()
    }

    /// Whether the data channel has arrived yet (see the struct doc).
    #[wasm_bindgen(js_name = channelReady)]
    #[must_use]
    pub fn channel_ready(&self) -> bool {
        self.channel_cell.borrow().is_some()
    }

    /// Consumes this result into a [`WebRtcPeer`], if the data channel has
    /// arrived (see [`Self::channel_ready`]) — `None` otherwise (poll again
    /// after another `await` tick, e.g. via a short `setTimeout`).
    #[wasm_bindgen(js_name = tryIntoPeer)]
    #[must_use]
    pub fn try_into_peer(self) -> Option<WebRtcPeer> {
        let channel = self.channel_cell.borrow_mut().take()?;
        // Detach the `ondatachannel` handler before `_on_datachannel` drops
        // (at the end of this method, along with the rest of `self`) — the
        // channel has already arrived, so the handler has no further job,
        // and leaving it attached to a closure about to be freed would
        // leave `pc` holding a raw JS reference into deallocated memory.
        self.pc.set_ondatachannel(None);
        Some(WebRtcPeer {
            pc: self.pc,
            channel,
        })
    }
}

/// Constructs an [`RtcPeerConnection`] with a small set of public STUN
/// servers (for real cross-NAT use; harmless/unused for the same-host
/// verification this release's own testing relies on — see `docs/netplay.md`).
fn new_peer_connection() -> Result<RtcPeerConnection, JsValue> {
    let ice_servers = js_sys::Array::new();
    let server = js_sys::Object::new();
    Reflect::set(
        &server,
        &JsValue::from_str("urls"),
        &JsValue::from_str("stun:stun.l.google.com:19302"),
    )?;
    ice_servers.push(&server);
    let config = RtcConfiguration::new();
    config.set_ice_servers(&ice_servers);
    RtcPeerConnection::new_with_configuration(&config)
}

/// Creates the host-side data channel, configured unreliable + unordered —
/// see this module's doc comment for why.
fn create_data_channel(pc: &RtcPeerConnection) -> RtcDataChannel {
    let init = RtcDataChannelInit::new();
    init.set_ordered(false);
    init.set_max_retransmits(0);
    pc.create_data_channel_with_data_channel_dict("rusty2600-netplay", &init)
}

/// Awaits `pc`'s `icegatheringstate` reaching `"complete"` — see
/// [`WebRtcPeer::create_offer`]'s doc comment for why this crate waits for
/// the full candidate set rather than streaming individual candidates.
///
/// Bridges the `onicegatheringstatechange` event into a `js_sys::Promise`
/// (the same "wrap a browser callback as an awaitable" shape every other
/// async call in this module already uses via `JsFuture::from(...)`, e.g.
/// `pc.create_offer()`) rather than hand-rolling a `Future`/`Waker` impl —
/// simpler and far less error-prone for a wasm-bindgen callback bridge. The
/// closure is held in an `Rc<RefCell<Option<_>>>` (not `.forget()`-ten) so
/// it — and the `onicegatheringstatechange` registration itself — are
/// cleared once the promise resolves, rather than leaking one closure per
/// connection setup.
async fn wait_for_ice_gathering_complete(pc: &RtcPeerConnection) -> Result<(), JsValue> {
    /// See [`wait_for_ice_gathering_complete`]'s doc comment.
    type IceChangeClosureCell = Rc<RefCell<Option<Closure<dyn FnMut()>>>>;

    if pc.ice_gathering_state() == RtcIceGatheringState::Complete {
        return Ok(());
    }
    let closure_cell: IceChangeClosureCell = Rc::new(RefCell::new(None));
    let closure_cell_for_promise = Rc::clone(&closure_cell);
    let pc_for_closure = pc.clone();
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let pc_inner = pc_for_closure.clone();
        let on_ice_change = Closure::<dyn FnMut()>::new(move || {
            if pc_inner.ice_gathering_state() == RtcIceGatheringState::Complete {
                let _ = resolve.call0(&JsValue::NULL);
            }
        });
        pc_for_closure.set_onicegatheringstatechange(Some(on_ice_change.as_ref().unchecked_ref()));
        *closure_cell_for_promise.borrow_mut() = Some(on_ice_change);
    });
    JsFuture::from(promise).await?;
    pc.set_onicegatheringstatechange(None);
    drop(closure_cell.borrow_mut().take());
    Ok(())
}
