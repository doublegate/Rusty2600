#!/usr/bin/env python3
"""One-off CDP script proving WebRtcSocket/WebRtcPeer's connection
establishment + data-channel send/receive works for real, in a real
Chromium instance, across two independent RTCPeerConnections (one per
CDP-opened tab). NOT part of the shipped crate -- verification tooling
only, run once and its output captured in the fork's final report."""
import json
import sys
import urllib.request
import asyncio
import websockets

CDP_HTTP = "http://localhost:9444"
PAGE_URL = "http://localhost:8793/"


def new_tab():
    req = urllib.request.Request(f"{CDP_HTTP}/json/new?{PAGE_URL}", method="PUT")
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())


class Tab:
    def __init__(self, ws_url):
        self.ws_url = ws_url
        self.ws = None
        self.msg_id = 0
        self.console = []

    async def connect(self):
        self.ws = await websockets.connect(self.ws_url, max_size=None)
        await self.send("Runtime.enable")
        await self.send("Console.enable")

    async def send(self, method, params=None):
        # A single reader loop per Tab (this method IS that loop, called
        # sequentially -- never concurrently from two places, which is what
        # a separate background listener task would have caused: the
        # `websockets` library forbids two concurrent `recv()` calls on one
        # connection) dispatches events (console/exception) into `self.console`
        # and returns once it sees the matching response id.
        self.msg_id += 1
        mid = self.msg_id
        await self.ws.send(json.dumps({"id": mid, "method": method, "params": params or {}}))
        while True:
            raw = await self.ws.recv()
            msg = json.loads(raw)
            if msg.get("id") == mid:
                return msg
            if msg.get("method") == "Runtime.consoleAPICalled":
                args = msg["params"].get("args", [])
                text = " ".join(str(a.get("value", a.get("description", ""))) for a in args)
                self.console.append(text)
            elif msg.get("method") == "Runtime.exceptionThrown":
                exc = msg["params"]["exceptionDetails"]
                self.console.append(
                    f"[EXCEPTION] {exc.get('text')} {exc.get('exception', {}).get('description', '')}"
                )

    async def eval_await(self, expr, timeout=10.0):
        result = await asyncio.wait_for(
            self.send("Runtime.evaluate", {
                "expression": expr,
                "awaitPromise": True,
                "returnByValue": True,
            }),
            timeout=timeout,
        )
        r = result.get("result", {})
        if "exceptionDetails" in r:
            return {"error": r["exceptionDetails"]}
        return r.get("result", {}).get("value")


async def main():
    host_info = new_tab()
    joiner_info = new_tab()
    host = Tab(host_info["webSocketDebuggerUrl"])
    joiner = Tab(joiner_info["webSocketDebuggerUrl"])
    await host.connect()
    await joiner.connect()

    # Wait for the wasm module to init on both tabs.
    await asyncio.sleep(3)

    print("=== host: create offer ===")
    # Read `.offer` BEFORE calling the consuming `.into_peer()` -- once
    # `into_peer()` runs, wasm-bindgen frees the Rust-side value behind
    # `window.__result`, and any further property access on it throws
    # "null pointer passed to rust" (a real, expected wasm-bindgen
    # move-semantics guard, not a bug in the Rust API itself).
    r = await host.eval_await("""
        (async () => {
          window.__result = await window.wasmBindings.WebRtcPeer.createOffer();
          window.__offerSdp = window.__result.offer;
          window.__hostPeer = window.__result.into_peer();
          return window.__offerSdp.length;
        })()
    """)
    print("offer length:", r)

    offer_sdp = await host.eval_await("window.__offerSdp")
    print("offer starts with:", (offer_sdp or "")[:40])

    print("=== joiner: create answer ===")
    escaped = json.dumps(offer_sdp)
    r = await joiner.eval_await(f"""
        (async () => {{
          window.__answerResult = await window.wasmBindings.WebRtcPeer.createAnswer({escaped});
          return window.__answerResult.answer.length;
        }})()
    """)
    print("answer length:", r)

    answer_sdp = await joiner.eval_await("window.__answerResult.answer")
    print("answer starts with:", (answer_sdp or "")[:40])

    print("=== host: accept answer ===")
    escaped_answer = json.dumps(answer_sdp)
    r = await host.eval_await(f"""
        (async () => {{
          await window.__hostPeer.acceptAnswer({escaped_answer});
          return "ok";
        }})()
    """)
    print("accept_answer:", r)

    print("=== waiting for channels to open ===")
    host_open = False
    joiner_open = False
    for i in range(20):
        await asyncio.sleep(0.5)
        host_ice = await host.eval_await("window.__hostPeer.iceConnectionState()")
        if not host_open:
            host_open = await host.eval_await("window.__hostPeer.isChannelOpen()")
        if not joiner_open:
            # Try to materialize the joiner peer once its channel arrives.
            await joiner.eval_await("""
                (() => {
                  if (!window.__joinerPeer && window.__answerResult.channelReady()) {
                    window.__joinerPeer = window.__answerResult.tryIntoPeer();
                  }
                  return !!window.__joinerPeer;
                })()
            """)
            has_peer = await joiner.eval_await("!!window.__joinerPeer")
            if has_peer:
                joiner_open = await joiner.eval_await("window.__joinerPeer.isChannelOpen()")
        joiner_ice = (
            await joiner.eval_await("window.__joinerPeer ? window.__joinerPeer.iceConnectionState() : 'no-peer-yet'")
        )
        print(f"  tick {i}: host_open={host_open} host_ice={host_ice} joiner_open={joiner_open} joiner_ice={joiner_ice}")
        if host_open and joiner_open:
            break

    if not (host_open and joiner_open):
        print("FAIL: channels did not both open in time")
        sys.exit(1)

    print("=== channels open on both sides — sending real bytes both directions ===")
    await host.eval_await("""
        (() => {
          window.__hostChannel = window.__hostPeer.into_channel();
          window.__hostReceived = [];
          window.__hostChannel.onmessage = (e) => window.__hostReceived.push(e.data);
          return true;
        })()
    """)
    await joiner.eval_await("""
        (() => {
          window.__joinerChannel = window.__joinerPeer.into_channel();
          window.__joinerReceived = [];
          window.__joinerChannel.onmessage = (e) => window.__joinerReceived.push(e.data);
          return true;
        })()
    """)

    await host.eval_await("window.__hostChannel.send('hello from host'); true")
    await joiner.eval_await("window.__joinerChannel.send('hello from joiner'); true")
    await asyncio.sleep(1.0)

    host_received = await host.eval_await("window.__hostReceived")
    joiner_received = await joiner.eval_await("window.__joinerReceived")
    print("host received:", host_received)
    print("joiner received:", joiner_received)

    ok = (
        host_received == ["hello from joiner"]
        and joiner_received == ["hello from host"]
    )
    print("RESULT:", "PASS - real bidirectional data channel bytes confirmed" if ok else "FAIL")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    asyncio.run(main())
