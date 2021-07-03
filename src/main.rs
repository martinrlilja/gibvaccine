use anyhow::Result;
use rand::Rng;
use regex::Regex;
use scraper::{Html, Selector};
use std::{collections::HashMap, io::Write, time::Duration};
use tabwriter::TabWriter;
use termion::{color, style};

const URL: &str = "https://www.vgregion.se/ov/vaccinationstider/bokningsbara-tider/";
const MUNICIPALITIES: &[&str] = &["Ale", "Göteborg", "Kungälv", "Mölndal"];
const MIN_SLEEP_DURATION: Duration = Duration::from_secs(50);
const MAX_SLEEP_DURATION: Duration = Duration::from_secs(120);

#[derive(Clone, Debug)]
struct Location {
    municipality: String,
    organization: String,
    booking_link: String,
    num_available: u64,
}

impl Location {
    fn key(&self) -> (String, String) {
        (self.municipality.clone(), self.organization.clone())
    }
}

fn main() -> Result<()> {
    let mut is_first_run = true;
    let mut current_locations: HashMap<(String, String), Location> = HashMap::new();
    let mut rng = rand::thread_rng();

    loop {
        let now = chrono::Local::now();

        println!(
            "{}{}{} {}{}{}",
            color::Fg(color::LightBlack),
            now.format("%Y-%m-%d").to_string(),
            color::Fg(color::Reset),
            style::Bold,
            now.format("%H:%M:%S").to_string(),
            style::Reset,
        );

        let locations = get_available()?;

        let mut changed_locations = vec![];

        for location in locations.iter() {
            current_locations
                .entry(location.key())
                .and_modify(|mut old_location| {
                    if old_location.num_available != location.num_available {
                        changed_locations.push(location.clone());
                    }
                    old_location.num_available = location.num_available;
                })
                .or_insert_with(|| {
                    changed_locations.push(location.clone());
                    location.clone()
                });
        }

        changed_locations.sort_by_key(|location| location.num_available);

        let filtered_locations = changed_locations
            .iter()
            .filter(|location| MUNICIPALITIES.contains(&location.municipality.as_str()))
            .collect::<Vec<_>>();

        if changed_locations.len() > filtered_locations.len() {
            let num_filtered = changed_locations.len() - filtered_locations.len();
            if num_filtered == 1 {
                println!("Filtered 1 location.");
            } else {
                println!("Filtered {} locations.", num_filtered);
            }
        }

        let mut tabwriter = TabWriter::new(std::io::stdout());

        for location in filtered_locations.iter() {
            writeln!(
                tabwriter,
                "{}{:>5}{}\t{}\t{}\t{}{}{}",
                style::Bold,
                location.num_available,
                style::Reset,
                location.municipality,
                location.organization,
                color::Fg(color::LightBlack),
                location.booking_link,
                color::Fg(color::Reset),
            )?;
        }

        tabwriter.flush()?;

        if let Some(location) = filtered_locations.first() {
            if !is_first_run {
                open::that(&location.booking_link)?;
            }
        }

        is_first_run = false;

        let sleep_duration = rng.gen_range(MIN_SLEEP_DURATION..MAX_SLEEP_DURATION);
        std::thread::sleep(sleep_duration);
    }
}

fn get_available() -> Result<Vec<Location>> {
    let body: String = ureq::get(URL).call()?.into_string()?;

    let document = Html::parse_document(&body);

    let block_selector =
        Selector::parse(".mottagningbookabletimeslistblock .block__row.media").unwrap();

    let locations = document
        .select(&block_selector)
        .flat_map(get_available_location)
        .collect();

    Ok(locations)
}

fn get_available_location<'r>(block: scraper::ElementRef<'r>) -> Option<Location> {
    lazy_static::lazy_static! {
        static ref LOCATION_SELECTOR: Selector = Selector::parse("h3").unwrap();
        static ref LINK_SELECTOR: Selector = Selector::parse("a").unwrap();
        static ref INFO_SELECTOR: Selector = Selector::parse("span").unwrap();

        static ref LOCATION_RE: Regex = Regex::new(r"^\s*(?P<municipality>[^:]+):\s+(?P<organization>.+)$").unwrap();
        static ref INFO_RE: Regex = Regex::new(r"^\s*\((?P<num_available>\d+)").unwrap();
    }

    let location = block
        .select(&LOCATION_SELECTOR)
        .next()
        .map(|location| location.text().collect::<String>());

    let link = block
        .select(&LINK_SELECTOR)
        .next()
        .and_then(|link| link.value().attr("href"));

    let info = block
        .select(&INFO_SELECTOR)
        .next()
        .map(|info| info.text().collect::<String>());

    match (location, link, info) {
        (Some(location), Some(link), Some(info)) => {
            let location_captures = LOCATION_RE.captures(&location);
            let info_captures = INFO_RE.captures(&info);

            match (location_captures, info_captures) {
                (Some(location_captures), Some(info_captures)) => {
                    let municipality = location_captures
                        .name("municipality")
                        .unwrap()
                        .as_str()
                        .to_owned();

                    let organization = location_captures
                        .name("organization")
                        .unwrap()
                        .as_str()
                        .to_owned();

                    let num_available = info_captures
                        .name("num_available")
                        .unwrap()
                        .as_str()
                        .parse()
                        .unwrap();

                    Some(Location {
                        municipality,
                        organization,
                        booking_link: link.to_owned(),
                        num_available,
                    })
                }
                _ => None,
            }
        }
        _ => None,
    }
}
