// Looking Glass Setup: finds or imports the game data and picks the language.
// The macOS counterpart of Setup.cmd; the rules themselves run in the engine.
import SwiftUI

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        // Also correct when run as a bare executable during development.
        NSApp.setActivationPolicy(.regular)
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}

@main
struct SetupApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var model = Model()

    var body: some Scene {
        Window("Looking Glass Setup", id: "setup") {
            SetupView(model: model)
                .frame(width: 560)
                .task {
                    bringToFront()
                    await model.start()
                }
        }
        .windowResizability(.contentSize)
    }
}

/// The game opens Setup from the background, where macOS will not activate a new app on
/// request alone. Ordering the window explicitly keeps it above other apps' windows.
@MainActor
private func bringToFront() {
    NSApp.activate()
    for window in NSApp.windows {
        window.makeKeyAndOrderFront(nil)
        window.orderFrontRegardless()
    }
}

struct SetupView: View {
    @Bindable var model: Model
    @State private var dropping = false

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            status
            Divider()
            source
            Divider()
            languagePicker
            if model.busy { progress }
            if let error = model.error {
                Label(error, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
                    .textSelection(.enabled)
            }
            if let notice = model.notice {
                Label(notice, systemImage: "checkmark.circle.fill").foregroundStyle(.green)
            }
            HStack {
                Spacer()
                Button("Quit") { NSApp.terminate(nil) }
                    .keyboardShortcut(.cancelAction)
                Button("Play") { model.play() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!model.report.ok || model.busy)
            }
        }
        .padding(24)
        .overlay {
            if dropping {
                RoundedRectangle(cornerRadius: 12).stroke(.tint, lineWidth: 3).padding(4)
            }
        }
        .dropDestination(for: URL.self) { urls, _ in
            guard let url = urls.first, !model.busy else { return false }
            model.importSource(url)
            return true
        } isTargeted: { dropping = $0 }
    }

    private var status: some View {
        HStack(alignment: .top, spacing: 14) {
            Image(systemName: model.report.ok ? "checkmark.seal.fill" : "questionmark.folder.fill")
                .font(.system(size: 34))
                .foregroundStyle(model.report.ok ? .green : .secondary)
            VStack(alignment: .leading, spacing: 4) {
                Text(model.report.ok ? "Game data is ready" : "Game data is needed")
                    .font(.title2.bold())
                if model.report.ok, let base = model.report.base {
                    Text(Model.display(base)).font(.caption).foregroundStyle(.secondary).textSelection(.enabled).lineLimit(3)
                    Button("Show in Finder") { model.showData() }
                        .buttonStyle(.link)
                        .font(.caption)
                    Text("\(model.report.maps) maps · \(name(model.report.language))")
                        .font(.callout)
                } else {
                    Text("Looking Glass plays American McGee's Alice using your own copy of the game files.")
                        .foregroundStyle(.secondary)
                    ForEach(model.report.problems.filter { !$0.hasPrefix("No game data is configured") }, id: \.self) {
                        Text($0).font(.callout).foregroundStyle(.red)
                    }
                }
                ForEach(model.report.warnings, id: \.self) {
                    Text($0).font(.caption).foregroundStyle(.orange)
                }
            }
        }
    }

    private var source: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(model.report.ok ? "Use different game files" : "Where are your game files?").font(.headline)
            ForEach(model.found, id: \.self) { url in
                HStack {
                    Image(systemName: "doc.zipper")
                    Text(url.lastPathComponent).bold()
                    Text("in \(url.deletingLastPathComponent().lastPathComponent)").foregroundStyle(.secondary)
                    Spacer()
                    Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting([url]) }
                        .buttonStyle(.link)
                    Button("Use") { model.importSource(url) }
                }
            }
            HStack {
                Button("Choose Folder…") { model.chooseFolder() }
                Button("Choose Archive…") { model.chooseArchive() }
                Spacer()
                Button("Download…") { model.download() }
            }
            .disabled(model.busy || model.engine == nil)
            Text("Accepts the Alice1 folder, its bin/base folder, or the 2011 archive. You can also drop one on this window. Files are copied into your Looking Glass folder; the originals are never changed.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var languagePicker: some View {
        HStack {
            Picker("Language", selection: Binding(get: { model.language },
                                                  set: { model.language = $0; model.languageChosen = true })) {
                ForEach(model.languages, id: \.code) { lang in
                    Text(lang.name + (!model.report.ok || model.report.languages.contains(lang.code) ? "" : " (not available)"))
                        .tag(lang.code)
                }
            }
            .frame(maxWidth: 300)
            Button("Apply Language") { model.applyLanguage() }
                .disabled(!model.canApplyLanguage)
        }
    }

    private var progress: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let fraction = model.fraction {
                ProgressView(value: fraction)
            } else {
                ProgressView().progressViewStyle(.linear)
            }
            HStack {
                Text(model.message).font(.callout).foregroundStyle(.secondary)
                Spacer()
                Button("Cancel") { model.cancel() }
            }
            Text("Installing to \(model.destination)")
                .font(.caption)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
        }
    }

    private func name(_ code: String?) -> String {
        model.languages.first { $0.code == code }?.name ?? "no language"
    }
}
