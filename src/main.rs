use clap::Parser;
use reqwest;
use scraper::{Html, Selector};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(version = "0.1")]
#[command(about="download a subdirectory from github repo", long_about=None)]
pub struct Cli {
    /// Github url
    url: String,

    /// Output directory. Is created if it doesn't exist.
    #[arg(short = 'o', long)]
    output: Option<String>,

    /// Ignore subdirectories
    #[arg(short = 'i', long)]
    ignore_subdirs: bool,
}

fn make_dir(path: &Path) {
    fs::create_dir_all(path)
        .unwrap_or_else(|_| panic!("Could not create dir '{}'", path.to_str().unwrap()));
}

fn get_git_dir(url: &String, output_path: &PathBuf, ignore_subdirs: bool) {
    let text = reqwest::blocking::get(url).unwrap().text().unwrap();

    let document = Html::parse_document(&text);
    let selector =
        Selector::parse(r#"script[type="application/json"][data-target="react-app.embeddedData"]"#)
            .unwrap();
    for title in document.select(&selector) {
        let v: Value = serde_json::from_str(&title.inner_html()).unwrap();

        let items = v["payload"]["tree"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_object().unwrap());

        for item in items {
            download(url, item, output_path, ignore_subdirs);
        }
    }
}

fn download(
    base_url: &String,
    item_info: &serde_json::Map<String, serde_json::Value>,
    output_path: &PathBuf,
    ignore_subdirs: bool,
) {
    let item_type = item_info["contentType"].as_str().unwrap();
    let item_name = item_info["name"].as_str().unwrap();
    let item_path = item_info["path"].as_str().unwrap();
    // TODO: get item_path relative to url, rather than repo root
    // e.g. given https://github.com/keziah55/tracks/tree/main/tracks/activities/test
    // currently writes files like 'tracks/activities/test/__init__.py'
    // so find a nice way of subtracting the end of the url from the beginning of the path

    let url = format!("{}/{}", base_url.clone(), item_name);

    match item_type {
        "file" => download_file(base_url, item_path, output_path),
        "directory" => {
            if !ignore_subdirs {
                get_git_dir(&url, output_path, ignore_subdirs)
            }
        }
        _ => panic!("Cannot handle item type '{}'", item_type),
    }
}

fn download_file(url: &String, item_path: &str, output_path: &PathBuf) {
    let url = format!("{}?raw=true", url.clone());

    let text = reqwest::blocking::get(url).unwrap().text().unwrap();

    let mut filename = PathBuf::from(output_path);
    filename.push(item_path);

    if !filename.parent().unwrap().exists() {
        make_dir(filename.clone().parent().unwrap());
    }

    fs::write(filename, text).unwrap();

    println!("Downloaded '{}'", item_path);
}

pub fn get_from_cli(cli: Cli) {
    let output_path = match cli.output {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from("."),
    };

    if !output_path.exists() {
        make_dir(&output_path);
    }

    get_git_dir(&cli.url, &output_path, cli.ignore_subdirs);
}

fn main() {
    let cli = Cli::parse();

    // let url = String::from("https://github.com/gjf2a/midi_fundsp/tree/master/examples");
    get_from_cli(cli);
}
