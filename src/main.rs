use clap::Parser;
use reqwest;
use scraper::{Html, Selector};
use serde_json::Value;
use std::{
    fmt, fs,
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

    /// Write paths relative to given url rather than repo root.
    #[arg(short = 'r', long)]
    relative: bool,
}

struct GitHubUrl {
    username: String,
    repo_name: String,
    branch: String,
    path: PathBuf,
}

impl GitHubUrl {
    /// Create new GitHubUrl instance
    pub fn new(url: &String) -> GitHubUrl {
        let prefix = "https://github.com/";
        if !url.starts_with(prefix) {
            panic!("'{}' is not a github url", url);
        }

        let url_parts: Vec<&str> = url.strip_prefix(prefix).unwrap().split("/").collect();
        if url_parts.len() < 4 {
            panic!("'{}' is not a url to a directory within a github repo", url);
        }

        if url_parts[2] != "tree" {
            panic!("Cannot parse url '{}'", url);
        }

        let username = String::from(url_parts[0]);
        let repo_name = String::from(url_parts[1]);
        let branch = String::from(url_parts[3]);
        let path = PathBuf::from(url_parts[4..].join("/"));

        GitHubUrl {
            username,
            repo_name,
            branch,
            path,
        }
    }

    /// Return url to directory
    pub fn url(&self) -> String {
        format!(
            "https://github.com/{}/{}/tree/{}/{}",
            self.username,
            self.repo_name,
            self.branch,
            self.path.to_str().unwrap()
        )
    }

    /// Return the name of the requested dir
    pub fn basename(&self) -> String {
        String::from(
            self.path
                .components()
                .last()
                .unwrap()
                .as_os_str()
                .to_str()
                .unwrap(),
        )
    }

    /// Return new GitHubUrl with `part` appended
    pub fn join(&self, part: &str) -> GitHubUrl {
        let new_url = format!("{}/{}", self.url(), part);
        GitHubUrl::new(&new_url)
    }

    /// Return url to get raw file
    pub fn as_raw_url(&self) -> String {
        format!("{}?raw=true", self.url())
    }
}

impl fmt::Display for GitHubUrl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "GitHubUrl {{\n  username: {},\n  repo: {},\n  branch: {},\n  path: {}\n}}",
            self.username,
            self.repo_name,
            self.branch,
            self.path.to_str().unwrap()
        )
    }
}

fn make_dir(path: &Path) {
    fs::create_dir_all(path)
        .unwrap_or_else(|_| panic!("Could not create dir '{}'", path.to_str().unwrap()));
}

fn get_git_dir(
    url: &GitHubUrl,
    output_path: &PathBuf,
    ignore_subdirs: bool,
    relative_to: Option<&GitHubUrl>,
) {
    // note: using blocking instead of async because this function is called recursively
    let text = reqwest::blocking::get(url.url()).unwrap().text().unwrap();

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
            download(url, item, output_path, ignore_subdirs, relative_to);
        }
    }
}

fn download(
    base_url: &GitHubUrl,
    item_info: &serde_json::Map<String, serde_json::Value>,
    output_path: &PathBuf,
    ignore_subdirs: bool,
    relative_to: Option<&GitHubUrl>,
) {
    let item_type = item_info["contentType"].as_str().unwrap();
    let item_name = item_info["name"].as_str().unwrap();
    let item_path = PathBuf::from(item_info["path"].as_str().unwrap());

    let url = base_url.join(item_name);

    let mut filename = PathBuf::from(output_path);

    let rel_path = match relative_to {
        Some(rel_url) => String::from(
            item_path
                .strip_prefix(rel_url.path.clone().to_str().unwrap())
                .unwrap()
                .to_str()
                .unwrap(),
        ),
        None => item_path
            .components()
            .skip(1)
            .map(|p| p.as_os_str().to_str().unwrap())
            .collect::<Vec<&str>>()
            .join("/"),
    };

    filename.push(rel_path);

    match item_type {
        "file" => download_file(base_url, &filename),
        "directory" => {
            if !ignore_subdirs {
                get_git_dir(&url, output_path, ignore_subdirs, relative_to)
            }
        }
        "symlink_file" => {
            println!("Skipping symlink '{}'", url.path.to_str().unwrap());
        }
        "symlink_directory" => {
            println!("Skipping symlink '{}'", url.path.to_str().unwrap());
        }
        _ => panic!("Cannot handle item type '{}'", item_type),
    }
}

fn download_file(url: &GitHubUrl, filename: &PathBuf) {
    let raw_url = url.as_raw_url();

    let text = reqwest::blocking::get(raw_url).unwrap().text().unwrap();

    if !filename.parent().unwrap().exists() {
        make_dir(filename.clone().parent().unwrap());
    }

    fs::write(filename, text).unwrap();

    println!("Downloaded '{}'", filename.to_str().unwrap());
}

/// Download directory from github
/// 
/// # Arguments
/// 
/// * `url` - String pointing directory in github repo
/// * `output` - Optional directory to write to. If `None`, inferred from url.
/// * `ignore_subdirs` - If `true`, don't download sub directories.
/// * `preserve_path_structure` - If `true`, 
pub fn get_git_subdir(
    url: &String,
    output: Option<String>,
    ignore_subdirs: bool,
    preserve_path_structure: bool,
) {
    println!("preserve_path_structure={preserve_path_structure}");
    let url = GitHubUrl::new(url);

    // if not given output path, use basename from url
    let output_path = match output {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(url.basename()),
    };

    if !output_path.exists() {
        make_dir(&output_path);
    }

    let relative_to = if preserve_path_structure {
        None
    } else {
        Some(&url)
    };

    get_git_dir(&url, &output_path, ignore_subdirs, relative_to);
}

fn main() {
    let cli = Cli::parse();
    get_git_subdir(&cli.url, cli.output, cli.ignore_subdirs, cli.relative);
}

// TESTS
#[test]
fn test_default_args() {
    // test default behaviour

    let url = String::from("https://github.com/keziah55/ABBAd_day/tree/master/ABBAd_day");

    get_git_subdir(&url, None, false, false);

    let expected_path = PathBuf::from("ABBAd_day");
    assert!(expected_path.exists());
    let expected_files = vec!["ABBAd_day.ino", "fileArray.ino"];
    for filename in expected_files {
        assert!(expected_path.join(filename).exists());
    }

    fs::remove_dir_all(expected_path).unwrap();
}

#[test]
fn test_custom_output_path() {
    // test set custom outdir for contents

    let url = String::from("https://github.com/keziah55/git-subdir/tree/main/src");

    let output_path = PathBuf::from("tmp_test");
    let expected_path = output_path.clone();
    let output_path_arg = Some(output_path.into_os_string().into_string().unwrap());

    get_git_subdir(&url, output_path_arg, false, false);

    assert!(expected_path.exists());
    let filepath = expected_path.join("main.rs");
    assert!(filepath.exists());

    fs::remove_dir_all(expected_path).unwrap();
}

#[test]
fn test_ignore_subdirs() {
    // test don't get subdirs

    let url = String::from("https://github.com/keziah55/pick/tree/main/mediabrowser");

    get_git_subdir(&url, None, true, false);

    let output_path = PathBuf::from("mediabrowser");
    let expected_path = output_path.clone();
    assert!(expected_path.exists());
    for item in fs::read_dir(expected_path).unwrap() {
        let p = item.unwrap().path();
        assert!(!p.is_dir(), "{:#?} is dir", p);
        assert_eq!(p.extension().unwrap(), "py");
    }

    fs::remove_dir_all(output_path).unwrap();
}

#[test]
fn test_relative_path() {
    // test preserving dir strcuture, relative to git repo root

    let url = String::from(
        "https://github.com/keziah55/pick/tree/main/mediabrowser/templates/mediabrowser",
    );

    let output_path = PathBuf::from("tmp_test");
    let mut expected_path = output_path.clone();
    let output_path_arg = Some(output_path.clone().into_os_string().into_string().unwrap());

    get_git_subdir(&url, output_path_arg, false, true);

    expected_path.push("templates/mediabrowser");

    assert!(expected_path.exists());

    for item in fs::read_dir(expected_path).unwrap() {
        let p = item.unwrap().path();
        assert!(!p.is_dir());
        assert_eq!(p.extension().unwrap(), "html");
    }

    fs::remove_dir_all(output_path).unwrap();
}
