use std::io::Cursor;
use std::path::Path;
use tokio::sync::OnceCell;

static DL_LOCKS: [OnceCell<tokio::sync::Mutex<()>>; 5] = [
    OnceCell::const_new(),
    OnceCell::const_new(),
    OnceCell::const_new(),
    OnceCell::const_new(),
    OnceCell::const_new(),
];
const NOT_FOUND: bytes::Bytes =
    bytes::Bytes::from_static(br#"{"message":"404 Project Not Found"}"#);

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub async fn fetch_url(url: String, file_name: impl AsRef<Path>) -> Result<()> {
    log::info!("Downloading {}", url);
    let mut bytes = try_get_bytes(&url).await;
    while bytes.is_err() {
        log::warn!("Failed to download {}, retrying.", url);
        bytes = try_get_bytes(&url).await;
    }
    log::info!("Got Bytes {}", url);
    let bytes = bytes.expect("It should be impossible for bytes to be an error here");
    let mut file = std::fs::File::create(file_name.as_ref())?;
    let mut content = Cursor::new(bytes);
    std::io::copy(&mut content, &mut file)?;
    log::info!(
        "Finished Downloading {} to {}",
        url,
        file_name.as_ref().display()
    );
    Ok(())
}

async fn try_get_bytes(url: impl reqwest::IntoUrl) -> Result<bytes::Bytes> {
    let (mut0, mut1, mut2, mut3, mut4) = tokio::join!(
        DL_LOCKS[0].get_or_init(|| async { tokio::sync::Mutex::new(()) }),
        DL_LOCKS[1].get_or_init(|| async { tokio::sync::Mutex::new(()) }),
        DL_LOCKS[2].get_or_init(|| async { tokio::sync::Mutex::new(()) }),
        DL_LOCKS[3].get_or_init(|| async { tokio::sync::Mutex::new(()) }),
        DL_LOCKS[4].get_or_init(|| async { tokio::sync::Mutex::new(()) })
    );
    let _lock;
    tokio::select! {
        lock1 = mut0.lock() => _lock = lock1,
        lock1 = mut1.lock() => _lock = lock1,
        lock1 = mut2.lock() => _lock = lock1,
        lock1 = mut3.lock() => _lock = lock1,
        lock1 = mut4.lock() => _lock = lock1,
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
