# The control channel — native capabilities, called from Ruby

The bridge runs page-to-shell. That is the right shape on mobile, where Hotwire
Native has no server to talk to. A desktop app is different: its server runs in
the same process tree, so Ruby can have a channel of its own.

Without one, every native call has to be bounced off a page — and a background
job cannot raise a notification while no page is open, which is exactly when it
wants to.

```ruby
TurboDesktop::Native.notify(title: "Export finished", body: "invoice.pdf")
TurboDesktop::Native.call("window", "resize", width: 1200, height: 900)
text = TurboDesktop::Native.clipboard_read
```

## How it works

The shell opens a loopback listener on a port the OS picks, generates a token,
and hands both to the app server as **one line of JSON on its stdin**:

```json
{"protocol":"1.0","control":"http://127.0.0.1:52341","token":"…","header":"x-desktop-token"}
```

Ruby reads that line at boot (a Rails initializer in the engine) and posts bridge
messages to it:

```
POST /invoke
X-Desktop-Token: <token>
{"component":"notification","event":"show","data":{"title":"Done"}}
```

The shell routes it to the same component that serves the page. `bridge.rs` now
has a `dispatch` function for that: the command from the page checks the calling
origin, the control channel checks the token, and both end at the same place.

## Why stdin

The token travels on stdin rather than in the environment or on the command
line, so another process on the machine cannot read it out of `ps`. Neutralino
does the same thing for the same reason.

It also does double duty. The shell holds the pipe open for the child's
lifetime, so closing it is the signal to exit — and that is the only layer that
survives the shell being force-quit, since no Rust code runs then. Measured: a
backend watching stdin exits about 0.2s after EOF.

**Exactly one line is consumed.** The handshake reader takes its line and leaves
the rest of the stream alone, so the exit watchdog still works.

## Security

Three things, none of them optional.

- **Loopback only.** The listener binds `127.0.0.1` on a port the OS picks.
- **A real token.** 32 bytes from OS entropy, hex encoded. Deliberately *not*
  the `uuid_simple()` used for window labels, which is a nanosecond counter:
  fine for a label, guessable as a credential.
- **Constant-time comparison**, so a wrong token cannot be discovered a byte at
  a time.

Requests that are not `POST /invoke` are refused, `Content-Length` is bounded so
a malformed header cannot make the shell allocate without limit, and a
malformed body is rejected before it reaches a component.

## On the web, it is a no-op

The same Rails app serves the browser, where there is no shell and no handshake.
There `available?` is false and calls return `nil` rather than raising, so the
same code runs in both places without a guard at every call site.

A shell that should be there but is not answering raises
`TurboDesktop::Native::Error` with the address it tried, because that is a
genuine fault rather than a normal condition.

## Every component, not only the ones with sugar

`dispatch` is the whole bridge, so anything a page can ask for, Ruby can ask
for. The `window` component is the one that shows it: resize, minimize,
fullscreen, centre, always-on-top and state, all from a background job with no
page in sight.

```ruby
TurboDesktop::Native.call("window", "resize", width: 1200, height: 900)
# => {"status" => "ok", "width" => 1200.0, "height" => 900.0}
```

The rules belong to the app, not to the caller: a window declared
`"resizable": false` refuses whoever asks, and the configured `min_width` and
`min_height` win over a smaller request. So a resize can come back larger than
what was asked for, which is why the size actually applied is in the reply.

A refusal is a 500 with its reason, which Ruby raises as
`TurboDesktop::Native::CallFailed` rather than returning quietly.

## Tested

- **Rust**, 6 tests on the channel: token length and uniqueness, constant-time
  comparison, header parsing whatever the casing, a missing token parsing as
  empty rather than matching, a malformed `Content-Length` neither panicking nor
  allocating, and only `POST /invoke` routing to the dispatcher.
- **Rust**, 5 tests on the `window` component, driven with the message a Ruby
  call puts on the wire and a window from Tauri's mock runtime: a resize applied
  and reported back, the configured minimums binding a call from Ruby too, a
  refusal carrying its reason, a message with no `window_label` meaning the main
  window (Ruby never sends one), and an unknown event not passing as an ok.
- **Ruby**, 12 tests: unavailable without a handshake, calls as a no-op on the
  web, the handshake parsed, **exactly one line of stdin consumed**, a
  non-handshake line ignored so a developer running `rails server` by hand still
  boots, messages arriving with the right component and payload, reply values
  coming back, **a window resize reporting the size the shell applied**, **a
  refused capability raised rather than returned**, a wrong token refused, and a
  clear error when nothing answers.

## Measured end to end, by hand

The shell, the channel and the real Ruby client, in one process tree: a debug
build of the shell with a `server.command` that boots a Ruby script instead of
Rails — handshake off stdin, address on stdout, then `TurboDesktop::Native`.

```
available=true control=http://127.0.0.1:55221
too_small={"height":600.0,"status":"ok","width":800.0}
ordinary={"height":910.0,"status":"ok","width":1240.0}
state={"height":910.0,...,"label":"main","scaleFactor":2.0,"status":"ok","width":1240.0}
```

A window that really moved: `state` reports what the resize left behind, not
what the reply claimed. 100x100 came back as the configured 800x600 minimum.

## Not yet done

That run is not automated. The Rust listener is unit-tested at the parsing and
token layer and the components it dispatches to are tested on the mock runtime,
but nothing drives the socket end to end in CI: `start` is typed to the real
runtime's `AppHandle`, which a unit test cannot produce. The Ruby tests still run
against a stub that matches `control.rs` by hand, which would not catch the two
drifting apart.
