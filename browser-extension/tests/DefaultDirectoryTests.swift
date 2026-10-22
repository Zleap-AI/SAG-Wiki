import Foundation

@main
struct DefaultDirectoryTests {
    static func main() throws {
        let directory = ClipFileStore.defaultDirectory
        precondition(directory.path.hasSuffix("/Documents/sag-wiki/clippings"))
        precondition(!directory.path.contains("/Library/Containers/"))
        let filename = "sag-default-save-test-\(UUID().uuidString).md"
        let content = "# Default folder save test\n"
        let file = try ClipFileStore.save(folder: "", filename: filename, content: content, downloads: directory)
        defer { do { try FileManager.default.removeItem(at: file) } catch { fatalError("Cleanup failed: \(error)") } }
        let saved = try String(contentsOf: file, encoding: .utf8)
        precondition(saved == content)
        print("Safari default folder: saved and read Markdown without a bookmark or folder picker.")
    }
}
