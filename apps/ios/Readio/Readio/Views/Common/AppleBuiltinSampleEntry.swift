enum AppleBuiltinSampleEntry {
    static func destinationItemID(
        libraryIsEmpty: Bool,
        searchOrFilterIsActive: Bool
    ) -> String? {
        guard libraryIsEmpty, !searchOrFilterIsActive else {
            return nil
        }
        return "readio:sample:v1"
    }
}
