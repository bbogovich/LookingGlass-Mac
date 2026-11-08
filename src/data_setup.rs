//! Game-data check and import for platform setup front ends (fork: macOS port).
//!
//! The rules mirror `Setup.cmd` (the Windows setup), so every platform accepts the
//! same data and lays out `private/game-data` the same way. The macOS Setup helper
//! (`macos/SetupHelper`) is a thin UI over these commands; a Linux front end can
//! reuse them unchanged. Windows keeps its own PowerShell setup.
//!
//! Commands (the first argument; each prints JSON on stdout):
//! - `--setup-info`: download details and the supported languages.
//! - `--check-data [folder]`: inspect a folder (or the configured data). One object.
//! - `--setup-data <folder|archive> [--language <code>]`: import into
//!   `private/game-data`, apply the language, then point `private/data-path.txt` at
//!   it. Progress is one JSON object per line; the last line has `"done"`.
//! - `--set-language <code>`: switch the language of a managed installation.
//!
//! `--extractor <7zz>` and `--cancel-file <path>` apply to the last two. Cancelling
//! (creating the cancel file) leaves the previous configuration and saves untouched.
use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const ARCHIVE_NAME: &str = "Alice1_2011_vanilla.7z";
pub const ARCHIVE_URL: &str = "https://archive.org/download/alice_202106/Alice1_2011_vanilla.7z";
pub const ARCHIVE_PAGE: &str = "https://archive.org/details/alice_202106";
pub const ARCHIVE_BYTES: u64 = 977_811_158;
pub const ARCHIVE_SHA256: &str = "b253bb2c9c875f1838a711d0fe4f12f4bc2c6207db3baae6431af962eca8705d";

/// Base packs, as in `Setup.cmd`. The 2000 CD release has no `pak5_mod.pk3`; the
/// engine runs without it, so it is reported as a warning rather than refused.
const REQUIRED: [&str; 4] = ["pak0.pk3", "pak1_large.pk3", "pak2.pk3", "pak3.pk3"];
const MOD_PACK: &str = "pak5_mod.pk3";
const MAP_COUNT: usize = 36;
const SUFFIXES: [&str; 5] = ["", "base", "bin/base", "Alice1/bin/base", "alice_202106/Alice1/bin/base"];

struct Language {
    code: &'static str,
    name: &'static str,
    file: &'static str,
    bundle: Option<&'static str>,
    word: &'static str,
}
const LANGUAGES: [Language; 5] = [
    Language { code: "en", name: "English", file: "pak4_english.pk3", bundle: None, word: "step" },
    Language { code: "de", name: "Deutsch", file: "pak4_german.pk3", bundle: Some("base_g.pk3"), word: "Schritt" },
    Language { code: "fr", name: "Francais", file: "pak4_french.pk3", bundle: Some("base_f.pk3"), word: "pas" },
    Language { code: "es", name: "Espanol", file: "pak4_spanish.pk3", bundle: Some("base_s.pk3"), word: "paso" },
    Language { code: "nl", name: "Nederlands", file: "pak4_dutch.pk3", bundle: Some("base_d.pk3"), word: "Terug" },
];

fn language(code: &str) -> Result<&'static Language> {
    LANGUAGES.iter().find(|l| l.code == code).with_context(|| format!("Unknown language: {code}"))
}

fn is_language_file(name: &str) -> bool {
    LANGUAGES.iter().any(|l| name.eq_ignore_ascii_case(l.file) || l.bundle.is_some_and(|b| name.eq_ignore_ascii_case(b)))
}

/// Dutch translates menus only, so it also needs English dialog.
fn required_languages(code: &str) -> Vec<&'static str> {
    let mut codes = vec![language(code).map(|l| l.code).unwrap_or("en")];
    if code == "nl" {
        codes.push("en");
    }
    codes
}

/// Handle a setup command if the first argument is one. Returns true when handled.
pub fn command() -> Result<bool> {
    let mut args = std::env::args().skip(1);
    let Some(first) = args.next() else { return Ok(false) };
    if !matches!(first.as_str(), "--setup-info" | "--check-data" | "--setup-data" | "--set-language") {
        return Ok(false);
    }
    let mut positional = None;
    let mut opts = Options::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--language" => opts.language = Some(args.next().context("--language requires a code")?),
            "--extractor" => opts.extractor = Some(args.next().context("--extractor requires a path")?.into()),
            "--cancel-file" => opts.cancel = Some(args.next().context("--cancel-file requires a path")?.into()),
            _ if positional.is_none() => positional = Some(arg),
            _ => bail!("Unexpected argument: {arg}"),
        }
    }
    let root = std::env::current_dir()?;
    match first.as_str() {
        "--setup-info" => print_json(&info()),
        "--check-data" => {
            let report = match positional {
                Some(folder) => check_folder(Path::new(&folder)),
                None => check_configured(&root),
            };
            print_json(&report);
        }
        "--setup-data" => {
            let source = positional.context("--setup-data requires a folder or archive")?;
            finish(setup(&root, Path::new(&source), &opts));
        }
        _ => {
            let code = positional.context("--set-language requires a language code")?;
            finish(set_language(&root, &code, &opts));
        }
    }
    Ok(true)
}

#[derive(Default)]
struct Options {
    language: Option<String>,
    extractor: Option<PathBuf>,
    cancel: Option<PathBuf>,
}
impl Options {
    fn check_cancel(&self) -> Result<()> {
        if self.cancel.as_ref().is_some_and(|p| p.exists()) {
            bail!("Setup cancelled. Previous settings and saves were kept.");
        }
        Ok(())
    }
}

fn print_json(value: &impl Serialize) {
    println!("{}", serde_json::to_string(value).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}")));
}

fn progress(message: &str, percent: Option<u8>) {
    print_json(&serde_json::json!({ "progress": message, "percent": percent }));
    let _ = std::io::stdout().flush();
}

fn finish(result: Result<Report>) {
    match result {
        Ok(report) => print_json(&serde_json::json!({ "done": true, "report": report })),
        Err(error) => print_json(&serde_json::json!({ "done": false, "error": format!("{error:#}") })),
    }
}

#[derive(Serialize)]
struct Info {
    archive_name: &'static str,
    url: &'static str,
    page: &'static str,
    bytes: u64,
    sha256: &'static str,
    languages: Vec<serde_json::Value>,
}
fn info() -> Info {
    Info {
        archive_name: ARCHIVE_NAME,
        url: ARCHIVE_URL,
        page: ARCHIVE_PAGE,
        bytes: ARCHIVE_BYTES,
        sha256: ARCHIVE_SHA256,
        languages: LANGUAGES.iter().map(|l| serde_json::json!({ "code": l.code, "name": l.name })).collect(),
    }
}

/// What a folder holds, in a form the setup front ends display directly.
#[derive(Serialize, Debug, Default)]
pub struct Report {
    pub ok: bool,
    pub base: Option<String>,
    pub managed: bool,
    pub language: Option<&'static str>,
    pub languages: Vec<&'static str>,
    pub maps: usize,
    pub problems: Vec<String>,
    pub warnings: Vec<String>,
}

/// A file in `dir` whose name matches `name` ignoring ASCII case (the CD uses `Pak2.pk3`).
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() && exact.file_name().is_some_and(|n| n == name) {
        return Some(exact);
    }
    fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
        p.is_file() && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(name))
    })
}

fn has_core_packs(dir: &Path) -> bool {
    REQUIRED.iter().all(|name| find_file(dir, name).is_some())
}

/// The base folder inside `folder` (accepting `Alice1`, its parent, `bin/base`...).
fn find_base(folder: &Path, require_language: bool) -> Option<PathBuf> {
    SUFFIXES.iter().map(|s| if s.is_empty() { folder.to_path_buf() } else { folder.join(s) }).find(|c| {
        has_core_packs(c) && (!require_language || active_language(c, true).is_some())
    })
}

/// Whether a language pack really is `code`, using the same marker words as Setup.cmd.
fn check_language(path: &Path, code: &str) -> Result<()> {
    let lang = language(code)?;
    let entry_name = if code == "nl" { "ui/main.urc" } else { "dialog/standard.tlk" };
    let mut zip = zip::ZipArchive::new(File::open(path)?).with_context(|| format!("Invalid PK3: {}", path.display()))?;
    let name = zip
        .file_names()
        .find(|n| n.replace('\\', "/").eq_ignore_ascii_case(entry_name))
        .map(str::to_owned)
        .with_context(|| format!("{entry_name} is missing"))?;
    let mut text = Vec::new();
    zip.by_name(&name)?.take(16 << 20).read_to_end(&mut text)?;
    let found = text.split(|&b| b == b'\n').map(|l| l.strip_suffix(b"\r").unwrap_or(l)).any(|line| {
        if code == "nl" {
            dutch_title(line, lang.word)
        } else {
            let lead = line.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(line.len());
            !(line[lead..].starts_with(b"//") || line[lead..].starts_with(b"\"")) && has_word(line, lang.word)
        }
    });
    ensure!(found, "Language pack does not match {code}: expected '{}' in {entry_name}", lang.word);
    Ok(())
}

/// Whole-word, case-insensitive match. Bytes >= 0x80 count as letters (cp1252 text).
fn has_word(line: &[u8], word: &str) -> bool {
    let word = word.as_bytes();
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80;
    (0..line.len().saturating_sub(word.len() - 1)).any(|i| {
        line[i..i + word.len()].eq_ignore_ascii_case(word)
            && (i == 0 || !is_word(line[i - 1]))
            && line.get(i + word.len()).is_none_or(|&b| !is_word(b))
    })
}

/// `title "Terug"` (quotes and a trailing comment optional), case-insensitive.
fn dutch_title(line: &[u8], word: &str) -> bool {
    let Ok(line) = std::str::from_utf8(line) else { return false };
    let line = line.split("//").next().unwrap_or("").trim();
    let Some(rest) = line.get(..5).filter(|t| t.eq_ignore_ascii_case("title")).map(|_| &line[5..]) else {
        return false;
    };
    rest.starts_with(char::is_whitespace) && rest.trim().trim_matches('"').eq_ignore_ascii_case(word)
}

/// The language the engine will use for `base`, following Setup.cmd: the last
/// translated pack (by lowercase filename) wins, English only supplies dialog.
fn active_language(base: &Path, include_loc: bool) -> Option<&'static str> {
    let mut mounted: Vec<(String, &'static str)> = Vec::new();
    let mut fallback = None;
    for lang in &LANGUAGES {
        for name in std::iter::once(lang.file).chain(lang.bundle) {
            let mut places = vec![(base.to_path_buf(), true)];
            if include_loc && lang.bundle == Some(name) {
                places.push((base.join("loc"), false));
            }
            for (dir, in_base) in places {
                if find_file(&dir, name).is_some_and(|p| check_language(&p, lang.code).is_ok()) {
                    if in_base {
                        mounted.push((name.to_ascii_lowercase(), lang.code));
                    }
                    fallback.get_or_insert(lang.code);
                }
            }
        }
    }
    mounted.sort();
    mounted.iter().rev().find(|(_, c)| *c != "en").or(mounted.last()).map(|(_, c)| *c).or(fallback)
}

/// Structural check of a base folder: packs, the 36 maps, and the FAKK 42 format.
fn inspect(base: &Path, report: &mut Report) {
    let mut maps = std::collections::BTreeSet::new();
    let mut village = false;
    let mut packs: Vec<PathBuf> = Vec::new();
    for name in REQUIRED {
        match find_file(base, name) {
            Some(path) => packs.push(path),
            None => report.problems.push(format!("Missing {name}. Choose the Alice1 game folder or {ARCHIVE_NAME}.")),
        }
    }
    match find_file(base, MOD_PACK) {
        Some(path) => packs.push(path),
        None => report.warnings.push(format!(
            "{MOD_PACK} is missing (the original 2000 CD release). The game runs, but the 2011 data set is the tested one."
        )),
    }
    for path in packs {
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let mut zip = match File::open(&path).map_err(anyhow::Error::from).and_then(|f| Ok(zip::ZipArchive::new(f)?)) {
            Ok(zip) if zip.len() > 0 => zip,
            Ok(_) => {
                report.problems.push(format!("Empty game archive: {name}"));
                continue;
            }
            Err(e) => {
                report.problems.push(format!("Unreadable game archive {name}: {e}"));
                continue;
            }
        };
        let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
        for original in names {
            let entry = original.replace('\\', "/").to_ascii_lowercase();
            if entry.starts_with("maps/") && entry.ends_with(".bsp") && !entry[5..].contains('/') {
                if entry == "maps/gvillage.bsp" {
                    let mut header = [0u8; 8];
                    let ok = zip.by_name(&original).ok().is_some_and(|mut e| e.read_exact(&mut header).is_ok());
                    if ok && &header[..4] == b"FAKK" && i32::from_le_bytes(header[4..].try_into().unwrap()) == 42 {
                        village = true;
                    } else {
                        report.problems.push("The village map is not the supported FAKK version 42.".into());
                    }
                }
                maps.insert(entry);
            }
        }
    }
    report.maps = maps.len();
    if report.problems.is_empty() && (!village || maps.len() != MAP_COUNT) {
        report.problems.push(format!("Expected the {MAP_COUNT}-map data set; found {} maps.", maps.len()));
    }
}

/// Where a base folder's language packs can be: beside the core packs, in the 2011
/// `loc` folder, or in the `languages` folder next to it (an app bundle's data).
fn language_places(base: &Path) -> [PathBuf; 3] {
    let beside = base.parent().map_or_else(|| base.join("languages"), |p| p.join("languages"));
    [base.to_path_buf(), base.join("loc"), beside]
}

fn check_folder(folder: &Path) -> Report {
    let mut report = Report::default();
    let Some(base) = find_base(folder, false) else {
        report.problems.push(format!(
            "No game data found in {}. Choose the Alice1 folder, its bin/base folder, or {ARCHIVE_NAME}.",
            folder.display()
        ));
        return report;
    };
    let base = base.canonicalize().unwrap_or(base);
    inspect(&base, &mut report);
    report.languages = LANGUAGES
        .iter()
        .filter(|l| {
            std::iter::once(l.file)
                .chain(l.bundle)
                .flat_map(|n| language_places(&base).map(|dir| find_file(&dir, n)))
                .flatten()
                .any(|p| check_language(&p, l.code).is_ok())
        })
        .map(|l| l.code)
        .collect();
    report.language = active_language(&base, false);
    if report.language.is_none() {
        report.problems.push("Missing a supported language pack (pak4_english.pk3 or a translation).".into());
    }
    // Only an import in private/game-data can be changed in place; packs inside an
    // app bundle are read-only (changing them would break its signature).
    let parts: Vec<_> = base.components().map(|c| c.as_os_str().to_owned()).collect();
    report.managed = parts.windows(2).any(|w| w[0] == "private" && w[1] == "game-data");
    report.base = Some(base.display().to_string());
    report.ok = report.problems.is_empty();
    report
}

/// The configured data from `private/data-path.txt`, checked.
fn check_configured(root: &Path) -> Report {
    match configured_base(root) {
        Some(base) => {
            let mut report = check_folder(&base);
            // Languages cached in private/game-data/languages are switchable too.
            for lang in &LANGUAGES {
                let cached = root.join("private/game-data/languages").join(lang.file);
                if !report.languages.contains(&lang.code) && check_language(&cached, lang.code).is_ok() {
                    report.languages.push(lang.code);
                }
            }
            report
        }
        None => Report { problems: vec!["No game data is configured yet.".into()], ..Default::default() },
    }
}

fn configured_base(root: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(root.join("private/data-path.txt")).ok()?;
    let path = PathBuf::from(text.trim());
    (!text.trim().is_empty()).then(|| if path.is_absolute() { path } else { root.join(path) })
}

/// True when `ok` data is configured: what the macOS launcher checks before playing.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn configured_ok(root: &Path) -> bool {
    configured_base(root).is_some_and(|base| has_core_packs(&base) && active_language(&base, false).is_some())
}

fn unique(dir: &Path, prefix: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    dir.join(format!("{prefix}{:x}{:x}", nanos, std::process::id()))
}

/// Write `text` to `path` through a temporary file and rename, so readers never see half a file.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn save_text(path: &Path, text: &str) -> Result<()> {
    let temp = unique(path.parent().context("no parent folder")?, ".tmp-");
    fs::write(&temp, text)?;
    fs::rename(&temp, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })?;
    Ok(())
}

fn sha256_file(path: &Path, opts: &Options) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = File::open(path)?;
    let total = file.metadata()?.len().max(1);
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    let (mut done, mut last) = (0u64, std::time::Instant::now());
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        done += n as u64;
        if last.elapsed().as_millis() > 300 {
            opts.check_cancel()?;
            progress("Checking the archive...", Some((done * 100 / total) as u8));
            last = std::time::Instant::now();
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// What reads .7z archives: the system `bsdtar` (libarchive, the engine behind
/// Archive Utility; Linux has it in libarchive-tools), else 7-Zip when installed.
enum Tool {
    Bsdtar(PathBuf),
    SevenZip(PathBuf),
}

fn tool(opts: &Options) -> Result<Tool> {
    if let Some(path) = &opts.extractor {
        let tar = path.file_name().is_some_and(|n| n.to_string_lossy().contains("tar"));
        return Ok(if tar { Tool::Bsdtar(path.clone()) } else { Tool::SevenZip(path.clone()) });
    }
    let dirs: Vec<PathBuf> = std::iter::once(PathBuf::from("/usr/bin"))
        .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()))
        .collect();
    for name in ["bsdtar", "7zz", "7z"] {
        if let Some(path) = dirs.iter().map(|d| d.join(name)).find(|p| p.is_file()) {
            return Ok(if name == "bsdtar" { Tool::Bsdtar(path) } else { Tool::SevenZip(path) });
        }
    }
    bail!("Cannot read .7z archives here (no bsdtar or 7-Zip found). Extract the archive and choose its folder instead.")
}

/// The first `n` whitespace-separated fields of `line`, and the rest (a name may hold spaces).
fn fields(line: &str, n: usize) -> Option<(Vec<&str>, &str)> {
    let mut rest = line.trim_start();
    let mut out = Vec::new();
    for _ in 0..n {
        let end = rest.find(char::is_whitespace)?;
        out.push(&rest[..end]);
        rest = rest[end..].trim_start();
    }
    Some((out, rest))
}

/// Archive entries with their unpacked sizes.
fn archive_entries(source: &Path, zip_archive: bool, opts: &Options) -> Result<Vec<(String, u64)>> {
    if zip_archive {
        let mut zip = zip::ZipArchive::new(File::open(source)?)?;
        return (0..zip.len()).map(|i| zip.by_index_raw(i).map(|e| (e.name().to_owned(), e.size())).map_err(Into::into)).collect();
    }
    let (program, args): (PathBuf, &[&str]) = match tool(opts)? {
        Tool::Bsdtar(path) => (path, &["-tvf"]),
        Tool::SevenZip(path) => (path, &["l", "-slt", "-ba", "--"]),
    };
    let tar = program.file_name().is_some_and(|n| n.to_string_lossy().contains("tar"));
    let out = Command::new(&program).args(args).arg(source).output().context("list the archive")?;
    ensure!(out.status.success(), "Cannot list archive: {}", String::from_utf8_lossy(&out.stderr).trim());
    let text = String::from_utf8_lossy(&out.stdout);
    if tar {
        // drwxr-xr-x  0 0      0   289276327 Jun 15  2011 Alice1/bin/base/pak0.pk3
        return Ok(text
            .lines()
            .filter_map(|l| fields(l, 8))
            .filter(|(f, _)| !f[0].starts_with('d'))
            .map(|(f, name)| (name.to_owned(), f[4].parse().unwrap_or(0)))
            .collect());
    }
    let mut entries = Vec::new();
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("Path = ") {
            entries.push((path.to_owned(), 0));
        } else if let (Some(size), Some(last)) = (line.strip_prefix("Size = "), entries.last_mut()) {
            last.1 = size.trim().parse().unwrap_or(0);
        }
    }
    Ok(entries)
}

/// Extract only the packs setup needs, flat into `dest`. Exact entry names and flat
/// extraction exclude executables, scripts and traversal paths, as on Windows.
fn extract(source: &Path, dest: &Path, opts: &Options) -> Result<()> {
    let zip_archive = source.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    let entries = archive_entries(source, zip_archive, opts)?;
    let find = |name: &str| entries.iter().find(|(e, _)| e.replace('\\', "/").eq_ignore_ascii_case(name)).cloned();
    let prefix = ["", "base/", "bin/base/", "Alice1/bin/base/", "alice_202106/Alice1/bin/base/"]
        .into_iter()
        .find(|p| find(&format!("{p}pak0.pk3")).is_some())
        .context("No supported base folder in the archive.")?;
    let mut wanted = Vec::new();
    for name in REQUIRED.iter().copied().chain([MOD_PACK]).chain(LANGUAGES.iter().map(|l| l.file)) {
        wanted.extend(find(&format!("{prefix}{name}")));
    }
    for name in LANGUAGES.iter().filter_map(|l| l.bundle) {
        wanted.extend(find(&format!("{prefix}{name}")).or_else(|| find(&format!("{prefix}loc/{name}"))));
    }
    let flat = |name: &str| -> Result<std::ffi::OsString> {
        Ok(Path::new(&name.replace('\\', "/")).file_name().context("bad archive entry")?.to_owned())
    };
    progress("Unpacking game files. This may take a few minutes...", None);
    if zip_archive {
        let mut zip = zip::ZipArchive::new(File::open(source)?)?;
        for (name, _) in &wanted {
            opts.check_cancel()?;
            let mut out = File::create(dest.join(flat(name)?))?;
            std::io::copy(&mut zip.by_name(name)?, &mut out)?;
        }
        return Ok(());
    }
    // Unpack the exact entries into a staging folder, reporting progress from the
    // bytes written so far, then move each file up flat.
    let staging = unique(dest, ".unpack-");
    fs::create_dir_all(&staging)?;
    let names: Vec<&str> = wanted.iter().map(|(n, _)| n.as_str()).collect();
    let mut command = match tool(opts)? {
        Tool::Bsdtar(path) => {
            let mut c = Command::new(path);
            c.arg("-xf").arg(source).arg("-C").arg(&staging).args(&names);
            c
        }
        Tool::SevenZip(path) => {
            let mut c = Command::new(path);
            c.args(["x", "-y", "-bso0", "-bsp0"]).arg(format!("-o{}", staging.display())).arg("--").arg(source).args(&names);
            c
        }
    };
    let result = (|| -> Result<()> {
        let mut child = command.stdout(Stdio::null()).stderr(Stdio::piped()).spawn().context("start the archive tool")?;
        let total = wanted.iter().map(|(_, size)| size).sum::<u64>().max(1);
        let mut last = None;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if opts.check_cancel().is_err() {
                let _ = child.kill();
                let _ = child.wait();
                opts.check_cancel()?;
            }
            let done: u64 = names.iter().filter_map(|n| fs::metadata(staging.join(n)).ok()).map(|m| m.len()).sum();
            let pct = Some((done * 100 / total).min(100) as u8);
            if pct != last {
                progress("Unpacking game files...", pct);
                last = pct;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        };
        let mut errors = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            let _ = stderr.read_to_string(&mut errors);
        }
        ensure!(
            status.success(),
            "Unpacking failed ({}). Check the archive and free disk space, then try again.",
            errors.trim()
        );
        for name in &names {
            fs::rename(staging.join(name), dest.join(flat(name)?))
                .with_context(|| format!("{name} was not unpacked"))?;
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

/// Import `source` (a game folder or a .7z/.zip archive) into `private/game-data`.
fn setup(root: &Path, source: &Path, opts: &Options) -> Result<Report> {
    let source = source.canonicalize().with_context(|| format!("Cannot open {}", source.display()))?;
    let game_data = root.join("private/game-data");
    let cache = game_data.join("languages");
    fs::create_dir_all(&cache)?;
    let import = unique(&game_data, "import-").join("base");
    let result = (|| {
        let staged = if source.is_dir() {
            find_base(&source, false).context(
                "No complete game data in that folder. Choose the Alice1 folder or its bin/base folder.",
            )?
        } else {
            let ext = source.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
            ensure!(ext == "7z" || ext == "zip", "Choose a .7z or .zip archive, or an extracted game folder.");
            let size = fs::metadata(&source)?.len();
            if size == ARCHIVE_BYTES {
                ensure!(
                    sha256_file(&source, opts)? == ARCHIVE_SHA256,
                    "This archive did not pass its integrity check. Download it again or choose a folder."
                );
            }
            fs::create_dir_all(&import)?;
            extract(&source, &import, opts)?;
            import.clone()
        };
        opts.check_cancel()?;
        progress("Validating game files...", None);
        let mut report = Report::default();
        inspect(&staged, &mut report);
        ensure!(report.problems.is_empty(), "{}", report.problems.join(" "));

        // Keep every valid language pack in the cache, under its canonical name.
        for lang in &LANGUAGES {
            let dest = cache.join(lang.file);
            if dest.exists() {
                continue;
            }
            let found = std::iter::once(lang.file)
                .chain(lang.bundle)
                .flat_map(|n| language_places(&staged).map(|dir| find_file(&dir, n)))
                .flatten()
                .find(|p| check_language(p, lang.code).is_ok());
            if let Some(path) = found {
                let temp = unique(&cache, ".tmp-");
                fs::copy(&path, &temp)?;
                fs::rename(&temp, &dest)?;
            }
        }
        let code = match &opts.language {
            Some(code) => language(code)?.code,
            // A folder keeps the language it was installed with. An archive holds
            // every translation flat, where the last one alphabetically (Spanish)
            // would win, so it starts in English unless a language is chosen.
            None if staged == import && check_language(&cache.join(LANGUAGES[0].file), "en").is_ok() => "en",
            None => active_language(&staged, true).context("Missing a supported language pack.")?,
        };
        opts.check_cancel()?;

        // Copy a folder source's core packs (APFS clones them instantly); an
        // extracted archive already lives in its import folder.
        if staged != import {
            progress("Copying game files...", None);
            fs::create_dir_all(&import)?;
            for entry in fs::read_dir(&staged)?.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.to_ascii_lowercase().ends_with(".pk3") && !is_language_file(&name) && entry.path().is_file() {
                    opts.check_cancel()?;
                    fs::copy(entry.path(), import.join(&name))?;
                }
            }
        }
        apply_language(&import, &cache, code)?;
        let mut report = check_folder(&import);
        ensure!(report.ok, "{}", report.problems.join(" "));
        opts.check_cancel()?;

        // Commit: point the game at the new import, then drop the previous import.
        let previous = configured_base(root).filter(|b| b.starts_with(&game_data));
        let relative = import.strip_prefix(root).unwrap_or(&import).display().to_string();
        let receipt = serde_json::json!({ "schema": 1, "source": source.display().to_string(), "dataPath": relative });
        save_text(&root.join("private/setup.json"), &format!("{receipt:#}\n"))?;
        save_text(&root.join("private/data-path.txt"), &format!("{relative}\n"))?;
        if let Some(old) = previous.and_then(|b| b.parent().map(Path::to_path_buf)) {
            if old != import.parent().unwrap_or(&import) && old.starts_with(&game_data) {
                let _ = fs::remove_dir_all(old);
            }
        }
        for lang in &LANGUAGES {
            if !report.languages.contains(&lang.code) && check_language(&cache.join(lang.file), lang.code).is_ok() {
                report.languages.push(lang.code);
            }
        }
        Ok(report)
    })();
    if result.is_err() {
        if let Some(dir) = import.parent() {
            let _ = fs::remove_dir_all(dir);
        }
    }
    result
}

/// Replace the language packs in a managed `base` with `code` from the cache.
/// Every target is prepared before any active pack moves, and a failure restores them.
fn apply_language(base: &Path, cache: &Path, code: &str) -> Result<()> {
    let txn = unique(base, ".language-");
    fs::create_dir_all(&txn)?;
    let mut prepared = Vec::new();
    for need in required_languages(code) {
        let lang = language(need)?;
        let cached = cache.join(lang.file);
        check_language(&cached, need)
            .with_context(|| format!("The {} language pack is not available. Choose a source that contains it.", lang.name))?;
        let temp = txn.join(format!("selected-{need}.tmp"));
        fs::copy(&cached, &temp)?;
        prepared.push((temp, base.join(lang.file)));
    }
    let mut moved = Vec::new();
    let mut installed = Vec::new();
    let result = (|| -> Result<()> {
        for entry in fs::read_dir(base)?.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_language_file(&name) && entry.path().is_file() {
                fs::rename(entry.path(), txn.join(&name))?;
                moved.push(name);
            }
        }
        for (temp, dest) in &prepared {
            fs::rename(temp, dest)?;
            installed.push(dest.clone());
        }
        Ok(())
    })();
    if result.is_err() {
        for path in &installed {
            let _ = fs::remove_file(path);
        }
        for name in &moved {
            let _ = fs::rename(txn.join(name), base.join(name));
        }
    }
    let _ = fs::remove_dir_all(&txn);
    result
}

fn set_language(root: &Path, code: &str, opts: &Options) -> Result<Report> {
    let code = language(code)?.code;
    let base = configured_base(root).context("No game data is configured yet. Set it up first.")?;
    let game_data = root.join("private/game-data");
    ensure!(
        base.starts_with(&game_data),
        "This installation uses an outside folder. Import it with Setup first, then change the language."
    );
    ensure!(active_language(&base, false) != Some(code), "That language is already active.");
    opts.check_cancel()?;
    let cache = game_data.join("languages");
    // Cache whatever is mounted now, so switching back never needs the source again.
    for lang in &LANGUAGES {
        let dest = cache.join(lang.file);
        if !dest.exists() {
            let found = std::iter::once(lang.file)
                .chain(lang.bundle)
                .filter_map(|n| find_file(&base, n))
                .find(|p| check_language(p, lang.code).is_ok());
            if let Some(path) = found {
                fs::create_dir_all(&cache)?;
                fs::copy(path, dest)?;
            }
        }
    }
    progress("Changing language...", None);
    apply_language(&base, &cache, code)?;
    Ok(check_configured(root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::FileOptions;

    fn pack(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, data) in files {
            zip.start_file(*name, FileOptions::default().compression_method(zip::CompressionMethod::Stored)).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    /// A synthetic 36-map data set (no game content), like tools/test_setup.py.
    fn make_data(root: &Path, with_mod: bool) -> PathBuf {
        let base = root.join("Alice1/bin/base");
        fs::create_dir_all(&base).unwrap();
        let mut header = b"FAKK".to_vec();
        header.extend(42i32.to_le_bytes());
        let mut maps: Vec<(String, Vec<u8>)> = (0..35).map(|i| (format!("maps/m{i}.bsp"), vec![0])).collect();
        maps.push(("maps/gvillage.bsp".into(), header));
        let files: Vec<(&str, &[u8])> = maps.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
        pack(&base.join("pak0.pk3"), &files);
        for name in ["pak1_large.pk3", "Pak2.pk3", "Pak3.pk3"] {
            pack(&base.join(name), &[("x.txt", b"synthetic")]);
        }
        if with_mod {
            pack(&base.join("pak5_mod.pk3"), &[("x.txt", b"synthetic")]);
        }
        pack(&base.join("pak4_english.pk3"), &[("dialog/standard.tlk", b"TALK\n\"sound/step.wav\"\nA first step.\n")]);
        base
    }

    fn temp(name: &str) -> PathBuf {
        let dir = unique(&std::env::temp_dir(), &format!("lg-setup-{name}-"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn language_markers_match_setup_cmd() {
        assert!(has_word(b"Every adventure needs a first step.", "step"));
        assert!(!has_word(b"footsteps", "step"));
        assert!(has_word(b"Un paso", "paso"));
        assert!(dutch_title(b"  title \"Terug\" // back", "Terug"));
        assert!(dutch_title(b"TITLE Terug", "Terug"));
        assert!(!dutch_title(b"subtitle Terug", "Terug"));
    }

    #[test]
    fn bsdtar_listings_keep_names_with_spaces() {
        let (f, name) = fields("-rw-r--r--  0 0      0   289276327 Jun 15  2011 Alice1/bin/base/My Pack.pk3", 8).unwrap();
        assert_eq!((f[0], f[4], name), ("-rw-r--r--", "289276327", "Alice1/bin/base/My Pack.pk3"));
        assert!(fields("too short", 8).is_none());
    }

    #[test]
    fn cd_layout_is_found_checked_and_imported() {
        let root = temp("import");
        let data = temp("cd");
        make_data(&data, false);
        let report = check_folder(&data);
        assert!(report.ok, "{report:?}");
        assert_eq!(report.maps, 36);
        assert_eq!(report.language, Some("en"));
        assert_eq!(report.warnings.len(), 1, "missing pak5_mod is only a warning");

        let report = setup(&root, &data, &Options::default()).unwrap();
        assert!(report.ok && report.managed, "{report:?}");
        assert!(configured_ok(&root));
        let base = configured_base(&root).unwrap();
        assert!(find_file(&base, "pak2.pk3").is_some());
        assert!(root.join("private/game-data/languages/pak4_english.pk3").is_file());
        assert_eq!(set_language(&root, "en", &Options::default()).unwrap_err().to_string(), "That language is already active.");
        // A second import replaces the first and removes it.
        let first = base.parent().unwrap().to_path_buf();
        setup(&root, &data, &Options::default()).unwrap();
        assert!(!first.exists());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(data);
    }

    #[test]
    fn language_switch_needs_the_pack_and_keeps_english_for_dutch() {
        let root = temp("lang");
        let data = temp("lang-src");
        let base = make_data(&data, true);
        pack(&base.join("pak4_dutch.pk3"), &[("ui/main.urc", b"title \"Terug\"\n")]);
        let report = setup(&root, &data, &Options { language: Some("en".into()), ..Default::default() }).unwrap();
        assert_eq!(report.language, Some("en"));
        assert!(report.languages.contains(&"nl"));
        assert!(set_language(&root, "de", &Options::default()).is_err());
        let report = set_language(&root, "nl", &Options::default()).unwrap();
        assert_eq!(report.language, Some("nl"));
        let managed = configured_base(&root).unwrap();
        assert!(managed.join("pak4_english.pk3").is_file() && managed.join("pak4_dutch.pk3").is_file());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(data);
    }

    #[test]
    fn bundled_data_is_read_only_but_its_languages_can_be_imported() {
        let root = temp("bundle");
        let bundle = temp("bundle-src").join("Contents/Resources/game-data");
        let source = temp("bundle-data");
        let base = make_data(&source, true);
        fs::create_dir_all(bundle.join("languages")).unwrap();
        fs::rename(&base, bundle.join("base")).unwrap();
        pack(&bundle.join("languages/pak4_dutch.pk3"), &[("ui/main.urc", b"title \"Terug\"\n")]);
        let report = check_folder(&bundle.join("base"));
        assert!(report.ok && !report.managed, "{report:?}");
        assert!(report.languages.contains(&"nl"), "{report:?}");
        let options = Options { language: Some("nl".into()), ..Default::default() };
        let report = setup(&root, &bundle.join("base"), &options).unwrap();
        assert!(report.managed && report.language == Some("nl"), "{report:?}");
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(source);
    }

    #[test]
    fn zip_archives_import_only_the_game_packs() {
        let root = temp("zip");
        let data = temp("zip-src");
        let base = make_data(&data, true);
        let archive = data.join("Alice1.zip");
        let mut files: Vec<(String, Vec<u8>)> = fs::read_dir(&base)
            .unwrap()
            .map(|e| e.unwrap().path())
            .map(|p| (format!("Alice1/bin/base/{}", p.file_name().unwrap().to_string_lossy()), fs::read(&p).unwrap()))
            .collect();
        files.push(("Alice1/bin/alice.exe".into(), b"not imported".to_vec()));
        files.push(("Alice1/bin/base/../../evil.pk3".into(), b"not imported".to_vec()));
        pack(&archive, &files.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect::<Vec<_>>());
        let report = setup(&root, &archive, &Options::default()).unwrap();
        assert!(report.ok, "{report:?}");
        let mut names: Vec<_> = fs::read_dir(configured_base(&root).unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_ascii_lowercase())
            .collect();
        names.sort();
        assert_eq!(names, ["pak0.pk3", "pak1_large.pk3", "pak2.pk3", "pak3.pk3", "pak4_english.pk3", "pak5_mod.pk3"]);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(data);
    }

    #[test]
    fn incomplete_folders_and_cancellation_leave_the_configuration_alone() {
        let root = temp("cancel");
        let data = temp("cancel-src");
        let base = make_data(&data, true);
        fs::remove_file(base.join("pak0.pk3")).unwrap();
        assert!(!check_folder(&data).ok);
        assert!(setup(&root, &data, &Options::default()).is_err());
        let base = make_data(&data, true);
        let cancel = root.join("cancel");
        fs::write(&cancel, "").unwrap();
        assert!(setup(&root, &base, &Options { cancel: Some(cancel), ..Default::default() }).is_err());
        assert!(!root.join("private/data-path.txt").exists());
        let leftovers = fs::read_dir(root.join("private/game-data")).unwrap().filter(|e| {
            e.as_ref().unwrap().file_name().to_string_lossy().starts_with("import-")
        });
        assert_eq!(leftovers.count(), 0);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(data);
    }
}
