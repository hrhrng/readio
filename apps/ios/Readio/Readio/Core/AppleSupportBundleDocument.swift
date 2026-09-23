import SwiftUI
import UniformTypeIdentifiers

struct ReadioSupportBundleDocument: FileDocument {
    static let readableContentTypes: [UTType] = [.json]

    let file: ReadioSupportBundleFile

    init(file: ReadioSupportBundleFile) {
        self.file = file
    }

    init(configuration: ReadConfiguration) throws {
        guard let data = configuration.file.regularFileContents else {
            throw CocoaError(.fileReadCorruptFile)
        }
        let bundle = try JSONDecoder().decode(ReadioSupportBundle.self, from: data)
        file = try ReadioSupportBundleFile(bundle: bundle)
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: file.data)
    }
}
