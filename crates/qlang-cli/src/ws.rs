//! The network side of WebSockets (RFC 6455): the handshake, the frames, and the two threads
//! that serve one connection. The program never sees any of it: a worker receives messages from
//! a channel and sends what its callbacks queued to another.

use qlang_core::codec::{base64_encode, sha1};
use qlang_core::interp::{WsIn, WsOut};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

/// The biggest message accepted, in bytes (bigger: close code 1009).
pub const MAX_MESSAGE: usize = 1 << 20;
/// A ping is sent when nothing was written for this long.
const PING_EVERY: Duration = Duration::from_secs(30);
/// How many received messages wait for the worker before the reader stops reading (so a flood
/// slows the client down instead of filling the memory).
pub const BACKLOG: usize = 64;

/// What a connection tells its worker.
#[derive(Debug)]
pub enum WsEvent {
    Message(WsIn),
    /// The connection is over (the client left, an error, a close frame).
    Closed,
}

/// The worker's side of a connection.
pub struct WsLink {
    pub events: mpsc::Receiver<WsEvent>,
    pub out: mpsc::Sender<WsOut>,
}

/// The connection's side.
pub struct WsPipe {
    pub events: mpsc::SyncSender<WsEvent>,
    pub out: mpsc::Receiver<WsOut>,
}

pub fn pipe() -> (WsLink, WsPipe) {
    let (events_tx, events_rx) = mpsc::sync_channel(BACKLOG);
    let (out_tx, out_rx) = mpsc::channel();
    (
        WsLink {
            events: events_rx,
            out: out_tx,
        },
        WsPipe {
            events: events_tx,
            out: out_rx,
        },
    )
}

/// The `Sec-WebSocket-Accept` for a client key.
pub fn accept_key(key: &str) -> String {
    let joined = format!("{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    base64_encode(&sha1(joined.as_bytes()), false)
}

/// The client key when these headers (names in lower case) ask for a WebSocket, else `None`.
pub fn upgrade_key(headers: &[(String, String)]) -> Option<String> {
    let get = |n: &str| {
        headers
            .iter()
            .find(|(k, _)| k == n)
            .map(|(_, v)| v.as_str())
    };
    let has = |header: &str, token: &str| {
        get(header).is_some_and(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case(token)))
    };
    if !has("upgrade", "websocket") || !has("connection", "upgrade") {
        return None;
    }
    if get("sec-websocket-version")? != "13" {
        return None;
    }
    let key = get("sec-websocket-key")?.trim();
    // 16 random bytes in base64
    (key.len() == 24).then(|| key.to_string())
}

pub const OP_CONT: u8 = 0x0;
pub const OP_TEXT: u8 = 0x1;
pub const OP_BINARY: u8 = 0x2;
pub const OP_CLOSE: u8 = 0x8;
pub const OP_PING: u8 = 0x9;
pub const OP_PONG: u8 = 0xA;

/// One frame read from a client.
#[derive(Debug, PartialEq)]
pub struct Frame {
    pub fin: bool,
    pub opcode: u8,
    pub payload: Vec<u8>,
}

/// Why a frame could not be read.
#[derive(Debug, PartialEq)]
pub enum FrameError {
    /// The peer is gone (or the read failed).
    Gone,
    /// A rule of the protocol was broken: close with 1002.
    Protocol,
    /// Too big: close with 1009.
    TooBig,
}

pub fn read_frame(r: &mut impl Read) -> Result<Frame, FrameError> {
    let mut head = [0u8; 2];
    r.read_exact(&mut head).map_err(|_| FrameError::Gone)?;
    let fin = head[0] & 0x80 != 0;
    if head[0] & 0x70 != 0 {
        // no extension was agreed
        return Err(FrameError::Protocol);
    }
    let opcode = head[0] & 0x0F;
    // a client must mask what it sends
    if head[1] & 0x80 == 0 {
        return Err(FrameError::Protocol);
    }
    let mut len = u64::from(head[1] & 0x7F);
    if len == 126 {
        let mut b = [0u8; 2];
        r.read_exact(&mut b).map_err(|_| FrameError::Gone)?;
        len = u64::from(u16::from_be_bytes(b));
    } else if len == 127 {
        let mut b = [0u8; 8];
        r.read_exact(&mut b).map_err(|_| FrameError::Gone)?;
        len = u64::from_be_bytes(b);
    }
    let control = opcode >= 0x8;
    if control && (len > 125 || !fin) {
        return Err(FrameError::Protocol);
    }
    if len > MAX_MESSAGE as u64 {
        return Err(FrameError::TooBig);
    }
    let mut mask = [0u8; 4];
    r.read_exact(&mut mask).map_err(|_| FrameError::Gone)?;
    let mut payload = vec![0u8; len as usize];
    r.read_exact(&mut payload).map_err(|_| FrameError::Gone)?;
    for (i, b) in payload.iter_mut().enumerate() {
        *b ^= mask[i % 4];
    }
    Ok(Frame {
        fin,
        opcode,
        payload,
    })
}

/// A frame as a server writes it (never masked).
pub fn encode_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 10);
    out.push(0x80 | opcode);
    match payload.len() {
        n if n < 126 => out.push(n as u8),
        n if n <= 0xFFFF => {
            out.push(126);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            out.push(127);
            out.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    out.extend_from_slice(payload);
    out
}

fn close_payload(code: u16, reason: &str) -> Vec<u8> {
    let mut p = code.to_be_bytes().to_vec();
    // a control frame holds 125 bytes at most
    let mut end = reason.len().min(123);
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    p.extend_from_slice(&reason.as_bytes()[..end]);
    p
}

type Writer = Arc<Mutex<TcpStream>>;

fn write_frame(w: &Writer, opcode: u8, payload: &[u8]) -> bool {
    let Ok(mut s) = w.lock() else { return false };
    s.write_all(&encode_frame(opcode, payload)).is_ok() && s.flush().is_ok()
}

/// Reads the messages of a client and gives them to the worker, answering pings itself. Ends
/// when the connection does.
fn read_loop(mut stream: TcpStream, writer: Writer, events: mpsc::SyncSender<WsEvent>) {
    let mut assembling: Option<(u8, Vec<u8>)> = None;
    let fail = |code: u16, reason: &str| {
        write_frame(&writer, OP_CLOSE, &close_payload(code, reason));
    };
    loop {
        let frame = match read_frame(&mut stream) {
            Ok(f) => f,
            Err(FrameError::Gone) => break,
            Err(FrameError::Protocol) => {
                fail(1002, "protocol error");
                break;
            }
            Err(FrameError::TooBig) => {
                fail(1009, "message too big");
                break;
            }
        };
        match frame.opcode {
            OP_PING => {
                if !write_frame(&writer, OP_PONG, &frame.payload) {
                    break;
                }
            }
            OP_PONG => {}
            OP_CLOSE => {
                // answer with the same code, then the connection ends
                let echo: Vec<u8> = frame.payload.iter().take(2).copied().collect();
                write_frame(&writer, OP_CLOSE, &echo);
                break;
            }
            OP_TEXT | OP_BINARY | OP_CONT => {
                let (kind, mut data) = match (frame.opcode, assembling.take()) {
                    (OP_CONT, Some(a)) => a,
                    (OP_CONT, None) => {
                        fail(1002, "unexpected continuation");
                        break;
                    }
                    (op, None) => (op, Vec::new()),
                    (_, Some(_)) => {
                        fail(1002, "a message was not finished");
                        break;
                    }
                };
                data.extend_from_slice(&frame.payload);
                if data.len() > MAX_MESSAGE {
                    fail(1009, "message too big");
                    break;
                }
                if !frame.fin {
                    assembling = Some((kind, data));
                    continue;
                }
                let message = if kind == OP_TEXT {
                    match String::from_utf8(data) {
                        Ok(t) => WsIn::Text(t),
                        Err(_) => {
                            fail(1007, "invalid text");
                            break;
                        }
                    }
                } else {
                    WsIn::Binary(data)
                };
                if events.send(WsEvent::Message(message)).is_err() {
                    break;
                }
            }
            _ => {
                fail(1002, "unknown opcode");
                break;
            }
        }
    }
    let _ = events.send(WsEvent::Closed);
}

/// Serves a connection whose handshake was written: a thread reads, this one writes what the
/// worker sends, with a ping from time to time. Returns when the connection is over.
pub fn run_session(stream: TcpStream, pipe: WsPipe) {
    let _ = stream.set_read_timeout(None);
    let _ = stream.set_nodelay(true);
    let (Ok(read_half), Ok(write_half)) = (stream.try_clone(), stream.try_clone()) else {
        return;
    };
    let writer: Writer = Arc::new(Mutex::new(write_half));
    let reader = {
        let writer = writer.clone();
        let events = pipe.events;
        std::thread::spawn(move || read_loop(read_half, writer, events))
    };
    loop {
        match pipe.out.recv_timeout(PING_EVERY) {
            Ok(WsOut::Text(t)) => {
                if !write_frame(&writer, OP_TEXT, t.as_bytes()) {
                    break;
                }
            }
            Ok(WsOut::Binary(b)) => {
                if !write_frame(&writer, OP_BINARY, &b) {
                    break;
                }
            }
            Ok(WsOut::Close(code, reason)) => {
                write_frame(&writer, OP_CLOSE, &close_payload(code, &reason));
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !write_frame(&writer, OP_PING, b"") {
                    break;
                }
            }
            // the worker let go of the connection
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
    let _ = reader.join();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn masked(opcode: u8, fin: bool, payload: &[u8]) -> Vec<u8> {
        let mask = [0x12, 0x34, 0x56, 0x78];
        let mut out = vec![if fin { 0x80 } else { 0 } | opcode];
        if payload.len() < 126 {
            out.push(0x80 | payload.len() as u8);
        } else {
            out.push(0x80 | 126);
            out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        out.extend_from_slice(&mask);
        out.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        out
    }

    #[test]
    fn the_accept_key_of_the_rfc() {
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn a_masked_frame_is_read() {
        let bytes = masked(OP_TEXT, true, b"hello");
        let f = read_frame(&mut &bytes[..]).unwrap();
        assert_eq!(
            f,
            Frame {
                fin: true,
                opcode: OP_TEXT,
                payload: b"hello".to_vec()
            }
        );
        let long = vec![b'x'; 300];
        let bytes = masked(OP_BINARY, true, &long);
        assert_eq!(read_frame(&mut &bytes[..]).unwrap().payload, long);
    }

    #[test]
    fn broken_frames_are_refused() {
        // not masked
        let plain = encode_frame(OP_TEXT, b"hi");
        assert_eq!(read_frame(&mut &plain[..]), Err(FrameError::Protocol));
        // a control frame that is fragmented
        let bytes = masked(OP_PING, false, b"");
        assert_eq!(read_frame(&mut &bytes[..]), Err(FrameError::Protocol));
        // reserved bits
        let mut bytes = masked(OP_TEXT, true, b"x");
        bytes[0] |= 0x40;
        assert_eq!(read_frame(&mut &bytes[..]), Err(FrameError::Protocol));
        // too big
        let mut big = vec![0x81, 0x80 | 127];
        big.extend_from_slice(&(MAX_MESSAGE as u64 + 1).to_be_bytes());
        assert_eq!(read_frame(&mut &big[..]), Err(FrameError::TooBig));
        // cut short
        let cut = &masked(OP_TEXT, true, b"hello")[..4];
        assert_eq!(read_frame(&mut &cut[..]), Err(FrameError::Gone));
    }

    #[test]
    fn server_frames_have_the_right_length_form() {
        assert_eq!(encode_frame(OP_TEXT, b"hi"), vec![0x81, 2, b'h', b'i']);
        let medium = encode_frame(OP_BINARY, &[0; 200]);
        assert_eq!(&medium[..4], &[0x82, 126, 0, 200]);
        let long = encode_frame(OP_BINARY, &vec![0; 70000]);
        assert_eq!(&long[..2], &[0x82, 127]);
        assert_eq!(u64::from_be_bytes(long[2..10].try_into().unwrap()), 70000);
    }

    #[test]
    fn only_a_complete_upgrade_is_accepted() {
        let h = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        let good = h(&[
            ("upgrade", "WebSocket"),
            ("connection", "keep-alive, Upgrade"),
            ("sec-websocket-version", "13"),
            ("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="),
        ]);
        assert!(upgrade_key(&good).is_some());
        let old = h(&[
            ("upgrade", "websocket"),
            ("connection", "upgrade"),
            ("sec-websocket-version", "8"),
            ("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="),
        ]);
        assert!(upgrade_key(&old).is_none());
        assert!(upgrade_key(&h(&[("upgrade", "h2c")])).is_none());
    }

    #[test]
    fn a_close_reason_is_cut_on_a_character() {
        let p = close_payload(1000, &"é".repeat(100));
        assert!(p.len() <= 125);
        assert!(std::str::from_utf8(&p[2..]).is_ok());
    }
}
