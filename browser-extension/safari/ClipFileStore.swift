import Foundation
import Darwin

enum ClipFileError: String, Error, LocalizedError {
    case invalidFolder = "invalid-folder"
    case invalidFilename = "invalid-filename"
    case emptyContent = "empty-content"
    var errorDescription: String? {
        switch self {
        case .invalidFolder: return "Invalid folder. Use a subfolder under Downloads."
        case .invalidFilename: return "Invalid Markdown filename."
        case .emptyContent: return "The page contains no text to save."
        }
    }
}

struct ClipFileStore {
    static var defaultDirectory: URL {
        let home = URL(fileURLWithPath: String(cString: getpwuid(getuid())!.pointee.pw_dir), isDirectory: true)
        return home.appendingPathComponent("Documents/sag-wiki/clippings", isDirectory: true)
    }

    static func save(folder: String, filename: String, content: String, downloads: URL) throws -> URL {
        let invalid = CharacterSet(charactersIn: "<>:\"\\|?*").union(.controlCharacters)
        if !folder.isEmpty && folder.components(separatedBy: "/").contains(where: {
            $0.isEmpty || $0 == "." || $0 == ".." || $0.rangeOfCharacter(from: invalid) != nil
        }) { throw ClipFileError.invalidFolder }
        guard !filename.isEmpty, filename != ".", filename != "..", !filename.contains("/"),
              filename.rangeOfCharacter(from: invalid) == nil, filename.hasSuffix(".md") else {
            throw ClipFileError.invalidFilename
        }
        guard !content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { throw ClipFileError.emptyContent }
        let directory = folder.isEmpty ? downloads : downloads.appendingPathComponent(folder, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let file = directory.appendingPathComponent(filename)
        try Data(content.utf8).write(to: file, options: .withoutOverwriting)
        return file
    }
}
