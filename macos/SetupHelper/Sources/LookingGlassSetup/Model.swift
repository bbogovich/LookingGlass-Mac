// State and actions for the Setup window. Data rules stay in the engine (Engine.swift).
import AppKit
import Observation

@MainActor @Observable
final class Model {
    var engine: Engine?
    var info: SetupInfo?
    var report = Report.empty
    var language = "en"
    /// The player picked a language, so an import should install it (otherwise the
    /// engine keeps a folder's language, or uses English for an archive).
    var languageChosen = false
    var busy = false
    var message = ""
    var fraction: Double?
    var error: String?
    var notice: String?
    var found: [URL] = []
    /// Opened by the game because no data was configured: Play just returns to it.
    let fromGame = CommandLine.arguments.contains("--from-game")
    private var cancelFile: URL?
    /// Run once the current work has finished and cleaned up.
    private var idle: [() -> Void] = []

    func whenIdle(_ action: @escaping () -> Void) {
        if busy { idle.append(action) } else { action() }
    }

    func start() async {
        do {
            let engine = try Engine.locate()
            self.engine = engine
            info = try await engine.run(["--setup-info"], as: SetupInfo.self)
            await refresh()
            findSources()
        } catch {
            self.error = error.localizedDescription
        }
    }

    func refresh() async {
        guard let engine else { return }
        do {
            report = try await engine.run(["--check-data"], as: Report.self)
            if let active = report.language { language = active }
        } catch {
            self.error = error.localizedDescription
        }
    }

    /// Local copies to offer before any download: the archive in Downloads, an earlier
    /// download, or an Alice1 folder or archive beside the app.
    func findSources() {
        guard let engine, let info else { return }
        let fm = FileManager.default
        var places = [
            fm.homeDirectoryForCurrentUser.appendingPathComponent("Downloads/\(info.archive_name)"),
            engine.privateFolder.appendingPathComponent("downloads/\(info.archive_name)"),
        ]
        if let beside = engine.gameApp?.deletingLastPathComponent() {
            places += [beside.appendingPathComponent(info.archive_name), beside.appendingPathComponent("Alice1")]
        }
        found = places.filter { fm.fileExists(atPath: $0.path) }
    }

    var languages: [LanguageInfo] { info?.languages ?? [] }

    /// Where imports go, for display while one runs.
    var destination: String {
        Model.display(engine?.privateFolder.appendingPathComponent("game-data").path)
    }

    /// A path with the home folder shortened to ~.
    static func display(_ path: String?) -> String {
        (path ?? "your Looking Glass folder").replacingOccurrences(of: NSHomeDirectory(), with: "~")
    }

    /// Open Finder at the game data (it lives in the hidden Library folder).
    func showData() {
        guard let base = report.base else { return }
        NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: base)
    }

    private var languageArguments: [String] { languageChosen ? ["--language", language] : [] }

    var canApplyLanguage: Bool {
        !busy && report.ok && report.language != language && report.languages.contains(language)
    }

    func chooseFolder() {
        let panel = NSOpenPanel()
        panel.message = "Choose the Alice1 folder (or its bin/base folder) from your game."
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        if panel.runModal() == .OK, let url = panel.url { importSource(url) }
    }

    func chooseArchive() {
        let panel = NSOpenPanel()
        panel.message = "Choose \(info?.archive_name ?? "the game archive") or a .zip of the game folder."
        panel.canChooseDirectories = false
        panel.allowedContentTypes = [.init(filenameExtension: "7z")!, .zip]
        if panel.runModal() == .OK, let url = panel.url { importSource(url) }
    }

    func importSource(_ url: URL) {
        work { engine, cancel in
            let report = try await engine.stream(["--setup-data", url.path] + self.languageArguments, cancelFile: cancel) { text, value in
                self.message = text
                self.fraction = value
            }
            self.notice = "Imported \(report.maps) maps to \(Model.display(report.base))."
        }
    }

    /// Ask for consent on every download start, defaulting to No, as on Windows.
    func download() {
        guard let info else { return }
        let alert = NSAlert()
        alert.messageText = "Download the game files from Internet Archive?"
        alert.informativeText = """
            Only download game data you are entitled to use, such as a copy of American McGee's Alice you own. \
            The file is \(info.archive_name) (\(info.bytes / 1_048_576) MB) from \(info.page).
            """
        alert.addButton(withTitle: "Don't Download")
        alert.addButton(withTitle: "I'm Entitled, Download")
        guard alert.runModal() == .alertSecondButtonReturn else { return }
        work { engine, cancel in
            let cancelPath = cancel.path
            let archive = try await Downloader.fetch(info, into: engine.privateFolder.appendingPathComponent("downloads"),
                                                     isCancelled: { FileManager.default.fileExists(atPath: cancelPath) }) { text, value in
                Task { @MainActor in
                    self.message = text
                    self.fraction = value
                }
            }
            let report = try await engine.stream(["--setup-data", archive.path] + self.languageArguments, cancelFile: cancel) { text, value in
                self.message = text
                self.fraction = value
            }
            self.notice = "Imported \(report.maps) maps to \(Model.display(report.base))."
        }
    }

    func applyLanguage() {
        let code = language
        // Data built into the app is read-only: changing language imports a copy
        // (an instant clone on the same disk) into the player's folder.
        let arguments = report.managed || report.base == nil
            ? ["--set-language", code]
            : ["--setup-data", report.base ?? "", "--language", code]
        work { engine, cancel in
            _ = try await engine.stream(arguments, cancelFile: cancel) { text, value in
                self.message = text
                self.fraction = value
            }
            self.notice = "Language changed."
        }
    }

    func cancel() {
        guard let cancelFile else { return }
        FileManager.default.createFile(atPath: cancelFile.path, contents: nil)
        message = "Cancelling..."
    }

    func play() {
        if !fromGame {
            do { try engine?.launchGame() } catch { self.error = error.localizedDescription; return }
        }
        NSApp.terminate(nil)
    }

    private func work(_ body: @escaping @MainActor (Engine, URL) async throws -> Void) {
        guard let engine, !busy else { return }
        busy = true
        error = nil
        notice = nil
        message = "Starting..."
        fraction = nil
        let cancel = FileManager.default.temporaryDirectory.appendingPathComponent("lg-setup-cancel-\(UUID().uuidString)")
        cancelFile = cancel
        Task {
            do { try await body(engine, cancel) } catch { self.error = error.localizedDescription }
            if self.error == nil { languageChosen = false }
            try? FileManager.default.removeItem(at: cancel)
            cancelFile = nil
            busy = false
            let waiting = idle
            idle = []
            waiting.forEach { $0() }
            await refresh()
            findSources()
        }
    }
}
