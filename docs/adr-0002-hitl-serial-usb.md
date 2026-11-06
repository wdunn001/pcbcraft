# ADR 0002: Serial and USB providers for hardware in the loop

Status: Proposed. The owner confirms or changes it in the #95 register (issue "#1490: SIM-7-01 decision on HITL transports"). Epic: #1364 (hardware in the loop). Depends on: #1464 (SIM-5-01, the in-circuit simulator).

## 1. Decision

Two provider traits cover the physical layer of hardware in the loop testing: `SerialPort` for byte streams and `UsbDevice` for USB transfers. Each trait has three implementations: a native one on the host operating system, a browser one that calls the Web API, and an in-memory one for tests. The only native dependency added is the `serialport` crate. The first release ships a native build only; the browser build is available when its APIs land in mainstream browsers.

## 3. Implementations

| Trait | Native | Browser | In-memory |
| --- | --- | --- | --- |
| `SerialPort` | `serialport` crate on Linux, Windows and macOS | WebSerial (`navigator.serial`) | Byte buffer with queued writes |
| `UsbDevice` | USB syscalls on Linux, Windows and macOS | WebUSB (`navigator.usb`) | Byte buffer with queued writes |

The browser implementation uses `wasm-bindgen` to call the Web API. Each call happens on the UI thread and uses an async method that the caller awaits. The in-memory implementation lives in the same file as the trait. It stores bytes in a bounded buffer and returns the last queued write when `read` is called.

## 4. Browser APIs

WebSerial exposes `navigator.serial` with `requestPort()`, `open(options)`, `readable` and `writable` streams. WebUSB exposes `navigator.usb` with `requestDevice()`, `claimInterface()`, `transferIn()` and `transferOut()`. Both APIs require a user gesture (a click) and a secure context (HTTPS or localhost). Neither API is available in every browser. The browser implementation calls `requestPort` or `requestDevice` on the first call that needs a device; subsequent calls use the same device until it is closed or the page is reloaded.
