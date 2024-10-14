#![allow(dead_code)]
use attohttpc::header::{HeaderMap, HeaderValue};
use color_eyre::{eyre::anyhow, Result};
use std::fmt::Write;
pub struct Action {
    action_type: String,
    title: String,
    url: String,
}

pub struct NotifyBuilder {
    body: String,
    headers: HeaderMap,
    actions: Vec<Action>,
}

impl NotifyBuilder {
    pub fn new(body: String) -> Self {
        Self {
            body,
            headers: HeaderMap::new(),
            actions: Vec::new(),
        }
    }

    fn set_string_header(&mut self, key: &'static str, value: &str) {
        self.headers
            .insert(key, HeaderValue::from_str(value).unwrap());
    }

    pub fn set_title(mut self, title: String) -> Self {
        self.set_string_header("title", &title);
        self
    }
    pub fn set_priority(mut self, priority: String) -> Self {
        self.set_string_header("priority", &priority);
        self
    }
    pub fn set_tags(mut self, tags: String) -> Self {
        self.set_string_header("tags", &tags);
        self
    }

    pub fn add_action(mut self, title: String, url: String) -> Self {
        let action = Action {
            action_type: "view".to_string(),
            title,
            url,
        };
        self.actions.push(action);
        self
    }

    pub fn send(mut self, url: &str) -> Result<()> {
        if url.is_empty() {
            log::warn!("No url provided, not sending notification");
            return Ok(());
        }
        let mut actions_header = String::new();
        for a in &self.actions {
            let _ = write!(actions_header, "{}, {}, {};", a.action_type, a.title, a.url);
        }

        self.set_string_header("actions", &actions_header);

        let mut req = attohttpc::post(url).text(self.body);

        for (key, value) in self.headers.iter() {
            req = req.header(key, value);
        }
        let req = req.send()?;

        if req.is_success() {
            Ok(())
        } else {
            Err(anyhow!("Failed to send notification"))
        }
    }
}

#[test]
fn test_notify() {
    let notif = NotifyBuilder::new("Hello world".to_string())
        .set_title("Hello world".to_string())
        .set_priority("high".to_string())
        .set_tags("warning,error,smile".to_string())
        .add_action("Google".to_string(), "https://google.com".to_string())
        .send("https://ntfy.sh/goog");

    println!("{:?}", notif);
}
