#![deny(clippy::unwrap_used)]
use std::fs::DirEntry;
use std::io::ErrorKind;
use std::path::PathBuf;
use tokio::task::JoinSet;
mod download;
mod versions;

const MAGMA_API_URL: &str = "https://api.magmafoundation.org/api/v2/";
const MAX_VERSIONS: usize = 5;

fn get_cwd() -> PathBuf {
    std::env::current_dir().expect("Failed to get current working directory")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
enum Version {
    V1_12_2,
    V1_16_5,
    V1_18_2,
    V1_19_3
}
impl Version {
    fn to_string(&self) -> &'static str {
        match self {
            Version::V1_12_2 => "1.12.2",
            Version::V1_16_5 => "1.16.5",
            Version::V1_18_2 => "1.18.2",
            Version::V1_19_3 => "1.19.3",
        }
    }
}
pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;

fn main() -> Result<(), ()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to build runtime")
        .block_on(run())
}

async fn run() -> Result<(), ()> {
    simple_logger::SimpleLogger::new()
        .with_utc_timestamps()
        .with_module_level("want", log::LevelFilter::Info)
        .with_module_level("reqwest::connect", log::LevelFilter::Info)
        .with_module_level("reqwest::blocking::wait", log::LevelFilter::Info)
        .with_module_level("mio::poll", log::LevelFilter::Info)
        .with_module_level("rustls", log::LevelFilter::Info)
        .init()
        .expect(
            "Failed to initialize logger. Setting the logger for the first time should not fail.",
        );
    let mut js = JoinSet::new();
    js.spawn(get_lib_list(Version::V1_12_2));
    js.spawn(get_lib_list(Version::V1_18_2));
    js.spawn(get_lib_list(Version::V1_19_3));

    while let Some(future) = js.join_next().await {
        if let Ok((version, res)) = future {
            match res {
                Ok(_) => log::info!("Got libs for version {}", version.to_string()),
                Err(e) => log::error!(
                    "Failed to get libs for version {}: {}",
                    version.to_string(),
                    e
                ),
            }
        }
    }
    return Ok(());
}

async fn get_lib_list(version: Version) -> (Version, Result<(), Error>) {
    let resp;
    let req = reqwest::get(MAGMA_API_URL.to_string() + version.to_string()).await;
    match req {
        Err(e) => return (version, Err(Box::new(e))),
        Ok(e) => resp = e,
    }
    let body;
    let body_r = resp.bytes().await;
    match body_r {
        Ok(e) => body = e,
        Err(e) => return (version, Err(Box::new(e))),
    }
    log::info!(
        "Got body for version {}. Parsing JSON.",
        version.to_string()
    );
    let versions = serde_json::from_slice::<Vec<versions::Version>>(&body);

    log::info!("Finished Parsing JSON.");
    let version_name = version.to_string();
    let mut folder_path = get_cwd().clone();
    folder_path.push(version_name);
    let folder_path = folder_path;
    if let Err(e) = std::fs::create_dir_all(folder_path.as_path()) {
        return (version, Err(Box::new(e)));
    }
    let folder_r = std::fs::read_dir(folder_path.as_path());
    let folder: Vec<DirEntry>;
    match folder_r {
        Err(e) => return (version, Err(Box::new(e))),
        Ok(e) => {
            folder = e
                .map(|e| e.ok())
                .filter(Option::is_some)
                .map(|e| unsafe { e.unwrap_unchecked() })
                .collect()
        }
    }

    let mut js: JoinSet<Result<(), Error>> = JoinSet::new();
    match versions {
        Err(e) => return (version, Err(Box::new(e))),
        Ok(mut versions) => {
            let versions_new: Vec<versions::Version>;
            let versions_old: Vec<versions::Version>;
            {
                versions.shrink_to_fit();
                if versions.len() <= MAX_VERSIONS {
                    versions_new = versions;
                    versions_old = Vec::new();
                } else {
                    versions_old = versions.split_off(MAX_VERSIONS);
                    versions_new = versions;
                }
            }
            for i in &folder {
                if let Some(file_name) = i.file_name().to_str() {
                    if versions_old
                        .iter()
                        .filter(|e| {
                            get_name(e.get_link()) == file_name
                                || get_name(e.get_installer_link()) == file_name
                        })
                        .count()
                        > 0
                    {
                        log::trace!("Deleting {}", file_name);
                        js.spawn(remove_version(i.path()));
                    }
                }
            }
            log::info!("Versions: {}", versions_new.len());
            for i in versions_new {
                // log::info!("Handling Version: {:#?}",i);
                // std::thread::sleep(std::time::Duration::from_millis(111));
                download_link(&folder, &folder_path, i.get_link(), &mut js);
                // std::thread::sleep(std::time::Duration::from_millis(111));
                download_link(&folder, &folder_path, i.get_installer_link(), &mut js);
            }
        }
    }
    while let Some(future) = js.join_next().await {
        if let Ok(res) = future {
            match res {
                Ok(_) => {}
                Err(e) => log::error!("{}", e),
            }
        }
    }
    (version, Ok(()))
}
fn download_link(
    folder: &Vec<DirEntry>,
    folder_path: &PathBuf,
    link: &String,
    js: &mut JoinSet<Result<(), Error>>,
) {
    let file_name = get_name(link);
    let path = std::path::Path::new(folder_path).join(&file_name);
    if folder_contains_file_name(folder, &file_name).is_none() {
        log::info!("Downloading {} to {}", link, path.display());
        js.spawn(download::fetch_url(link.clone(), path));
    } else {
        log::info!("{} already exists", path.display())
    }
}

async fn remove_version(i: PathBuf) -> Result<(), Error> {
    match tokio::fs::remove_file(&i).await {
        Ok(_) => {
            log::info!("Deleted {}", i.display());
            Ok(())
        }
        Err(e) => Err(Box::new(std::io::Error::new(
            ErrorKind::Other,
            format!("Failed to delete {}: {}", i.display(), e),
        ))),
    }
}

fn folder_contains_file_name(folder: &Vec<DirEntry>, name: impl AsRef<str>) -> Option<&DirEntry> {
    folder
        .iter()
        // .map(|e| .map(|e|e.to_string()))
        // .filter(|e|e.is_some())
        // .map(|e|unsafe{e.unwrap_unchecked()})
        .filter(|e| match e.file_name().to_str() {
            None => false,
            Some(e) => e == name.as_ref(),
        })
        .next()
}

fn get_name(url: &String) -> &str {
    let name = url.rsplit_once('/');
    match name {
        None => url.as_str(),
        Some((_, end)) => end,
    }
}
