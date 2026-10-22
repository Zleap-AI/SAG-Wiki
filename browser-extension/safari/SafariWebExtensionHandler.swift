import SafariServices
import AppKit

class SafariWebExtensionHandler: NSObject, NSExtensionRequestHandling {
    private func reply(_ value: [String: Any], to context: NSExtensionContext) {
        let response = NSExtensionItem()
        response.userInfo = [SFExtensionMessageKey: value]
        context.completeRequest(returningItems: [response], completionHandler: nil)
    }
    private func failure(_ error: Error, to context: NSExtensionContext) {
        var result: [String: Any] = ["error": error.localizedDescription]
        if let fileError = error as? ClipFileError {
            result["code"] = fileError.rawValue
        } else if let fileError = error as? CocoaError {
            switch fileError.code {
            case .fileWriteFileExists: result["code"] = "file-exists"
            case .fileReadNoPermission, .fileWriteNoPermission: result["code"] = "permission-denied"
            case .fileWriteOutOfSpace: result["code"] = "storage-full"
            default: break
            }
        }
        reply(result, to: context)
    }
    private func savedDirectory(_ bookmarkKey: String) throws -> URL? {
        guard let data = UserDefaults.standard.data(forKey: bookmarkKey) else { return nil }
        var stale = false
        let url = try URL(resolvingBookmarkData: data, options: .withSecurityScope, relativeTo: nil, bookmarkDataIsStale: &stale)
        if stale {
            guard url.startAccessingSecurityScopedResource() else { return nil }
            defer { url.stopAccessingSecurityScopedResource() }
            UserDefaults.standard.set(try url.bookmarkData(options: .withSecurityScope, includingResourceValuesForKeys: nil, relativeTo: nil), forKey: bookmarkKey)
        }
        return url
    }
    func beginRequest(with context: NSExtensionContext) {
        let request = context.inputItems.first as? NSExtensionItem
        guard let message = request?.userInfo?[SFExtensionMessageKey] as? [String: Any], let action = message["action"] as? String else {
            reply(["error": "Invalid clipping request."], to: context); return
        }
        let isDefault = message["destination"] as? String == "default"
        let bookmarkKey = isDefault ? "defaultClippingDirectory" : "clippingDirectory"
        let subfolder = isDefault ? "sag-wiki/clippings" : ""
        if action == "choose" {
            DispatchQueue.main.async {
                let panel = NSOpenPanel()
                let chinese = (message["language"] as? String)?.hasPrefix("zh") == true
                panel.title = isDefault ? (chinese ? "选择“文档”文件夹" : "Choose the Documents folder") : (chinese ? "选择网页剪藏保存文件夹" : "Choose clipping folder")
                panel.prompt = chinese ? "选择" : "Choose"
                panel.canChooseFiles = false
                panel.canChooseDirectories = true
                panel.canCreateDirectories = true
                panel.allowsMultipleSelection = false
                panel.level = .floating
                if let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first {
                    panel.directoryURL = documents
                }
                panel.begin { result in
                    guard result == .OK, let url = panel.url else { self.reply(["cancelled": true], to: context); return }
                    let accessing = url.startAccessingSecurityScopedResource()
                    defer { if accessing { url.stopAccessingSecurityScopedResource() } }
                    do {
                        let target = isDefault ? url.appendingPathComponent(subfolder, isDirectory: true) : url
                        let bookmark = try url.bookmarkData(options: .withSecurityScope, includingResourceValuesForKeys: nil, relativeTo: nil)
                        if message["persist"] as? Bool != false {
                            if isDefault { try FileManager.default.createDirectory(at: target, withIntermediateDirectories: true) }
                            UserDefaults.standard.set(bookmark, forKey: bookmarkKey)
                        }
                        self.reply(["path": target.path, "bookmark": bookmark.base64EncodedString()], to: context)
                    } catch { self.failure(error, to: context) }
                }
            }
            return
        }
        do {
            if action == "commit" {
                guard let encoded = message["bookmark"] as? String, let bookmark = Data(base64Encoded: encoded) else { throw ClipFileError.invalidFolder }
                var stale = false
                let selected = try URL(resolvingBookmarkData: bookmark, options: .withSecurityScope, relativeTo: nil, bookmarkDataIsStale: &stale)
                guard selected.startAccessingSecurityScopedResource() else {
                    reply(["error": "Choose a save folder again.", "code": "folder-required"], to: context); return
                }
                defer { selected.stopAccessingSecurityScopedResource() }
                let target = isDefault ? selected.appendingPathComponent(subfolder, isDirectory: true) : selected
                if isDefault { try FileManager.default.createDirectory(at: target, withIntermediateDirectories: true) }
                UserDefaults.standard.set(bookmark, forKey: bookmarkKey)
                reply(["path": target.path], to: context); return
            }
            if action == "status" {
                if isDefault {
                    reply(["path": ClipFileStore.defaultDirectory.path, "ready": true], to: context); return
                }
                let directory = try savedDirectory(bookmarkKey)
                let ready = directory?.startAccessingSecurityScopedResource() ?? false
                defer { if ready { directory?.stopAccessingSecurityScopedResource() } }
                reply(["path": (isDefault ? directory?.appendingPathComponent(subfolder, isDirectory: true) : directory)?.path ?? "", "ready": ready], to: context); return
            }
            guard action == "save", let folder = message["folder"] as? String,
                  let filename = message["filename"] as? String,
                  let content = message["content"] as? String else { throw ClipFileError.invalidFilename }
            let directory: URL
            let useDownloads = message["useDownloads"] as? Bool == true
            if isDefault {
                directory = ClipFileStore.defaultDirectory
            } else if useDownloads {
                directory = try FileManager.default.url(for: .downloadsDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            } else {
                guard let selected = try savedDirectory(bookmarkKey), selected.startAccessingSecurityScopedResource() else {
                    reply(["error": "Choose and authorize a save folder in extension settings.", "code": "folder-required"], to: context); return
                }
                directory = selected
            }
            defer { if !useDownloads && !isDefault { directory.stopAccessingSecurityScopedResource() } }
            let file = try ClipFileStore.save(folder: useDownloads ? folder : "", filename: filename, content: content, downloads: directory)
            reply(["path": file.path], to: context)
        } catch { failure(error, to: context) }
    }
}
