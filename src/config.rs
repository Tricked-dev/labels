use std::{collections::HashMap, env, path::Path};

use tinyjson::JsonValue;
#[derive(Debug)]
pub struct Config {
    pub model: String,
    pub prompt: String,
    pub openai_api_key: String,
    pub irc_host: String,
    pub irc_channel: String,
    pub irc_token: String,
    pub irc_username: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: "gpt-4o-mini".to_string(),
            prompt: "You extract the x y location and size from a text, the x and y can appear anywhere in the text and the size can be nothing in which case you set it to 5, remove the indication words such as Place,At and with. If x<number> is used you remove the x and set number to size".to_owned(),
            openai_api_key: "".to_lowercase(),
            irc_token: "".to_owned(),
            irc_host: "irc.chat.twitch.tv".to_string(),
            irc_username:"".to_owned(),
            irc_channel:"".to_owned()
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let mut config = Config::default();
        if let Ok(model) = env::var("MODEL") {
            config.model = model;
        }
        if let Ok(prompt) = env::var("PROMPT") {
            config.prompt = prompt;
        }
        if let Ok(openai_api_key) = env::var("OPENAI_API_KEY") {
            config.openai_api_key = openai_api_key;
        };

        if let Ok(irc_host) = env::var("IRC_HOST") {
            config.irc_host = irc_host;
        };
        if let Ok(irc_token) = env::var("IRC_TOKEN") {
            config.irc_token = irc_token;
        };
        if let Ok(irc_username) = env::var("IRC_USERNAME") {
            config.irc_username = irc_username;
        };
        if let Ok(irc_channel) = env::var("IRC_CHANNEL") {
            config.irc_channel = irc_channel;
        };

        let config_path = env::var("CONFIG_PATH").unwrap_or("config.json".to_string());
        if Path::new(&config_path).exists() {
            let file = std::fs::read_to_string(config_path).unwrap();
            let data: JsonValue = file.parse().unwrap();
            let parsed: &HashMap<_, _> = data.get().unwrap();
            if let Some(model) = parsed.get("model") {
                config.model = model.get::<String>().unwrap().to_string();
            }
            if let Some(prompt) = parsed.get("prompt") {
                config.prompt = prompt.get::<String>().unwrap().to_string();
            }
            if let Some(openai_api_key) = parsed.get("openai_api_key") {
                config.openai_api_key = openai_api_key.get::<String>().unwrap().to_string();
            }
            if let Some(irc_token) = parsed.get("irc_token") {
                config.irc_token = irc_token.get::<String>().unwrap().to_string();
            }
            if let Some(irc_host) = parsed.get("irc_host") {
                config.irc_host = irc_host.get::<String>().unwrap().to_string();
            }
            if let Some(irc_username) = parsed.get("irc_username") {
                config.irc_username = irc_username.get::<String>().unwrap().to_string();
            }
            if let Some(irc_channel) = parsed.get("irc_channel") {
                config.irc_channel = irc_channel.get::<String>().unwrap().to_string();
            }
        };

        if config.openai_api_key.is_empty() {
            panic!("No OpenAI API key found");
        };
        if config.irc_token.is_empty() {
            panic!("No IRC token found");
        };
        if config.irc_username.is_empty() {
            panic!("No IRC username found");
        };

        config
    }
}
