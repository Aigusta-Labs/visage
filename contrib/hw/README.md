# Hardware Quirks Database

Camera-specific UVC control bytes for IR emitter activation.

## Format

Each file is a TOML entry named `{vendor_id}-{product_id}.toml` (lowercase hex, no `0x` prefix):

```toml
[device]
vendor_id  = 0x04F2
product_id = 0xB6D9
name       = "ASUS Zenbook 14 UM3406HA IR Camera"

[emitter]
unit          = 14
selector      = 6
control_bytes = [1, 3, 3, 0, 0, 0, 0, 0, 0]
```

**Field reference:**

| Section | Field | Type | Description |
|---------|-------|------|-------------|
| `[device]` | `vendor_id` | hex int | USB idVendor (from `lsusb` or `visage discover`) |
| `[device]` | `product_id` | hex int | USB idProduct |
| `[device]` | `name` | string | Human-readable camera name |
| `[emitter]` | `unit` | u8 | UVC extension unit ID |
| `[emitter]` | `selector` | u8 | UVC control selector |
| `[emitter]` | `control_bytes` | byte array | Payload to activate the emitter. Zeros of the same length deactivate it. |
| `[emitter]` | `off_bytes` | byte array | Optional. Explicit payload to deactivate the emitter. Needed for cameras that reject an all-zero "off" payload (e.g. with `ERANGE`). Defaults to zeros of `control_bytes` length when omitted. |
| `[emitter]` | `reset_on_close` | bool | Optional. Set `true` for cameras that reset the control when the controlling fd closes and only re-illuminate on a fresh open→set edge; the emitter then holds one fd open for the duration of each capture. Defaults to `false`. |

The `control_bytes` values are found via `linux-enable-ir-emitter configure` or UVC descriptor analysis.

## Contributing

1. Run `visage discover` to detect your camera's VID:PID and check for existing quirk support
2. If no quirk exists, use `linux-enable-ir-emitter configure` to find the control bytes
3. Create a TOML file named `{vid}-{pid}.toml` (e.g. `04f2-b6d9.toml`) following the format above
4. **Register it in `crates/visage-hw/src/quirks.rs`** — two lines, and the step that is easy
   to miss:

   ```rust
   const QUIRK_04F2_B6D0: &str = include_str!("../../../contrib/hw/04f2-b6d0.toml");
   //  …then add it to QUIRK_SOURCES:
   ("04f2-b6d0.toml", QUIRK_04F2_B6D0),
   ```

5. Run `cargo test -p visage-hw quirks::` and submit a PR

Quirk files are embedded at compile time via `include_str!` — there is no runtime file loading,
so **dropping a `.toml` into this directory does nothing on its own.** A file that is not
registered in `QUIRK_SOURCES` is never read.

### Why step 4 has tests behind it

`quirk_db()` skips a malformed TOML rather than panicking — the daemon authenticates logins, and
one bad contributed quirk must not stop it starting. The cost is that at runtime a camera with a
broken quirk is **indistinguishable from a camera with no quirk**: no error, no crash, the
emitter simply never fires.

`crates/visage-hw/src/quirks.rs` therefore asserts, in CI:

| Test | Catches |
|---|---|
| `quirk_sources_all_parse` | an entry that is embedded but malformed, and was silently dropped |
| `every_quirk_is_reachable_by_lookup` | an entry that parses but `lookup_quirk` cannot find |
| `no_duplicate_vid_pid` | two entries claiming one device, where listing order silently decides |
| `emitter_payloads_are_coherent` | empty `control_bytes`, or `off_bytes` of a mismatched length |

The parse count is compared against `QUIRK_SOURCES.len()` rather than a hard-coded number, so
the assertion cannot drift as cameras are added.
