//! macOS download of the co-op internet helper (Cloudflare's `cloudflared`).
//!
//! Upstream fetches the Windows `.exe` with PowerShell. On macOS Cloudflare ships the
//! same pinned release as a `.tgz` holding one signed `cloudflared` binary, so the
//! game downloads it with the system `curl`, checks the archive, and unpacks it with
//! the system `bsdtar`. `coop::tunnel` still checks the unpacked binary's size and
//! SHA-256 before it is cached or run.
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Child, Command, Stdio},
};

/// Release asset for `coop::tunnel::HELPER_VERSION` on Apple Silicon.
const ARCHIVE_URL: &str = "https://github.com/cloudflare/cloudflared/releases/download/2026.10.2/cloudflared-darwin-arm64.tgz";
/// GitHub's published digest and size for that asset.
const ARCHIVE_HASH: &str = "529a07089ac2d46762578389230fa148e5cb6ce2689200fc006956864042d17a";
const ARCHIVE_SIZE: u64 = 19_808_025;

/// Start downloading the helper archive to `partial`. The caller polls the child,
/// so cancelling and the time limit work as they do on Windows.
pub fn start_helper_download(partial: &Path) -> Result<Child> {
    Command::new("/usr/bin/curl")
        // HTTPS only, for GitHub's redirect to its download host as well.
        .args(["--proto", "=https", "--proto-redir", "=https", "--tlsv1.2"])
        .args(["--fail", "--silent", "--location", "--max-time", "115"])
        .args(["--max-filesize", &ARCHIVE_SIZE.to_string()])
        .arg("--output")
        .arg(partial)
        .arg(ARCHIVE_URL)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Could not start downloading the internet helper.")
}

/// Check the downloaded archive at `partial`, then replace it with the executable
/// it contains. Nothing from the archive is read until its hash matches.
pub fn unpack_helper(partial: &Path) -> Result<()> {
    ensure!(
        fs::metadata(partial)?.len() == ARCHIVE_SIZE && archive_hash(partial)? == ARCHIVE_HASH,
        "Internet helper verification failed. Do not run this file."
    );
    let unpacked = partial.with_extension("unpacked");
    let result = (|| {
        let output = File::create(&unpacked)?;
        // -O writes the one named member to stdout, so no other path is created.
        let status = Command::new("/usr/bin/bsdtar")
            .arg("-xOf")
            .arg(partial)
            .arg("cloudflared")
            .stdin(Stdio::null())
            .stdout(output)
            .stderr(Stdio::null())
            .status()
            .context("Could not unpack the internet helper.")?;
        ensure!(status.success(), "Could not unpack the internet helper.");
        fs::set_permissions(&unpacked, fs::Permissions::from_mode(0o755))?;
        fs::rename(&unpacked, partial)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&unpacked);
    }
    result
}

/// SHA-256 of the file at `path`, as lowercase hex.
fn archive_hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unexpected_archive_is_refused_before_unpacking() {
        let dir = std::env::temp_dir().join(format!("lg-coop-helper-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let partial = dir.join("download.partial");
        fs::write(&partial, b"not the release archive").unwrap();
        let error = unpack_helper(&partial).unwrap_err().to_string();
        assert!(error.contains("verification failed"));
        assert!(partial.exists() && !partial.with_extension("unpacked").exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
