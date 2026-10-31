//! Whether the assistant may send anything at all.
//!
//! Context: the pane asks [`can_send`] before it starts a connection or sends a message, and
//! explains the answer in its own words (the catalog).
//! Rules: with the assistant switched off, with no connection, or with no way to send requests
//! (the browser build), no request leaves the app.
//! Do not: add a path that sends without asking this first.

/// Why nothing may be sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    /// The assistant is switched off in Settings.
    Disabled,
    /// This build has no way to send requests (the browser build).
    NoHttp,
    /// No connection has been added yet.
    NoConnection,
}

impl std::fmt::Display for Blocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Blocked::Disabled => "The assistant is switched off in Settings.",
            Blocked::NoHttp => "The assistant needs the desktop app.",
            Blocked::NoConnection => "Add a model connection in Settings to start.",
        })
    }
}

/// `Ok` only when the assistant is on, this build can send requests and a connection exists.
pub fn can_send(enabled: bool, has_connection: bool, host_has_http: bool) -> Result<(), Blocked> {
    if !enabled {
        Err(Blocked::Disabled)
    } else if !host_has_http {
        Err(Blocked::NoHttp)
    } else if !has_connection {
        Err(Blocked::NoConnection)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CountingHttp;

    #[test]
    fn no_request_leaves_the_app_until_a_connection_exists() {
        let http = CountingHttp::new(Ok((200, b"{}".to_vec())));
        let sender = http.http();
        for _ in 0..10 {
            // The pane only sends when the guard says so.
            if can_send(true, false, true).is_ok() {
                let _ = sender(&crate::HttpRequest {
                    method: "GET",
                    url: "http://localhost/".into(),
                    headers: vec![],
                    body: None,
                    timeout_s: 1,
                    stream_out: None,
                });
            }
            assert_eq!(can_send(true, false, true), Err(Blocked::NoConnection));
        }
        assert_eq!(http.calls(), 0);
        assert_eq!(can_send(false, true, true), Err(Blocked::Disabled));
        assert_eq!(can_send(true, true, false), Err(Blocked::NoHttp));
        assert!(can_send(true, true, true).is_ok());
    }
}
