use color_eyre::{eyre::anyhow, Result};
use tinyjson::JsonValue;

use crate::{Data, CONFIG};

fn escape_json_string(input: &str) -> String {
    let mut escaped = String::new();
    for c in input.chars() {
        match c {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0C}' => escaped.push_str("\\f"),
            _ if c.is_control() => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            _ => escaped.push(c),
        }
    }
    escaped
}

pub fn parse_json_to_data(json: &str) -> Option<Data> {
    let data: JsonValue = json.parse().ok()?;
    let content: &String = data["choices"][0]["message"]["content"].get()?;
    let data_content: JsonValue = content.parse().ok()?;
    let x: &f64 = data_content["x"].get()?;
    let y: &f64 = data_content["y"].get()?;
    let size: &f64 = data_content["size"].get()?;
    let rest_text: &String = data_content["rest_text"].get()?;

    let data = Data {
        text: rest_text.clone(),
        x: (*x).min(CONFIG.width) as u32,
        y: (*y).min(CONFIG.height) as u32,
        size: (*size).min(CONFIG.max_size) as u32,
    };
    Some(data)
}

pub fn text_to_data(text: &str) -> Result<Data> {
    let body = [
        r##"
{
  "model": ""##,
        &CONFIG.model,
        r##"",
  "messages": [
    {
      "role": "system",
      "content": ""##,
        &CONFIG.prompt,
        r##""
    },
    {
      "role": "user",
      "content": "QUERY"
    }
  ],
  "response_format": {
    "type": "json_schema",
    "json_schema": {
      "name": "extract_schema",
      "schema": {
        "type": "object",
        "properties": {
          "rest_text": {
            "description": "The rest of the text",
            "type": "string"
          },
          "x": {
            "description": "The x location",
            "type": "integer"
          },
          "y": {
            "description": "The y location",
            "type": "integer"
          },
          "size": {
            "description": "The size",
            "type": "integer"
          }
        },
        "additionalProperties": false
      }
    }
  }
}

    "##,
    ]
    .join("");

    let req = attohttpc::post("https://api.openai.com/v1/chat/completions")
        .text(body.replace("QUERY", &escape_json_string(text)))
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            format!("Bearer {}", CONFIG.openai_api_key.clone()),
        )
        .send()?;

    let data = parse_json_to_data(req.text()?.as_str()).ok_or(anyhow!("Failed to parse JSON"))?;
    log::info!("Data: {:?}", data);
    Ok(data)
}
