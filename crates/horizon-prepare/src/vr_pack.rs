//! `horizon-prepare vr-pack`: everything Horizon VR's first start asks for, in one step.
//! It fetches the pinned server jar from Mojang when none is at hand, prepares the
//! resources from the player's own Minecraft (the official launcher's copy, played once),
//! and zips what only the Java exporters make into `horizon-derived.zip` beside this
//! program, for the headset's setup screen. The zip is derived from Mojang's game: it is
//! for the player's own headset, never to be shared.
use std::io::Write;
use std::path::{Path, PathBuf};

pub use horizon_prepare::derived_pack::zip_derived;

/// The pinned server jar, fetched from Mojang into `root` when it is not there yet
/// (`curl` ships with Windows 10 and later, macOS and Linux). Its SHA-1 is checked by the
/// preparation itself.
pub fn server_jar(root: &Path, sha1: &str, log: &dyn Fn(&str)) -> Result<PathBuf, String> {
    let jar = root.join("server.jar");
    if jar.is_file() {
        return Ok(jar);
    }
    let url = format!("https://piston-data.mojang.com/v1/objects/{sha1}/server.jar");
    log(&format!("Downloading the Minecraft server jar from Mojang ({url})"));
    let partial = root.join("server.jar.part");
    let status = std::process::Command::new("curl")
        .args(["-fL", "--retry", "3", "-o"])
        .arg(&partial)
        .arg(&url)
        .status()
        .map_err(|e| format!("curl is needed to download the server jar: {e}"))?;
    if !status.success() {
        return Err(format!("could not download {url} ({status})"));
    }
    std::fs::rename(&partial, &jar).map_err(|e| e.to_string())?;
    Ok(jar)
}

/// Mojang's own version file for `version` (`sha1`, the pinned one), saved where the
/// preparation looks before the launcher's: some launchers rewrite theirs.
pub fn version_file(root: &Path, version: &str, sha1: &str, log: &dyn Fn(&str)) -> Result<(), String> {
    let dir = root.join(".local").join("minecraft").join(version);
    let file = dir.join("manifest.json");
    if file.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let url = format!("https://piston-meta.mojang.com/v1/packages/{sha1}/{version}.json");
    log(&format!("Downloading Mojang's version file ({url})"));
    let status = std::process::Command::new("curl")
        .args(["-fsL", "--retry", "3", "-o"])
        .arg(&file)
        .arg(&url)
        .status()
        .map_err(|e| format!("curl is needed to download the version file: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&file);
        return Err(format!("could not download {url} ({status})"));
    }
    Ok(())
}

/// Where the headset's "Get it from my computer" button asks (UDP broadcast) and where
/// this program answers with the zip (TCP). Kept in step with SetupActivity.java.
pub const FIND_PORT: u16 = 47821;
pub const SEND_PORT: u16 = 47822;
const FIND: &[u8] = b"HORIZON-VR-FIND";

/// Offers `zip` to the headset on the home network until this program closes: a headset
/// asking on `FIND_PORT` hears back where to fetch it, and each connection to
/// `SEND_PORT` gets its length (8 bytes, big-endian) then its bytes. Only answers inside
/// the local network: the zip goes from the player's computer to the player's headset.
pub fn share(zip: &Path, log: &dyn Fn(&str)) -> Result<(), String> {
    let data = std::fs::read(zip).map_err(|e| format!("{}: {e}", zip.display()))?;
    let udp = std::net::UdpSocket::bind(("0.0.0.0", FIND_PORT)).map_err(|e| format!("port {FIND_PORT}: {e}"))?;
    let tcp = std::net::TcpListener::bind(("0.0.0.0", SEND_PORT)).map_err(|e| format!("port {SEND_PORT}: {e}"))?;
    std::thread::spawn(move || {
        let mut buf = [0u8; 64];
        while let Ok((n, from)) = udp.recv_from(&mut buf) {
            if &buf[..n] == FIND && local(from.ip()) {
                let _ = udp.send_to(format!("HORIZON-VR-HERE {SEND_PORT}").as_bytes(), from);
            }
        }
    });
    let sent = |ip: std::net::IpAddr| {
        println!("\nSent to the headset ({ip}) / Inviato al visore ({ip}) \u{2713}");
    };
    std::thread::spawn(move || {
        for stream in tcp.incoming().flatten() {
            let Ok(peer) = stream.peer_addr() else { continue };
            if !local(peer.ip()) {
                continue;
            }
            let mut stream = stream;
            let ok = stream
                .write_all(&(data.len() as u64).to_be_bytes())
                .and_then(|()| stream.write_all(&data))
                .and_then(|()| stream.flush());
            if ok.is_ok() {
                sent(peer.ip());
            }
        }
    });
    log("\nNow put on the headset: on Horizon VR's setup screen press \"Get it from my computer\".");
    log("Keep this window open until the headset has it. If Windows asks, allow access on private networks.");
    log("Ora nel visore: nella schermata di setup di Horizon VR premi \"Get it from my computer\".");
    log("Tieni aperta questa finestra finché il visore non l'ha ricevuto. Se Windows chiede, consenti l'accesso alle reti private.");
    Ok(())
}

/// A home network's address: private IPv4 ranges, link-local, or the computer itself.
fn local(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => v4.is_private() || v4.is_link_local() || v4.is_loopback(),
        std::net::IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xffc0) == 0xfe80,
    }
}

/// Waits for Enter, so a window opened by a double click stays to be read.
pub fn pause() {
    print!("\nPress Enter to close / Premi Invio per chiudere... ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}
