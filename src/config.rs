use std::{collections::HashMap, env};
use tinyjson::JsonValue;

macro_rules! define_config {
    ($(
        $field:ident: $type:ty = $default:expr
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
                    let key = stringify!($field);
                    if let Ok(value) = env::var(key.to_uppercase()) {
                        config.$field = value.parse().unwrap_or(config.$field);
                    } else if let Some(value) = parsed.get(key) {
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
    prompt: String = "You extract the x y location and size from a text, the x and y can appear anywhere in the text and the size can be nothing in which case you set it to 5, remove the indication words such as Place,At and with. If x<number> is used you remove the x and set number to size".to_owned(),

    openai_api_key: String = String::new(),
    irc_host: String = "irc.chat.twitch.tv".to_string(),
    irc_channel: String = String::new(),
    irc_token: String = String::new(),
    irc_username: String = String::new(),
    irc_port: f64 = 6697.0,
    width: f64 = 500.0,
    height: f64 = 500.0,
    notify_url: String = String::new(),
    clock_time: f64 = 60.0 * 5.0,
    timer_file: String = "timer.txt".to_string(),
    timer_prefix: String = "printing starts in: ".to_string(),
    disable_printer: bool = false,
    save_path: String = "saves/".to_string(),
    max_size: f64 = 100.0,
    test_text: bool = false,
}

impl Config {
    pub fn height(&self) -> usize {
        (self.height) as usize
    }

    pub fn width(&self) -> usize {
        (self.width) as usize
    }
}
