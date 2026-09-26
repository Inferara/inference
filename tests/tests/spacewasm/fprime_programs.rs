//! The sources of F´ (F Prime) programs the SpaceWasm tier's F´ rows run and
//! pin against the reference hosts, in a module of their own so that a binary
//! other than this tier's can include them by path and run the same text.

/// A program calling all six reference hosts and ending in `panic`:
/// `command`'s answer is `telemetry`'s id, and `rsleep` sleeps for the clock
/// reading pushed past `u32::MAX`.
pub const EVERY_CALL: &str = r"external fn panic(text: [u8; 4], len: i32, line: i32);
external fn rsleep(ticks: i64);
external fn command(opcode: i32, arg: i32) -> i32;
external fn message(text: [u8; 2], len: i32);
external fn telemetry(id: i32, mut time: [u8; 11], time_len: i32, value: [u8; 4], value_len: i32) -> i32;
use { panic, rsleep, command, message, telemetry } from host::fprime_core;

external fn clock_ms() -> i64;
use { clock_ms } from host::env;

pub fn go() -> i64 {
    let hi: [u8; 2] = [104, 105];
    message(hi, 2);
    let id: i32 = command(42, 7);
    let mut time: [u8; 11] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let value: [u8; 4] = [21, 0, 0, 0];
    let status: i32 = telemetry(id, time, 11, value, 4);
    let now: i64 = clock_ms();
    rsleep(now + 5000000000);
    let boom: [u8; 4] = [98, 111, 111, 109];
    panic(boom, 4, 17);
    return now;
}";

/// A downlink whose time the caller seeded with the bytes 1 to 11, answering
/// the sum of those bytes after the call: 66 if the host wrote nothing, 0 if
/// it wrote the whole time, and 255 if it refused the downlink.
pub const DOWNLINK: &str = r"external fn telemetry(id: i32, mut time: [u8; 11], time_len: i32, value: [u8; 4], value_len: i32) -> i32;
use { telemetry } from host::fprime_core;

pub fn downlink(time_len: i32) -> u8 {
    let mut time: [u8; 11] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    let value: [u8; 4] = [21, 0, 0, 0];
    let status: i32 = telemetry(3, time, time_len, value, 4);
    if status != 0 {
        return 255;
    }
    return time[0] + time[1] + time[2] + time[3] + time[4] + time[5] + time[6] + time[7]
        + time[8] + time[9] + time[10];
}";

/// A program that sends a command and then counts to `to`, answering the
/// count plus the command's answer: a loop whose cost follows its argument.
pub const SPIN: &str = r"external fn command(opcode: i32, arg: i32) -> i32;
use { command } from host::fprime_core;

pub fn spin(to: i32) -> i32 {
    let id: i32 = command(42, 7);
    let mut i: i32 = 0;
    loop i < to {
        i = i + 1;
    }
    return i + id;
}";
