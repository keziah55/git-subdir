//! # git-subdir
//! 
//! Simple command line tool to download a sub directory from a github repo.

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
struct Cli {
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
    site: String,
    raw_site: String,
    username: String,
    repo_name: String,
    branch: String,
    path: PathBuf,
}

impl GitHubUrl {
    /// Create new GitHubUrl instance
    pub fn new(url: &String) -> Result<GitHubUrl, String> {
        let prefix = "https://github.com";
        if !url.starts_with(prefix) {
            return Err(make_error_message(format!("'{}' is not a github url", url)));
        }

        let url_parts: Vec<&str> = url
            .strip_prefix(prefix)
            .unwrap()
            .split("/")
            .filter(|s| s.len() > 0)
            .collect();
        if url_parts.len() == 2 {
            return Err(make_error_message(format!(
                "{}' is a top-level git repo.\nInstead, try:\n  {}",
                url,
                highlight_message(format!("git clone {}", url))
            )));
        } else if url_parts.len() < 4 {
            return Err(make_error_message(format!(
                "'{}' is not a url to a directory within a github repo",
                url
            )));
        }

        if url_parts[2] != "tree" {
            return Err(make_error_message(format!("cannot parse url '{}'", url)));
        }

        let site = String::from(prefix);
        let raw_site = String::from("https://raw.githubusercontent.com");
        let username = String::from(url_parts[0]);
        let repo_name = String::from(url_parts[1]);
        let branch = String::from(url_parts[3]);
        let path = PathBuf::from(url_parts[4..].join("/"));

        Ok(GitHubUrl {
            site,
            raw_site,
            username,
            repo_name,
            branch,
            path,
        })
    }

    /// Return url to directory
    pub fn url(&self) -> String {
        format!(
            "{}/{}/{}/tree/{}/{}",
            self.site,
            self.username,
            self.repo_name,
            self.branch,
            self.path.to_str().unwrap()
        )
    }

    /// Return url to get raw file
    pub fn raw_url(&self) -> String {
        format!(
            "{}/{}/{}/{}/{}",
            self.raw_site,
            self.username,
            self.repo_name,
            self.branch,
            self.path.to_str().unwrap()
        )
        // format!("{}?raw=true", self.url())
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
        GitHubUrl::new(&new_url).unwrap()
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

/// Return a formatted error message.
///
/// # Arguments
///
/// * `msg` - Error message content
fn make_error_message(msg: String) -> String {
    format!("\x1b[91mError:\x1b[0m {}", msg)
}

/// Returns a formatted warning message
///
/// # Arguments
///
/// * `msg` - Error message content
fn make_warning_message(msg: String) -> String {
    format!("\x1b[93m{}\x1b[0m", msg)
}

/// Format the given string to appear in blue.
///
/// # Arguments
///
/// * `msg` - Message content
fn highlight_message(msg: String) -> String {
    format!("\x1b[94m{}\x1b[0m", msg)
}

/// Make directory.
///
/// # Arguments
///
/// * `path` - Dir path to make
///
/// # Panics
///
/// If failed to make directory.
fn make_dir(path: &Path) {
    fs::create_dir_all(path)
        .unwrap_or_else(|_| panic!("Could not create dir '{}'", path.to_str().unwrap()));
}

/// Download all items in github directory.
///
/// Note this function is called recursively.
///
/// # Arguments
///
/// * `url` - [`GitHubUrl`] struct pointing to directory.
/// * `output` - Directory to write to.
/// * `ignore_subdirs` - If `true`, don't download sub directories.
/// * `relative_to` - If `Some(GithubUrl)`, write relative to url. Otherwise, write files relative to repo root.
fn get_subdir(
    url: &GitHubUrl,
    output_path: &PathBuf,
    ignore_subdirs: bool,
    relative_to: Option<&GitHubUrl>,
) {
    // note: using blocking instead of async because this function is called recursively
    let text = reqwest::blocking::get(url.url()).unwrap().text().unwrap();

    // find table of items in html
    let document = Html::parse_document(&text);
    let selector =
        Selector::parse(r#"script[type="application/json"][data-target="react-app.embeddedData"]"#)
            .unwrap();
    for title in document.select(&selector) {
        let v: Value = serde_json::from_str(&title.inner_html()).unwrap();
        // get vector of items
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

/// Download item.
///
/// If item is a file, simply download it. If it is a directory, download all contents
/// (unless `ignore_subdirs` is true).
///
/// # Arguments
///
/// * `base_url` - [`GitHubUrl`] to download.
/// * `item_info` - Map of info about the item to be downloaded.
/// * `output_path` - Path to write to.
/// * `ignore_subdirs` - If `true`, don't download anything if item is a directory.
/// * `relative_to` - Optional url to write relative path to.
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

    // url to item
    let url = base_url.join(item_name);

    // filename to write to
    // create from `output_path` with either abridged or full path
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
        "file" => download_file(&url, &filename),
        "directory" => {
            if !ignore_subdirs {
                get_subdir(&url, output_path, ignore_subdirs, relative_to)
            }
        }
        "symlink_file" | "symlink_directory" => {
            let msg =
                make_warning_message(format!("Skipping symlink '{}'", url.path.to_str().unwrap()));
            println!("{}", msg);
        }
        _ => panic!("Cannot handle item type '{}'", item_type),
    }
}

/// Download given file
///
/// # Arguments
///
/// * `url` - [`GitHubUrl`] struct. This function gets the raw version of the url.
/// * `filename` - Path to write to.
fn download_file(url: &GitHubUrl, filename: &PathBuf) {
    let raw_url = url.raw_url();

    let response = reqwest::blocking::get(raw_url).unwrap();

    let text = response.text().unwrap();

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
/// * `url` - String pointing directory in github repo.
/// * `output` - Optional directory to write to. If `None`, inferred from url.
/// * `ignore_subdirs` - If `true`, don't download sub directories.
/// * `preserve_path_structure` - If `true`, write files relative to repo root. Otherwise, write relative to url.
pub fn get_git_subdir(
    url: &String,
    output: Option<String>,
    ignore_subdirs: bool,
    preserve_path_structure: bool,
) {
    let url = GitHubUrl::new(url);

    match url {
        Ok(url) => {
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

            get_subdir(&url, &output_path, ignore_subdirs, relative_to);
        }
        Err(s) => println!("{s}"),
    }
}

fn main() {
    let cli = Cli::parse();
    get_git_subdir(&cli.url, cli.output, cli.ignore_subdirs, cli.relative);
}

// TESTS
#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
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

        // check content of file
        let contents = fs::read_to_string(filepath).unwrap();
        let first_line = contents.split_once("\n").unwrap().0;
        let expected = "//! # git-subdir";
        assert_eq!(first_line, expected, "expected '{}'; got '{}", expected, first_line);

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
        // test preserving dir structure, relative to git repo root

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

    #[rstest]
    #[case(String::from("https://some-other.url"), "is not a github url")]
    #[case(
        String::from("https://github.com/username/repo"),
        "is a top-level git repo"
    )]
    #[case(
        String::from("https://github.com/username/"),
        "is not a url to a directory within a github repo"
    )]
    #[case(
        String::from("https://github.com/username/repo/not_tree/branch.dir"),
        "cannot parse url"
    )]
    fn test_invalid_url(#[case] url: String, #[case] expected_msg: &str) {
        // let url = String::from("https://some-other.url");
        let result = GitHubUrl::new(&url);
        assert!(result.is_err());

        match result {
            Err(s) => assert!(
                s.contains(expected_msg),
                "'{}' does not contain '{}'",
                s,
                expected_msg
            ),
            Ok(_) => (),
        }
    }
}
