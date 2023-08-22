use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;
use std::thread::JoinHandle;

const DL_LOCK_NUMBER: usize = 10;
static DL_LOCKS: [OnceLock<tokio::sync::Mutex<()>>; DL_LOCK_NUMBER] = [
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
];
const NOT_FOUND: bytes::Bytes =
    bytes::Bytes::from_static(br#"{"message":"404 Project Not Found"}"#);

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub async fn fetch_url(url: String, file_name: impl AsRef<Path>) -> Result<JoinHandle<Result<()>>> {
    log::info!("Downloading {}", url);
    let mut bytes = try_get_bytes(&url).await;
    while bytes.is_err() {
        log::warn!("Failed to download {}, retrying.", url);
        bytes = try_get_bytes(&url).await;
    }
    log::info!("Got Bytes {}", url);
    let bytes = bytes.expect("It should be impossible for bytes to be an error here");
    log::info!(
        "Finished Downloading {} to {}",
        url,
        file_name.as_ref().display()
    );
    let path = file_name.as_ref().to_path_buf();
    Ok(std::thread::spawn(move || {
        let mut file = std::fs::File::create(path)?;
        file.write_all(&bytes)?;
        Ok(())
    }))
}

async fn try_get_bytes(url: impl reqwest::IntoUrl) -> Result<bytes::Bytes> {
    let _lock;
    tokio::select! {
        biased;
        lock1 = DL_LOCKS[0].get_or_init(|| tokio::sync::Mutex::new(())).lock() => _lock = lock1,
        lock1 = DL_LOCKS[1].get_or_init(|| tokio::sync::Mutex::new(())).lock() => _lock = lock1,
        lock1 = DL_LOCKS[2].get_or_init(|| tokio::sync::Mutex::new(())).lock() => _lock = lock1,
        lock1 = DL_LOCKS[3].get_or_init(|| tokio::sync::Mutex::new(())).lock() => _lock = lock1,
        lock1 = DL_LOCKS[4].get_or_init(|| tokio::sync::Mutex::new(())).lock() => _lock = lock1,
        lock1 = DL_LOCKS[5].get_or_init(|| tokio::sync::Mutex::new(())).lock() => _lock = lock1,
        lock1 = DL_LOCKS[6].get_or_init(|| tokio::sync::Mutex::new(())).lock() => _lock = lock1,
        lock1 = DL_LOCKS[7].get_or_init(|| tokio::sync::Mutex::new(())).lock() => _lock = lock1,
        lock1 = DL_LOCKS[8].get_or_init(|| tokio::sync::Mutex::new(())).lock() => _lock = lock1,
        lock1 = DL_LOCKS[9].get_or_init(|| tokio::sync::Mutex::new(())).lock() => _lock = lock1,
    }
    let response = reqwest::get(url).await?;
    let status = response.status();
    let bytes = response.bytes().await?;
    if status == reqwest::StatusCode::NOT_FOUND && bytes == NOT_FOUND {
        return Ok(bytes::Bytes::new());
    }
    if status != reqwest::StatusCode::OK {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "request was not status code OK",
        )));
    }
    Ok(bytes)
}
