import Foundation

@main
struct ClipFileStoreTests {
    static func main() throws {
        let base = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        defer { do { try FileManager.default.removeItem(at: base) } catch { fatalError("Cleanup failed: \(error)") } }
        let content = "---\nsource: https://example.org\n---\n\n# 中文正文\n"
        let file = try ClipFileStore.save(folder: "网页剪辑/资料", filename: "测试.md", content: content, downloads: base)
        let saved = try String(contentsOf: file, encoding: .utf8)
        precondition(saved == content)
        do {
            _ = try ClipFileStore.save(folder: "网页剪辑/资料", filename: "测试.md", content: "overwrite", downloads: base)
            fatalError("A duplicate save must not overwrite the file")
        } catch let error as NSError { precondition(error.code == NSFileWriteFileExistsError) }
        let unchanged = try String(contentsOf: file, encoding: .utf8)
        precondition(unchanged == content)
        for folder in ["../outside", "/absolute", "x/../outside", "x\\outside"] {
            do {
                _ = try ClipFileStore.save(folder: folder, filename: "test.md", content: content, downloads: base)
                fatalError("Folder traversal was accepted")
            } catch ClipFileError.invalidFolder { }
        }
        do {
            _ = try ClipFileStore.save(folder: "", filename: "../outside.md", content: content, downloads: base)
            fatalError("Filename traversal was accepted")
        } catch ClipFileError.invalidFilename { }
        print("Safari file save: UTF-8 content, nested directories, no overwrites and path boundaries passed.")
    }
}
