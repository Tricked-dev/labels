use crate::CONFIG;

use super::Data;

pub fn parse_string(input: &str) -> Option<Data> {
    if let Some(pos) = input.rfind(' ') {
        let (text, numbers) = input.split_at(pos);
        let mut parts = numbers.trim().split(',');

        // Attempt to parse x and y first
        if let (Some(x_str), Some(y_str)) = (parts.next(), parts.next()) {
            let size = parts.next().unwrap_or("5"); // Use default size 5 if missing

            if let (Ok(x), Ok(y), Ok(size)) = (
                x_str.parse::<u32>(),
                y_str.parse::<u32>(),
                size.parse::<u32>(),
            ) {
                return Some(Data {
                    text: text.to_string(),
                    x: x.min(CONFIG.width as u32),
                    y: y.min(CONFIG.height as u32),
                    size: size.min(CONFIG.max_size as u32),
                });
            }
        }
    }
    None
}

#[test]
fn test_parsing() {
    let test_cases = [
        "hello world 20,20,100",
        "test 30,40,20",
        "hi 50,60,10",
        "example 70,80,200",
        "another test 100,150",
        "missing size number",
        "ayo what 10,10,9",
        "TT 10,10",
        "omg 20,20",
    ];

    for &test_case in &test_cases {
        match parse_string(test_case) {
            Some(data) => println!(
                "Parsed Data - Text: '{}', X: {}, Y: {}, Size: {}",
                data.text, data.x, data.y, data.size
            ),
            None => println!("Failed to parse: '{}'", test_case),
        }
    }
}
