# Known Issue: WebKitGTK UI-process segfault on heavy-site navigation

**Status:** upstream (libwebkit2gtk-4.1.so.0.21.10) · **Aileron impact:** contained
**First observed:** 2026-10-26 · **Reproductions:** 2 (GitHub first-load, YouTube→Reddit transition)
**Non-reproduction:** same sites load fine on fresh instances and on retry

## Evidence package

- Kernel: `segfault at 48 ip 00007f20c4af27ce sp ... error 4 in
  libwebkit2gtk-4.1.so.0.21.10[1cf17ce,7f20c4125000+42dd000]` — same
  instruction pointer on both reproductions.
- Victim context: WebKitGTK 2.0.2 API crate (WebKitGTK 4.1), CachyOS,
  Wayland, NVIDIA (DMA-BUF disabled by aileron → shared-GL fallback),
  debug build of aileron (wgpu validation layer active).
- Aileron-side contributors ruled out: the snapshot backpressure, epoch
  gating, and compositing gate fixes (commits 57a8dc0, 4689f3b, 302b59e)
  removed the memory-growth and starvation factors; the segfault is a
  native-library crash in the UI process during heavy-page load.

## Aileron mitigation (shipped)

- `aileron.service` (Restart=on-failure, 5s) + `aileron-watchdog.timer`
  (5min MCP-handshake health check, 2-strike restart): a crash costs ~5s
  of bridge availability, then the session restores.
- Workspace auto-save (30s) restores panes on relaunch.

## Recommended user mitigations

- Keep webkit2gtk updated; retest on the next package bump.
- On NVIDIA, keep aileron's DMA-BUF disable (bootstrap does this
  automatically).

## Upstream report checklist (WebKit bugzilla / GitHub WebKit/WebKit)

- [ ] Attach the coredump (`coredumpctl` entry present, SIGSEGV)
- [ ] Reproduce outside aileron: plain WebKitGTK viewer loading
      github.com under shared-GL fallback after a heavy-page transition
- [ ] Include `ldd` of libwebkit2gtk and the NVIDIA driver version
