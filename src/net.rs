//! UDP receiver thread and the message type shared with the UI.

use std::io;
use std::net::UdpSocket;
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::packet::{decode, DecodeError, Packet};

/// Events flowing from background threads to the application.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Packet { src: String, packet: Packet },
    Invalid { src: String, error: DecodeError },
    Status(String),
}

/// Turn a received datagram into a message.
pub fn classify(src: String, datagram: &[u8]) -> Msg {
    match decode(datagram) {
        Ok(packet) => Msg::Packet { src, packet },
        Err(error) => Msg::Invalid { src, error },
    }
}

/// Receive datagrams on `socket` until the channel closes.
pub fn spawn_receiver(socket: UdpSocket, tx: Sender<Msg>) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut buf = vec![0u8; 65_536];
        loop {
            match socket.recv_from(&mut buf) {
                Ok((n, from)) => {
                    if tx.send(classify(from.to_string(), &buf[..n])).is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => {
                    if tx.send(Msg::Status(format!("receive error: {e}"))).is_err() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(200));
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::{encode, Telemetry};

    #[test]
    fn classify_valid_and_invalid() {
        let bytes = encode(1, 2, &Telemetry::default());
        assert!(matches!(classify("a".into(), &bytes), Msg::Packet { .. }));
        let mut bad = bytes;
        bad[30] ^= 1;
        assert!(matches!(
            classify("a".into(), &bad),
            Msg::Invalid {
                error: DecodeError::BadCrc { .. },
                ..
            }
        ));
    }
}
