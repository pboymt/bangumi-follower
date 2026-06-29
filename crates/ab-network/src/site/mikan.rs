use quick_xml::Reader;
use quick_xml::events::Event;
use crate::error::NetworkError;
use crate::client::RssEntry;

pub fn parse_rss(xml: &str) -> Result<Vec<RssEntry>, NetworkError> {
    let mut reader = Reader::from_str(xml);
    let mut entries = Vec::new();
    let mut current_title = String::new();
    let mut current_url = String::new();
    let mut current_homepage = String::new();
    let mut in_item = false;
    let mut in_title = false;
    let mut in_link = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "item" => {
                        in_item = true;
                        current_title.clear();
                        current_url.clear();
                        current_homepage.clear();
                    }
                    "title" if in_item => in_title = true,
                    "link" if in_item => in_link = true,
                    "enclosure" if in_item => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"url" {
                                current_url = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "enclosure" && in_item {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"url" {
                            current_url = String::from_utf8_lossy(&attr.value).to_string();
                        }
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default().to_string();
                if in_title { current_title.push_str(&text); }
                if in_link { current_homepage.push_str(&text); }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "item" => {
                        if in_item {
                            let url = if current_url.is_empty() { current_homepage.clone() } else { current_url.clone() };
                            entries.push(RssEntry {
                                title: current_title.clone(),
                                url,
                                homepage: current_homepage.clone(),
                            });
                            in_item = false;
                        }
                    }
                    "title" => in_title = false,
                    "link" => in_link = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(NetworkError::XmlParse(format!("XML parse error: {e}"))),
            _ => {}
        }
        buf.clear();
    }
    Ok(entries)
}

pub fn parse_rss_title(xml: &str) -> Result<String, NetworkError> {
    let mut reader = Reader::from_str(xml);
    let mut in_channel = false;
    let mut in_title = false;
    let mut title = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "channel" => in_channel = true,
                    "title" if in_channel => in_title = true,
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_title {
                    title = e.unescape().unwrap_or_default().to_string();
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "title" => in_title = false,
                    "channel" => break,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(NetworkError::XmlParse(format!("XML parse error: {e}"))),
            _ => {}
        }
        buf.clear();
    }
    Ok(title)
}
