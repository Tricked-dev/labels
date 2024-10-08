// Vendored library is good
#![allow(dead_code)]

#[derive(Debug)]
pub enum CapMode {
    LS,
    END,
}

/// IRC commands
#[derive(Debug)]
pub enum Command {
    // TODO:
    // SERVICE <nickname> <reserved> <distribution> <type> <reserved> <info>
    // SQUIT <server> <comment>
    //
    /// Gets information about the admin of the IRC server.
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Config::from_toml("config.toml")?)?;
    /// client.admin("192.168.178.100")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    ADMIN(
        /// Target
        String,
    ),
    /// Sets the user status to AWAY
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Config::from_toml("config.toml")?)?;
    /// client.away("AFK")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    AWAY(
        /// Message
        String,
    ),
    #[doc(hidden)]
    CAP(CapMode),
    /// Invite user to channel
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Config::from_toml("config.toml")?)?;
    /// client.invite("liblirc", "#circe")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    INVITE(
        /// User
        String,
        /// Channel
        String,
    ),
    /// Joins a channel
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Config::from_toml("config.toml")?)?;
    /// client.join("#main")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    JOIN(
        /// Channel
        String,
    ),
    /// Lists all channels and their topics
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Config::from_toml("config.toml")?)?;
    /// client.list(None, None)?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    LIST(
        /// Channel
        Option<String>,
        /// Server to foreward request to
        Option<String>,
    ),
    /// Sets the mode of the user
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Config::from_toml("config.toml")?)?;
    /// client.mode("test", Some("+B"))?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    /// If the MODE is not given (e.g. None), then the client will send "MODE target"
    MODE(
        /// Channel
        String,
        /// Mode
        Option<String>,
    ),
    /// List all nicknames visiable to the Client
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Config::from_toml("config.toml")?)?;
    /// client.names("#main,#circe", None)?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    NAMES(
        /// Channel
        String,
        /// Server to foreward request to
        Option<String>,
    ),
    #[doc(hidden)]
    NICK(String),
    /// Attempts to identify as a channel operator
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Config::from_toml("config.toml")?)?;
    /// client.oper("username", "password")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    OPER(
        /// Username
        String,
        /// Password
        String,
    ),
    /// Everything that is not a command
    OTHER(String),
    /// Leave a channel
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Config::from_toml("config.toml")?)?;
    /// client.part("#main")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    PART(
        /// Target
        String,
    ),
    #[doc(hidden)]
    PASS(String),
    #[doc(hidden)]
    PING(String),
    #[doc(hidden)]
    PONG(String),
    /// Sends a message in a channel
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Config::from_toml("config.toml")?)?;
    /// client.privmsg("#main", "This is an example message")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    PRIVMSG(
        /// Source Nickname
        String,
        /// Channel
        String,
        /// Message
        String,
    ),
    /// Leaves the IRC
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Config::from_toml("config.toml")?)?;
    /// client.quit(Some("Leaving..."))?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    QUIT(
        /// Leave message
        String,
    ),
    /// Sets or gets the topic of a channel
    /// ```no_run
    /// # use circe::*;
    /// # let mut client = Client::new(Config::from_toml("config.toml")?)?;
    /// client.topic("#main", Some("main channel"))?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    TOPIC(
        /// Channel
        String,
        /// Topic
        Option<String>,
    ),
    #[doc(hidden)]
    USER(String, String, String, String),
}

impl Command {
    /// Creates a Command from a `&str`. Currently only `[PING]` and `[PRIVMSG]` are supported.
    ///
    /// # Panics
    ///
    /// This function will panic if the ``IRCd`` sends malformed messages. Please contact the
    /// maintainer of your ``IRCd`` if this happens.
    #[must_use]
    pub fn command_from_str(s: &str) -> Self {
        let new = s.trim();

        #[cfg(feature = "debug")]
        print!("{}", new);

        let parts: Vec<&str> = new.split_whitespace().collect();

        if parts.get(0) == Some(&"PING") {
            // We can assume that [1] exists because if it doesn't then something's gone very wrong
            // with the IRCD
            let command = parts[1].to_string();
            return Self::PONG(command);
        } else if parts.get(1) == Some(&"PRIVMSG") {
            let nick = parts[0];
            let index = nick.chars().position(|c| c == '!').unwrap(); // This panics for the same reason as above
            let nick = String::from(&nick[1..index]);
            let target = parts[2];
            let mut builder = String::new();

            for part in parts[3..].to_vec() {
                builder.push_str(&format!("{} ", part));
            }

            return Self::PRIVMSG(nick, target.to_string(), (&builder[1..]).to_string());
        }

        Self::OTHER(new.to_string())
    }
}
