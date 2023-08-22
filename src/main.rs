#![deny(clippy::unwrap_used)]
use log::LevelFilter;
use std::fs::DirEntry;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::mpsc::TryRecvError;
use std::sync::OnceLock;
use std::thread::JoinHandle;
use tokio::task::JoinSet;
use tokio::time::Instant;

mod download;
mod versions;

const MAGMA_API_URL: &str = "https://api.magmafoundation.org/api/v2/";
const MAX_VERSIONS: usize = 0;

fn get_cwd() -> PathBuf {
    std::env::current_dir().expect("Failed to get current working directory")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
enum Version {
    V1_12_2,
    V1_16_5,
    V1_18_2,
    V1_19_3,
    V1_20_1,
}
impl Version {
    fn to_string(&self) -> &'static str {
        match self {
            Version::V1_12_2 => "1.12.2",
            Version::V1_16_5 => "1.16.5",
            Version::V1_18_2 => "1.18.2",
            Version::V1_19_3 => "1.19.3",
            Version::V1_20_1 => "1.20.1",
        }
    }
}
pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;
fn get_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create runtime")
    })
}
fn main() -> Result<(), ()> {
    get_runtime().block_on(run())
}

const LOG_LEVEL: LevelFilter = LevelFilter::Warn;
async fn run() -> Result<(), ()> {
    simple_logger::SimpleLogger::new()
        .with_utc_timestamps()
        .with_module_level("want", LOG_LEVEL)
        .with_module_level("reqwest::connect", LOG_LEVEL)
        .with_module_level("reqwest::blocking::wait", LOG_LEVEL)
        .with_module_level("mio::poll", LOG_LEVEL)
        .with_module_level("rustls", LOG_LEVEL)
        .with_level(LOG_LEVEL)
        .init()
        .expect(
            "Failed to initialize logger. Setting the logger for the first time should not fail.",
        );
    log::error!("Starting");
    let mut js = JoinSet::new();
    js.spawn(get_lib_list(Version::V1_12_2));
    js.spawn(get_lib_list(Version::V1_16_5));
    js.spawn(get_lib_list(Version::V1_18_2));
    js.spawn(get_lib_list(Version::V1_19_3));
    js.spawn(get_lib_list(Version::V1_20_1));
    let (send_io, rec_io) = std::sync::mpsc::channel::<JoinHandle<Result<(), Error>>>();
    let writer = std::thread::spawn(move || loop {
        match rec_io.try_recv() {
            Err(TryRecvError::Disconnected) => {
                return;
            }
            Err(TryRecvError::Empty) => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Ok(e) => match e.join() {
                Ok(Ok(_)) => log::info!("Finished writing"),
                Ok(Err(e)) => log::error!("Failed to write: {}", e),
                Err(e) => log::error!("Failed to join writer thread: {:#?}", e),
            },
        }
    });
    while let Some(future) = js.join_next().await {
        if let Ok((version, mut download_thread, res)) = future {
            while let Some(thread) = download_thread.join_next().await {
                match thread {
                    Ok(Ok(e)) => {
                        log::info!("Finished downloading {}", version.to_string());
                        send_io.send(e).expect("Failed to send");
                    }
                    Ok(Err(e)) => log::error!("Failed to download: {}", e),
                    Err(e) => log::error!("Failed to join download thread: {}", e),
                }
            }
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
    drop(send_io);
    match writer.join() {
        Ok(_) => {}
        Err(e) => log::error!("Failed to join writer thread: {:#?}", e),
    };
    log::error!("Finished");
    return Ok(());
}

async fn get_lib_list(
    version: Version,
) -> (
    Version,
    JoinSet<Result<JoinHandle<Result<(), Error>>, Error>>,
    Result<(), Error>,
) {
    let mut download: JoinSet<Result<JoinHandle<Result<(), Error>>, Error>> = JoinSet::new();
    let resp;
    let req = reqwest::get(MAGMA_API_URL.to_string() + version.to_string()).await;
    match req {
        Err(e) => return (version, download, Err(Box::new(e))),
        Ok(e) => resp = e,
    }
    let body;
    let body_r = resp.bytes().await;
    match body_r {
        Ok(e) => body = e,
        Err(e) => return (version, download, Err(Box::new(e))),
    }
    log::info!(
        "Got body for version {}. Parsing JSON.",
        version.to_string()
    );
    return match tokio::task::spawn_blocking(move || {
        let versions = serde_json::from_slice::<Vec<versions::Version>>(&body);

        let mut js = JoinSet::new();
        log::info!("Finished Parsing JSON.");
        let version_name = version.to_string();
        let mut folder_path = get_cwd().clone();
        folder_path.push(version_name);
        let folder_path = folder_path;
        let folder_server_path = folder_path.join("server");
        let folder_installer_path = folder_path.join("installer");
        let folder_server: Vec<DirEntry> = match get_folder_content(&folder_server_path) {
            Ok(e) => e,
            Err(e) => return (version, download, js, Err(e)),
        };
        let folder_installer: Vec<DirEntry> = match get_folder_content(&folder_installer_path) {
            Ok(e) => e,
            Err(e) => return (version, download, js, Err(e)),
        };

        match versions {
            Err(e) => return (version, download, js, Err(Box::new(e))),
            Ok(mut versions) => {
                let versions_new: Vec<versions::Version>;
                let versions_old: Vec<versions::Version>;
                {
                    versions.shrink_to_fit();
                    if versions.len() <= MAX_VERSIONS || MAX_VERSIONS == 0 {
                        versions_new = versions;
                        versions_old = Vec::new();
                    } else {
                        versions_old = versions.split_off(MAX_VERSIONS);
                        versions_new = versions;
                    }
                }
                log::info!("Versions: {}", versions_new.len());
                //delete old versions in server folder
                for i in &folder_server {
                    if let Some(file_name) = i.file_name().to_str() {
                        if versions_old
                            .iter()
                            .filter(|e| get_name(e.get_link()) == file_name)
                            .count()
                            > 0
                        {
                            log::trace!("Deleting {}", file_name);
                            js.spawn(remove_version(i.path()));
                        }
                    }
                }
                //delete old versions in installer folder
                for i in &folder_installer {
                    if let Some(file_name) = i.file_name().to_str() {
                        if versions_old
                            .iter()
                            .filter(|e| get_name(e.get_installer_link()) == file_name)
                            .count()
                            > 0
                        {
                            log::trace!("Deleting {}", file_name);
                            js.spawn(remove_version(i.path()));
                        }
                    }
                }
                for i in versions_new {
                    log::trace!("Handling Version: {:#?}", i);
                    let link = i.get_link();
                    let installer_link = i.get_installer_link();
                    download_link(&folder_server, &folder_server_path, link, &mut download);
                    if link != installer_link {
                        download_link(
                            &folder_installer,
                            &folder_installer_path,
                            installer_link,
                            &mut download,
                        );
                    }
                }
            }
        }

        (version, download, js, Ok(()))
    })
    .await
    {
        Ok((verison, download, mut js, ok)) => {
            let pre_io = Instant::now();
            while let Some(thred) = js.join_next().await {
                match thred {
                    Ok(_) => {}
                    Err(e) => log::error!("Failed to join io thread: {}", e),
                }
            }
            log::warn!(
                "Old version IO for version {} took {}ms",
                verison.to_string(),
                pre_io.elapsed().as_millis()
            );
            (verison, download, ok)
        }
        Err(e) => (version, JoinSet::new(), Err(Box::new(e))),
    };
}
fn download_link(
    folder: &Vec<DirEntry>,
    folder_path: impl AsRef<Path>,
    link: &String,
    js: &mut JoinSet<Result<JoinHandle<Result<(), Error>>, Error>>,
) {
    let file_name = get_name(link).to_string();
    let path = folder_path.as_ref().join(&file_name);
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
fn get_folder_content(path: impl AsRef<Path>) -> Result<Vec<DirEntry>, Error> {
    if let Err(e) = std::fs::create_dir_all(&path) {
        return Err(Box::new(e));
    }
    match std::fs::read_dir(&path) {
        Err(e) => Err(Box::new(e)),
        Ok(e) => Ok(e
            .map(|e| e.ok())
            .filter(Option::is_some)
            .map(|e| unsafe { e.unwrap_unchecked() })
            .collect()),
    }
}
