use std::{collections::HashMap, env};
use tinyjson::JsonValue;

macro_rules! define_config {
    ($(
        $field:ident: $type:ty = $default:expr,
        key: $json_key:expr
    ),+ $(,)?) => {
        #[derive(Debug)]
        pub struct Config {
            $(pub $field: $type),+
        }

        impl Default for Config {
            fn default() -> Self {
                Self {
                    $($field: $default),+
                }
            }
        }

        impl Config {
            pub fn load() -> Self {
                let mut config = Config::default();
                let parsed: HashMap<String, JsonValue> = Self::load_json_config();

                $(
                    if let Ok(value) = env::var($json_key.to_uppercase()) {
                        config.$field = value.parse().unwrap_or(config.$field);
                    } else if let Some(value) = parsed.get($json_key) {
                        config.$field = value.get().cloned().unwrap_or(config.$field);
                    }
                )+

                config.validate();
                config
            }

            fn load_json_config() -> HashMap<String, JsonValue> {
                let config_path = env::var("CONFIG_PATH").unwrap_or_else(|_| "config.json".to_string());
                std::fs::read_to_string(config_path)
                    .ok()
                    .and_then(|file| file.parse::<JsonValue>().ok())
                    .and_then(|data| data.get().cloned())
                    .unwrap_or_default()
            }

            fn validate(&self) {
                if self.openai_api_key.is_empty() {
                    panic!("No OpenAI API key found");
                }
                if self.irc_token.is_empty() {
                    panic!("No IRC token found");
                }
                if self.irc_username.is_empty() {
                    panic!("No IRC username found");
                }
            }
        }
    };
}

define_config! {
    model: String = "gpt-4o-mini".to_string(),
    key: "model",

    prompt: String = "You extract the x y location and size from a text, the x and y can appear anywhere in the text and the size can be nothing in which case you set it to 5, remove the indication words such as Place,At and with. If x<number> is used you remove the x and set number to size".to_owned(),
    key: "prompt",

    openai_api_key: String = String::new(),
    key: "openai_api_key",

    irc_host: String = "irc.chat.twitch.tv".to_string(),
    key: "irc_host",

    irc_channel: String = String::new(),
    key: "irc_channel",

    irc_token: String = String::new(),
    key: "irc_token",

    irc_username: String = String::new(),
    key: "irc_username",

    irc_port: f64 = 6697.0,
    key: "irc_port",

    width: f64 = 500.0,
    key: "width",

    height: f64 = 500.0,
    key: "height",

    notify_url: String = String::new(),
    key: "notify_url",
}
