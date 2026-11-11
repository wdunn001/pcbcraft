# ADR 0002: Serial and USB provider traits for hardware in the loop testing

Status: Proposed. Issue #588 (roadmap id SIM-7-01). Depends on: SIM-5-01 (#1464).

## 1. Decision

PCBCraft exposes two traits, `SerialPort` and `UsbDevice`, that abstract physical hardware access for hardware in the loop (HITL) testing. Each trait has three implementations: a native implementation that uses OS-level APIs, a browser implementation that uses Web APIs, and an in-memory implementation for simulation and testing. When the platform-specific API is unavailable, the trait reports `Unavailable` with a plain reason string rather than panicking.

## 2. Traits

### SerialPort

The `SerialPort` trait provides serial communication with external devices such as microcontrollers and sensors.

Methods:
- `open(path: string, baud_rate: i32): Result<SerialPort, Error>` - Open a serial port at the given path and baud rate.
- `write(data: &[u8]): Result<usize, Error>` - Write bytes to the port.
- `read(buf: &mut [u8], timeout_ms: i32): Result<usize, Error>` - Read bytes with a timeout.
- `flush(): Result<(), Error>` - Flush the output buffer.
- `close(): Result<(), Error>` - Close the port.

### UsbDevice

The `UsbDevice` trait provides USB communication with external devices.

Methods:
- `open(vendor_id: u16, product_id: u16): Result<UsbDevice, Error>` - Open a USB device by vendor and product ID.
- `control_transfer(request_type: u8, request: u8, value: u16, index: u16, data: &[u8]): Result<Vec<u8>, Error>` - Perform a control transfer.
- `bulk_write(endpoint: u8, data: &[u8]): Result<usize, Error>` - Write to a bulk endpoint.
- `bulk_read(endpoint: u8, buf: &mut [u8]): Result<usize, Error>` - Read from a bulk endpoint.
- `close(): Result<(), Error>` - Close the device.

## 3. Implementations

| Trait | Native | Browser | In-memory |
| --- | --- | --- | --- |
| `SerialPort` | `crates/hitl/src/serial_native.rs` | `crates/hitl/src/serial_browser.rs` | `crates/hitl/src/serial_mem.rs` |
| `UsbDevice` | `crates/hitl/src/usb_native.rs` | `crates/hitl/src/usb_browser.rs` | `crates/hitl/src/usb_mem.rs` |

The in-memory implementations are used for simulation and testing, providing deterministic behaviour without requiring actual hardware.

## 4. Browser APIs

### WebSerial

The browser serial implementation uses the WebSerial API (`navigator.serial`).

Methods used:
- `requestPort()` - Show the port picker dialog.
- `getPorts()` - Get previously granted ports.
- `open(options)` - Open the port with configuration.
- `write(data)` - Write bytes.
- `read()` - Read bytes.
- `flush()` - Flush the output buffer.
- `close()` - Close the port.

### WebUSB

The browser USB implementation uses the WebUSB API (`navigator.usb`).

Methods used:
- `requestDevice(filters)` - Show the device picker dialog.
- `getDevices()` - Get previously granted devices.
- `open()` - Open the device.
- `controlTransferOut(setup, data)` - Perform a control transfer.
- `bulkTransferOut(endpoint, data)` - Write to a bulk endpoint.
- `bulkTransferIn(endpoint, length)` - Read from a bulk endpoint.
- `close()` - Close the device.

## 5. Browser support

| API | Chrome | Edge | Firefox | Safari |
| --- | --- | --- | --- | --- |
| WebSerial | 63+ | 79+ | Not supported | Not supported |
| WebUSB | 61+ | 79+ | Not supported | Not supported |

Both APIs are Chromium-only. Firefox and Safari do not support either API.

## 6. Fallback when API is missing

When the browser API is not available (e.g., Firefox, Safari, or older Chrome), the trait reports `Unavailable` with a plain reason string:

- WebSerial missing: `"WebSerial API not available (navigator.serial is undefined)"`
- WebUSB missing: `"WebUSB API not available (navigator.usb is undefined)"`

The application can then fall back to the in-memory implementation or prompt the user to use a supported browser.

## 7. Native dependency

The native serial implementation uses the `serialport` crate (version 4.6, MIT OR Apache-2.0). This is the only new native dependency for HITL support.

The native USB implementation uses the `libusb` crate, which is already a dependency of the project.

## 8. Plan

1. This page (#588).
2. Define the `SerialPort` and `UsbDevice` traits in `crates/hitl/src/lib.rs` (#589).
3. Implement the in-memory versions for testing (#590).
4. Implement the native versions using `serialport` and `libusb` (#591).
5. Implement the browser versions using WebSerial and WebUSB (#592).
6. Add engine commands for HITL testing (#593).

Sources: WebSerial API specification, WebUSB API specification, serialport crate documentation, libusb crate documentation.
