use std::sync::RwLock;
use std::{collections::HashMap, env};
use tinyjson::JsonValue;

macro_rules! define_config {
    ($(
        $field:ident: $type:ty = $default:expr
    ),+ $(,)?) => {
        #[derive(Debug)]
        pub struct Config {
            $(pub $field: RwLock<$type>),+
        }

        impl Default for Config {
            fn default() -> Self {
                Self {
                    $($field: RwLock::new($default)),+
                }
            }
        }

        impl Config {
            pub fn load() -> Self {
                let  config = Config::default();
                let parsed: HashMap<String, JsonValue> = Self::load_json_config();

                $(
                    let key = stringify!($field);
                    if let Ok(value) = env::var(key.to_uppercase()) {
                        *config.$field.write().unwrap() = value.parse().unwrap_or_else(|_| $default);
                    } else if let Some(value) = parsed.get(key) {
                        *config.$field.write().unwrap() = value.get().cloned().unwrap_or_else(|| $default);
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
                if self.openai_api_key.read().unwrap().is_empty() {
                    log::error!("NO openai api key found using fallback parser");
                    log::info!("I would strongly advice you set a openai_api_key in config.json or parsing will be very bad");
                }
                if self.irc_token.read().unwrap().is_empty() {
                    panic!("No IRC token found");
                }
                if self.irc_username.read().unwrap().is_empty() {
                    panic!("No IRC username found");
                }
            }

            // Getter and Setter methods
            $(
                pub fn $field(&self) -> $type {
                    self.$field.read().unwrap().clone()
                }
            )+
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
    set_shutdown_timer: f64 = 0.0,
    censoring_enabled: bool = true,
    invert_overlapping_text: bool = true,
}

impl Config {
    pub fn get_shutdown_time(&self) -> u8 {
        self.set_shutdown_timer
            .read()
            .unwrap()
            .round()
            .clamp(0.0, 4.0) as u8
    }
}
