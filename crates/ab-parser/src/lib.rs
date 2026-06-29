pub mod error;
pub mod torrent_parser;
pub mod raw_parser;
pub mod offset_detector;
pub mod enricher;
pub mod title_parser;

pub use error::ParserError;
pub use raw_parser::{raw_parser, Episode};
pub use torrent_parser::{torrent_parser, subtitle_parser, ParsedFile, ParsedSubtitle, FileType};
pub use offset_detector::{detect_offset_mismatch, OffsetSuggestion, TMDBInfo, TMDBSeason, EpisodeAirDate};
pub use enricher::tmdb::{tmdb_search};
pub use enricher::mikan::{mikan_parse, ImageSaver};
pub use enricher::bgm::{fetch_bgm_calendar, match_weekday, bgm_search, CalendarItem};
pub use title_parser::{full_parse, quick_parse, ParserConfig, ParsedBangumi};
