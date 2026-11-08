// Optional Internet Archive download, with the same safeguards as the Windows setup:
// HTTPS only, the exact size, a SHA-256 check before use, partial files discarded.
import CryptoKit
import Foundation

final class Downloader: NSObject, URLSessionDownloadDelegate, @unchecked Sendable {
    private let progress: @Sendable (Int64, Int64) -> Void
    private var continuation: CheckedContinuation<URL, Error>?
    private let staging: URL
    private let limit: Int64
    private var task: URLSessionDownloadTask?
    private let lock = NSLock()

    private init(staging: URL, limit: Int64, progress: @escaping @Sendable (Int64, Int64) -> Void) {
        self.staging = staging
        self.limit = limit
        self.progress = progress
    }

    /// Download `info.url` into `folder`, verified, and return the final archive path.
    /// A verified earlier download is reused.
    static func fetch(_ info: SetupInfo, into folder: URL, isCancelled: @escaping @Sendable () -> Bool,
                      progress: @escaping @Sendable (String, Double?) -> Void) async throws -> URL {
        let fm = FileManager.default
        try fm.createDirectory(at: folder, withIntermediateDirectories: true)
        let final = folder.appendingPathComponent(info.archive_name)
        if fm.fileExists(atPath: final.path) {
            progress("Checking the previously downloaded file...", nil)
            if try sha256(final, isCancelled: isCancelled, progress: progress) == info.sha256 { return final }
            try? fm.removeItem(at: final)
        }
        guard let url = URL(string: info.url), url.scheme == "https" else {
            throw EngineError(message: "The download address is not HTTPS.")
        }
        let partial = folder.appendingPathComponent(UUID().uuidString + ".partial")
        defer { try? fm.removeItem(at: partial) }
        progress("Connecting to Internet Archive...", nil)
        let total = info.bytes
        let downloader = Downloader(staging: partial, limit: info.bytes) { done, _ in
            let mb = Double(done) / 1_048_576
            progress(String(format: "Downloading game files: %.0f of %.0f MB", mb, Double(total) / 1_048_576),
                     min(1, Double(done) / Double(total)))
        }
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 30
        config.httpAdditionalHeaders = ["User-Agent": "LookingGlass-Setup/1.0 (macOS)"]
        let session = URLSession(configuration: config, delegate: downloader, delegateQueue: nil)
        defer { session.invalidateAndCancel() }
        let watcher = Task.detached {
            while !Task.isCancelled {
                if isCancelled() { downloader.cancel(); return }
                try? await Task.sleep(for: .milliseconds(200))
            }
        }
        defer { watcher.cancel() }
        _ = try await withCheckedThrowingContinuation { (c: CheckedContinuation<URL, Error>) in
            downloader.lock.withLock {
                downloader.continuation = c
                let task = session.downloadTask(with: url)
                downloader.task = task
                task.resume()
            }
        }
        if isCancelled() { throw EngineError(message: "Download cancelled. Nothing was changed.") }
        let size = (try fm.attributesOfItem(atPath: partial.path)[.size] as? NSNumber)?.int64Value ?? -1
        guard size == info.bytes else {
            throw EngineError(message: "The download is incomplete or has changed. Try again, or choose a local archive.")
        }
        progress("Checking the downloaded file...", nil)
        guard try sha256(partial, isCancelled: isCancelled, progress: progress) == info.sha256 else {
            throw EngineError(message: "The download did not pass its integrity check. It has not been installed.")
        }
        try fm.moveItem(at: partial, to: final)
        return final
    }

    static func sha256(_ file: URL, isCancelled: @Sendable () -> Bool,
                       progress: @Sendable (String, Double?) -> Void) throws -> String {
        let handle = try FileHandle(forReadingFrom: file)
        defer { try? handle.close() }
        let total = max(1, (try? handle.seekToEnd()) ?? 1)
        try handle.seek(toOffset: 0)
        var hash = SHA256()
        var done: UInt64 = 0
        while let chunk = try handle.read(upToCount: 4 << 20), !chunk.isEmpty {
            if isCancelled() { throw EngineError(message: "Cancelled. Nothing was changed.") }
            hash.update(data: chunk)
            done += UInt64(chunk.count)
            progress("Checking the file...", Double(done) / Double(total))
        }
        return hash.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private func cancel() { lock.withLock { task?.cancel() } }

    private func resume(_ result: Result<URL, Error>) {
        lock.withLock {
            continuation?.resume(with: result)
            continuation = nil
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        // Redirects to the archive's storage servers are fine; leaving HTTPS is not.
        completionHandler(request.url?.scheme == "https" ? request : nil)
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64,
                    totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        progress(totalBytesWritten, totalBytesExpectedToWrite)
        if totalBytesWritten > limit { downloadTask.cancel() } // the file has changed
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        // The temporary file vanishes when this returns, so move it now.
        do {
            guard let http = downloadTask.response as? HTTPURLResponse, http.statusCode == 200 else {
                throw EngineError(message: "Internet Archive answered with an error. Try again later.")
            }
            try FileManager.default.moveItem(at: location, to: staging)
            resume(.success(staging))
        } catch {
            resume(.failure(error))
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        if let error {
            let cancelled = (error as? URLError)?.code == .cancelled
            resume(.failure(cancelled ? EngineError(message: "Download cancelled. Nothing was changed.") : error))
        }
    }
}
