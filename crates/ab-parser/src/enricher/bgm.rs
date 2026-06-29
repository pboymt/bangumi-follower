use ab_network::NetworkClient;
use crate::error::ParserError;

#[derive(Debug, Clone)]
pub struct CalendarItem {
    pub name: String,
    pub name_cn: String,
    pub air_weekday: i32,
}

pub async fn fetch_bgm_calendar(client: &NetworkClient) -> Result<Vec<CalendarItem>, ParserError> {
    let url = "https://api.bgm.tv/calendar";
    let data: serde_json::Value = client.get_json(url).await.map_err(ParserError::Network)?;

    let mut calendar = Vec::new();
    if let Some(days) = data.as_array() {
        for (weekday, day) in days.iter().enumerate() {
            if let Some(items) = day["items"].as_array() {
                for item in items {
                    let name = item["name"].as_str().unwrap_or("").to_string();
                    let name_cn = item["name_cn"].as_str().unwrap_or("").to_string();
                    if !name.is_empty() || !name_cn.is_empty() {
                        calendar.push(CalendarItem {
                            name,
                            name_cn,
                            air_weekday: weekday as i32,
                        });
                    }
                }
            }
        }
    }
    Ok(calendar)
}

pub fn match_weekday(
    official_title: &str,
    title_raw: &str,
    calendar: &[CalendarItem],
) -> Option<i32> {
    for item in calendar {
        if item.name_cn == official_title || item.name == official_title {
            return Some(item.air_weekday);
        }
    }
    for item in calendar {
        if item.name == title_raw || item.name_cn == title_raw {
            return Some(item.air_weekday);
        }
    }
    for item in calendar {
        if official_title.len() >= 4 && item.name_cn.contains(official_title) {
            return Some(item.air_weekday);
        }
        if title_raw.len() >= 4 && item.name.contains(title_raw) {
            return Some(item.air_weekday);
        }
    }
    None
}

pub async fn bgm_search(client: &NetworkClient, title: &str) -> Result<Option<serde_json::Value>, ParserError> {
    let url = format!("https://api.bgm.tv/search/subject/{title}?type=2");
    let data: serde_json::Value = client.get_json(&url).await.map_err(ParserError::Network)?;
    Ok(Some(data))
}
