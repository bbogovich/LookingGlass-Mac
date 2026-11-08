// Runs the engine's setup commands (src/data_setup.rs) and decodes their JSON.
// All the data rules live in the engine; this file only starts it and reads lines.
import AppKit
import Foundation

struct Report: Decodable, Sendable, Equatable {
    var ok: Bool
    var base: String?
    var managed: Bool
    var language: String?
    var languages: [String]
    var maps: Int
    var problems: [String]
    var warnings: [String]

    static let empty = Report(ok: false, base: nil, managed: false, language: nil, languages: [],
                              maps: 0, problems: [], warnings: [])
}

struct LanguageInfo: Decodable, Sendable, Hashable {
    var code: String
    var name: String
}

struct SetupInfo: Decodable, Sendable {
    var archive_name: String
    var url: String
    var page: String
    var bytes: Int64
    var sha256: String
    var languages: [LanguageInfo]
}

private struct Line: Decodable {
    var progress: String?
    var percent: Int?
    var done: Bool?
    var report: Report?
    var error: String?
}

struct EngineError: LocalizedError {
    var message: String
    var errorDescription: String? { message }
}

/// Where the engine and the player's data folder are.
struct Engine: Sendable {
    let executable: URL
    let home: URL
    /// The app bundle that contains this helper, when installed (`Looking Glass.app`).
    let gameApp: URL?

    static func locate() throws -> Engine {
        let env = ProcessInfo.processInfo.environment
        let fm = FileManager.default
        // Installed: Looking Glass.app/Contents/Helpers/Looking Glass Setup.app
        let contents = Bundle.main.bundleURL.deletingLastPathComponent().deletingLastPathComponent()
        let bundledEngine = contents.appendingPathComponent("MacOS/looking-glass")
        let installed = contents.lastPathComponent == "Contents" && fm.isExecutableFile(atPath: bundledEngine.path)
        let executable: URL
        if let path = env["LOOKING_GLASS_ENGINE"], !path.isEmpty {
            executable = URL(fileURLWithPath: path)
        } else if installed {
            executable = bundledEngine
        } else {
            throw EngineError(message: "Cannot find the game. Set LOOKING_GLASS_ENGINE to target/release/looking-glass when running the helper on its own.")
        }
        // Same folder rule as the engine (src/platform/mod.rs, init_working_dir).
        let home: URL
        if let path = env["LOOKING_GLASS_HOME"], !path.isEmpty {
            home = URL(fileURLWithPath: path, isDirectory: true)
        } else if executable.path.contains(".app/Contents/MacOS/") {
            home = fm.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/Looking Glass", isDirectory: true)
        } else {
            home = URL(fileURLWithPath: fm.currentDirectoryPath, isDirectory: true)
        }
        try fm.createDirectory(at: home.appendingPathComponent("private"), withIntermediateDirectories: true)
        return Engine(executable: executable, home: home, gameApp: installed ? contents.deletingLastPathComponent() : nil)
    }

    var privateFolder: URL { home.appendingPathComponent("private", isDirectory: true) }

    private func process(_ arguments: [String]) -> (Process, Pipe) {
        let process = Process()
        process.executableURL = executable
        process.arguments = arguments
        process.currentDirectoryURL = home
        var env = ProcessInfo.processInfo.environment
        env["LOOKING_GLASS_HOME"] = home.path
        process.environment = env
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = Pipe()
        return (process, pipe)
    }

    /// Run a one-shot command (`--setup-info`, `--check-data`) and decode its output.
    func run<T: Decodable & Sendable>(_ arguments: [String], as type: T.Type) async throws -> T {
        let (process, pipe) = process(arguments)
        try process.run()
        let data = try await Task.detached { try pipe.fileHandleForReading.readToEnd() ?? Data() }.value
        process.waitUntilExit()
        do {
            return try JSONDecoder().decode(T.self, from: data)
        } catch {
            throw EngineError(message: "The game did not answer the setup check (exit \(process.terminationStatus)).")
        }
    }

    /// Run `--setup-data` / `--set-language`, reporting each progress line.
    func stream(_ arguments: [String], cancelFile: URL,
                progress: @escaping @Sendable @MainActor (String, Double?) -> Void) async throws -> Report {
        let (process, pipe) = process(arguments + ["--cancel-file", cancelFile.path])
        try process.run()
        var final: Line?
        for try await text in pipe.fileHandleForReading.bytes.lines {
            guard let line = try? JSONDecoder().decode(Line.self, from: Data(text.utf8)) else { continue }
            if let message = line.progress {
                await progress(message, line.percent.map { Double($0) / 100 })
            }
            if line.done != nil { final = line }
        }
        process.waitUntilExit()
        if let report = final?.report, final?.done == true { return report }
        throw EngineError(message: final?.error ?? "Setup stopped unexpectedly (exit \(process.terminationStatus)).")
    }

    /// Start the game itself and return immediately.
    func launchGame() throws {
        if let app = gameApp {
            let config = NSWorkspace.OpenConfiguration()
            config.createsNewApplicationInstance = false
            NSWorkspace.shared.openApplication(at: app, configuration: config)
            return
        }
        let (process, _) = process([])
        process.standardOutput = nil
        try process.run()
    }
}
