// CIRCE - Vendored from https://crates.io/crates/circe
// LICENSE - unlicense https://choosealicense.com/licenses/unlicense/
// Applies to commands.rs TOO
//! A simple IRC crate written in rust
//! ```no_run
//! use circe::{commands::Command, Client, Config};
//! fn main() -> Result<(), std::io::Error> {
//!     let config = Default::default();
//!     let mut client = Client::new(config)?;
//!     client.identify()?;
//!
//!     loop {
//!         if let Ok(ref command) = client.read() {
//!             if let Command::OTHER(line) = command {
//!                 print!("{}", line);
//!             }
//!             if let Command::PRIVMSG(nick, channel, message) = command {
//!                println!("PRIVMSG received from {}: {} {}", nick, channel, message);
//!             }
//!         }
//!         # break;
//!     }
//!     # Ok(())
//! }

#![warn(missing_docs)]
#![allow(clippy::too_many_lines)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![allow(dead_code)]

use color_eyre::{eyre::anyhow, Result};
use rustls::pki_types::ServerName;
use rustls::ClientConnection;
use rustls::StreamOwned;
use std::borrow::Cow;
use std::io::{Error, Read, Write};
use std::net::Shutdown;
use std::net::TcpStream;
use std::sync::Arc;

/// IRC comamnds
pub mod commands;

/// An IRC client
pub struct Client {
    config: Config,
    stream: StreamOwned<ClientConnection, TcpStream>,
}

/// Config for the IRC client
#[derive(Clone, Default)]
pub struct Config {
    pub channels: Vec<String>,
    pub host: String,
    pub mode: Option<String>,
    pub nickname: Option<String>,
    pub port: u16,
    pub username: String,
}

/// Custom Error for the `read` function
#[derive(Debug)]
pub struct NoNewLines;

impl std::fmt::Display for NoNewLines {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Now new lines from the stream.")
    }
}

impl std::error::Error for NoNewLines {}

impl Client {
    /// Creates a new client with a given [`Config`].
    /// ```no_run
    /// # use circe::*;
    /// # let config = Default::default();
    /// let mut client = Client::new(config)?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not connect to the host.
    /// # Panics
    /// Panics if the client can't connect to the given host.
    pub fn new(config: Config) -> Result<Self> {
        let mut roots = rustls::RootCertStore::empty();
        for cert in rustls_native_certs::load_native_certs().expect("could not load platform certs")
        {
            roots.add(cert).unwrap();
        }

        let tls_config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();

        // Use default certificate roots.

        let dns_name = ServerName::try_from(config.host.clone())?;

        let tcp_stream = TcpStream::connect(format!("{}:{}", config.host, config.port))?;
        let client = ClientConnection::new(Arc::new(tls_config), dns_name)?;

        let stream = StreamOwned::new(client, tcp_stream);

        Ok(Self { config, stream })
    }

    /// Identify user and joins the in the [`Config`] specified channels.
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Default::default())?;
    /// client.identify()?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not write to the stream.
    pub fn identify(&mut self) -> Result<(), Error> {
        self.write_command(commands::Command::CAP(commands::CapMode::LS))?;
        self.write_command(commands::Command::CAP(commands::CapMode::END))?;

        self.write_command(commands::Command::USER(
            self.config.username.clone(),
            "*".into(),
            "*".into(),
            self.config.username.clone(),
        ))?;

        if let Some(nick) = self.config.nickname.clone() {
            self.write_command(commands::Command::NICK(nick))?;
        } else {
            self.write_command(commands::Command::NICK(self.config.username.clone()))?;
        }

        loop {
            if let Ok(ref command) = self.read() {
                match command {
                    commands::Command::PING(code) => {
                        self.write_command(commands::Command::PONG(code.to_string()))?;
                    }
                    commands::Command::OTHER(line) => {
                        if line.contains("001") {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }

        let config = self.config.clone();
        self.write_command(commands::Command::MODE(config.username, config.mode))?;
        for channel in &config.channels {
            self.write_command(commands::Command::JOIN(channel.to_string()))?;
        }

        Ok(())
    }

    // edit by me - i am pretty sure that original circe creator tested this and it doesn't matter
    #[allow(clippy::unused_io_amount)]
    fn read_string(&mut self) -> Option<String> {
        let mut buffer = [0u8; 512];

        match self.stream.read(&mut buffer) {
            Ok(_) => {}
            Err(_) => return None,
        };

        let res = String::from_utf8_lossy(&buffer);

        // The trimming is required because if the message is less than 512 bytes it will be
        // padded with a bunch of 0u8 because of the pre-allocated buffer
        Some(res.trim().trim_matches(char::from(0)).trim().into())
    }

    /// Read data coming from the IRC as a [`commands::Command`].
    /// ```no_run
    /// # use circe::*;
    /// # use circe::commands::Command;
    /// # fn main() -> Result<(), std::io::Error> {
    /// # let config = Default::default();
    /// # let mut client = Client::new(config)?;
    /// if let Ok(ref command) = client.read() {
    ///     if let Command::OTHER(line) = command {
    ///         print!("{}", line);
    ///     }
    /// }
    /// # Ok::<(), std::io::Error>(())
    /// # }
    /// ```
    /// # Errors
    /// Returns error if there are no new messages. This should not be taken as an actual error, because nothing went wrong.
    pub fn read(&mut self) -> Result<commands::Command, NoNewLines> {
        if let Some(string) = self.read_string() {
            let command = commands::Command::command_from_str(&string);

            if let commands::Command::PONG(command) = command {
                if let Err(_e) = self.write_command(commands::Command::PONG(command)) {
                    return Err(NoNewLines);
                }
                return Ok(commands::Command::PONG("".to_string()));
            }

            return Ok(command);
        }

        Err(NoNewLines)
    }

    pub fn write(&mut self, data: &str) -> Result<(), Error> {
        let formatted = {
            let new = format!("{}\r\n", data);
            Cow::Owned(new) as Cow<str>
        };

        self.stream.write_all(formatted.as_bytes())?;

        Ok(())
    }

    /// Send a [`commands::Command`] to the IRC.<br>
    /// Not reccomended to use, use the helper functions instead.
    /// ```no_run
    /// # use circe::*;
    /// # use circe::commands::Command;
    /// # let mut client = Client::new(Default::default())?;
    /// client.write_command(Command::PRIVMSG("".to_string(), "#main".to_string(), "Hello".to_string()))?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not write to the stream.
    pub fn write_command(&mut self, command: commands::Command) -> Result<(), Error> {
        use commands::Command::{
            ADMIN, AWAY, CAP, INVITE, JOIN, LIST, MODE, NAMES, NICK, OPER, OTHER, PART, PASS, PING,
            PONG, PRIVMSG, QUIT, TOPIC, USER,
        };
        let computed = match command {
            ADMIN(target) => {
                let formatted = format!("ADMIN {}", target);
                Cow::Owned(formatted) as Cow<str>
            }
            AWAY(message) => {
                let formatted = format!("AWAY {}", message);
                Cow::Owned(formatted) as Cow<str>
            }
            CAP(mode) => {
                use commands::CapMode::{END, LS};
                Cow::Borrowed(match mode {
                    LS => "CAP LS 302",
                    END => "CAP END",
                }) as Cow<str>
            }
            INVITE(username, channel) => {
                let formatted = format!("INVITE {} {}", username, channel);
                Cow::Owned(formatted) as Cow<str>
            }
            JOIN(channel) => {
                let formatted = format!("JOIN {}", channel);
                Cow::Owned(formatted) as Cow<str>
            }
            LIST(channel, server) => {
                let mut formatted = "LIST".to_string();
                if let Some(channel) = channel {
                    formatted.push_str(format!(" {}", channel).as_str());
                }
                if let Some(server) = server {
                    formatted.push_str(format!(" {}", server).as_str());
                }
                Cow::Owned(formatted) as Cow<str>
            }
            NAMES(channel, server) => {
                let formatted = {
                    if let Some(server) = server {
                        format!("NAMES {} {}", channel, server)
                    } else {
                        format!("NAMES {}", channel)
                    }
                };
                Cow::Owned(formatted) as Cow<str>
            }
            NICK(nickname) => {
                let formatted = format!("NICK {}", nickname);
                Cow::Owned(formatted) as Cow<str>
            }
            MODE(target, mode) => {
                let formatted = {
                    if let Some(mode) = mode {
                        format!("MODE {} {}", target, mode)
                    } else {
                        format!("MODE {}", target)
                    }
                };
                Cow::Owned(formatted) as Cow<str>
            }
            OPER(nick, password) => {
                let formatted = format!("OPER {} {}", nick, password);
                Cow::Owned(formatted) as Cow<str>
            }
            OTHER(_) => {
                return Err(Error::new(
                    std::io::ErrorKind::Other,
                    "Cannot write commands of type OTHER",
                ));
            }
            PART(target) => {
                let formatted = format!("PART {}", target);
                Cow::Owned(formatted) as Cow<str>
            }
            PASS(password) => {
                let formatted = format!("PASS {}", password);
                Cow::Owned(formatted) as Cow<str>
            }
            PING(target) => {
                let formatted = format!("PING {}", target);
                Cow::Owned(formatted) as Cow<str>
            }
            PONG(code) => {
                let formatted = format!("PONG {}", code);
                Cow::Owned(formatted) as Cow<str>
            }
            PRIVMSG(_, target, message) => {
                let formatted = format!("PRIVMSG {} {}", target, message);
                Cow::Owned(formatted) as Cow<str>
            }
            QUIT(message) => {
                let formatted = format!("QUIT :{}", message);
                Cow::Owned(formatted) as Cow<str>
            }
            TOPIC(channel, topic) => {
                let formatted = {
                    if let Some(topic) = topic {
                        format!("TOPIC {} :{}", channel, topic)
                    } else {
                        format!("TOPIC {}", channel)
                    }
                };
                Cow::Owned(formatted) as Cow<str>
            }
            USER(username, s1, s2, realname) => {
                let formatted = format!("USER {} {} {} :{}", username, s1, s2, realname);
                Cow::Owned(formatted) as Cow<str>
            }
        };

        self.write(&computed)?;
        Ok(())
    }

    // Helper commands

    /// Helper function for requesting information about the ADMIN of an IRC server
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Default::default())?;
    /// client.admin("192.168.178.100")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not write to the stream.
    pub fn admin(&mut self, target: &str) -> Result<(), Error> {
        self.write_command(commands::Command::ADMIN(target.to_string()))?;
        Ok(())
    }

    /// Helper function for setting the users status to AWAY
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Default::default())?;
    /// client.away("AFK")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not write to the stream.
    pub fn away(&mut self, message: &str) -> Result<(), Error> {
        self.write_command(commands::Command::AWAY(message.to_string()))?;
        Ok(())
    }

    /// Helper function for sending PRIVMSGs.
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Default::default())?;
    /// client.privmsg("#main", "Hello")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not write to the stream.
    pub fn privmsg(&mut self, channel: &str, message: &str) -> Result<(), Error> {
        self.write_command(commands::Command::PRIVMSG(
            String::from(""),
            channel.to_string(),
            message.to_string(),
        ))?;
        Ok(())
    }

    /// Helper function to INVITE people to a channels
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Default::default())?;
    /// client.invite("liblirc", "#circe")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not write to the stream.
    pub fn invite(&mut self, username: &str, channel: &str) -> Result<(), Error> {
        self.write_command(commands::Command::INVITE(
            username.to_string(),
            channel.to_string(),
        ))?;
        Ok(())
    }

    /// Helper function for sending JOINs.
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Default::default())?;
    /// client.join("#main")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not write to the stream.
    pub fn join(&mut self, channel: &str) -> Result<(), Error> {
        self.write_command(commands::Command::JOIN(channel.to_string()))?;
        Ok(())
    }

    /// Helper function for ``LISTing`` channels and modes
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Default::default())?;
    /// client.list(None, None)?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not write to the stream.
    pub fn list(&mut self, channel: Option<&str>, server: Option<&str>) -> Result<(), Error> {
        let channel_config = channel.map(std::string::ToString::to_string);
        let server_config = server.map(std::string::ToString::to_string);
        self.write_command(commands::Command::LIST(channel_config, server_config))?;
        Ok(())
    }

    /// Helper function for getting all nicknames in a channel
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Default::default())?;
    /// client.names("#main,#circe", None)?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not write to the stream.
    pub fn names(&mut self, channel: &str, server: Option<&str>) -> Result<(), Error> {
        if let Some(server) = server {
            self.write_command(commands::Command::NAMES(
                channel.to_string(),
                Some(server.to_string()),
            ))?;
        } else {
            self.write_command(commands::Command::NAMES(channel.to_string(), None))?;
        }
        Ok(())
    }

    /// Helper function to try to register as a channel operator
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Default::default())?;
    /// client.oper("username", "password")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not write to the stream.
    pub fn oper(&mut self, username: &str, password: &str) -> Result<(), Error> {
        self.write_command(commands::Command::OPER(
            username.to_string(),
            password.to_string(),
        ))?;
        Ok(())
    }

    /// Helper function for sending MODEs.
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Default::default())?;
    /// client.mode("test", Some("+B"))?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not write to the stream.
    pub fn mode(&mut self, target: &str, mode: Option<&str>) -> Result<(), Error> {
        if let Some(mode) = mode {
            self.write_command(commands::Command::MODE(
                target.to_string(),
                Some(mode.to_string()),
            ))?;
        } else {
            self.write_command(commands::Command::MODE(target.to_string(), None))?;
        }
        Ok(())
    }

    /// Helper function for leaving channels.
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Default::default())?;
    /// client.part("#main")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not write to the stream.
    pub fn part(&mut self, target: &str) -> Result<(), Error> {
        self.write_command(commands::Command::PART(target.to_string()))?;
        Ok(())
    }

    /// Helper function for setting or getting the topic of a channel
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Default::default())?;
    /// client.topic("#main", Some("main channel"))?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not write to the stream.
    pub fn topic(&mut self, channel: &str, topic: Option<&str>) -> Result<(), Error> {
        if let Some(topic) = topic {
            self.write_command(commands::Command::TOPIC(
                channel.to_string(),
                Some(topic.to_string()),
            ))?;
        } else {
            self.write_command(commands::Command::TOPIC(channel.to_string(), None))?;
        }
        Ok(())
    }

    /// Helper function for leaving the IRC server and shutting down the TCP stream afterwards.
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Default::default())?;
    /// client.quit(None)?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// # Errors
    /// Returns error if the client could not write to the stream.
    pub fn quit(&mut self, message: Option<&str>) -> Result<(), Error> {
        if let Some(message) = message {
            self.write_command(commands::Command::QUIT(message.to_string()))?;
        } else {
            self.write_command(commands::Command::QUIT(format!(
                "circe {} (https://crates.io/crates/circe)",
                env!("CARGO_PKG_VERSION")
            )))?;
        }

        self.stream.sock.shutdown(Shutdown::Both)?;

        Ok(())
    }
}
