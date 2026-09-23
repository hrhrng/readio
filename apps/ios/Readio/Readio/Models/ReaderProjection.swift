import Foundation

/// Native view data projected from one immutable Reader IR. The projection may
/// re-index a chapter for a local playback reducer, but never discovers sentence
/// boundaries, word boundaries, block ownership, or canonical coordinates.
struct AppleReaderProjection: Sendable {
    let reader: ReadioReaderIR
    let chapters: [EpubChapter]
    let navigation: [TOCEntry]
    let allSentences: [Sentence]

    private let sentencesByChapterID: [String: [Sentence]]

    fileprivate init(
        reader: ReadioReaderIR,
        chapters: [EpubChapter],
        navigation: [TOCEntry],
        allSentences: [Sentence],
        sentencesByChapterID: [String: [Sentence]]
    ) {
        self.reader = reader
        self.chapters = chapters
        self.navigation = navigation
        self.allSentences = allSentences
        self.sentencesByChapterID = sentencesByChapterID
    }

    func sentences(in chapterID: String?) -> [Sentence] {
        guard let chapterID else { return allSentences }
        return sentencesByChapterID[chapterID] ?? []
    }

    func chapterID(containing charOffset: UInt64) -> String? {
        reader.chapters.first(where: { chapter in
            chapter.charRange.start <= charOffset
                && charOffset < chapter.charRange.end
        })?.id ?? reader.chapters.last(where: {
            $0.charRange.start <= charOffset
        })?.id
    }

    func localSentenceIndex(readerIndex: UInt32, in chapterID: String?) -> Int? {
        sentences(in: chapterID).firstIndex {
            $0.readerIndex == Int(readerIndex)
        }
    }
}

extension ReadioReaderIR {
    func projectedForApple(
        assetMediaTypes: [String: String] = [:]
    ) -> AppleReaderProjection {
        let allSentences = textIndex.sentences.enumerated().map { localIndex, sentence in
            Sentence(
                index: localIndex,
                text: sentence.text,
                charOffset: Int(sentence.charRange.start),
                readerIndex: Int(sentence.index)
            )
        }
        let indexedByBlock = Dictionary(
            grouping: textIndex.sentences,
            by: \.blockID
        )
        var sentencesByChapterID: [String: [Sentence]] = [:]
        var projectedChapters: [EpubChapter] = []

        for chapter in chapters {
            let chapterSentences = textIndex.sentences.filter {
                chapter.sentenceRange.start <= UInt64($0.index)
                    && UInt64($0.index) < chapter.sentenceRange.end
            }
            let localSentences = chapterSentences.enumerated().map { localIndex, sentence in
                Sentence(
                    index: localIndex,
                    text: sentence.text,
                    charOffset: Int(sentence.charRange.start),
                    readerIndex: Int(sentence.index)
                )
            }
            let localIndexByReaderIndex = Dictionary(
                uniqueKeysWithValues: localSentences.map {
                    ($0.readerIndex, $0.index)
                }
            )
            let start = min(Int(chapter.blockRange.start), document.blocks.count)
            let end = min(
                max(start, Int(chapter.blockRange.end)),
                document.blocks.count
            )
            let blocks = document.blocks[start..<end].map { block in
                projectBlock(
                    block,
                    indexedSentences: indexedByBlock[block.id] ?? [],
                    localIndexByReaderIndex: localIndexByReaderIndex,
                    assets: document.assets,
                    assetMediaTypes: assetMediaTypes
                )
            }
            sentencesByChapterID[chapter.id] = localSentences
            projectedChapters.append(
                EpubChapter(
                    id: chapter.id,
                    title: chapter.title,
                    blocks: blocks
                )
            )
        }

        let projectedNavigation = navigation.map { entry in
            TOCEntry(
                id: entry.id,
                title: entry.title,
                href: nil,
                depth: entry.depth,
                chapterId: entry.chapterID,
                blockId: entry.blockID,
                sentenceIndex: entry.sentenceIndex.flatMap { readerIndex in
                    sentencesByChapterID[entry.chapterID]?.firstIndex {
                        $0.readerIndex == Int(readerIndex)
                    }
                }
            )
        }
        let chaptersWithNavigation = projectedChapters.map { chapter in
            EpubChapter(
                id: chapter.id,
                title: chapter.title,
                blocks: chapter.blocks,
                tocEntries: projectedNavigation.filter {
                    $0.chapterId == chapter.id
                }
            )
        }

        return AppleReaderProjection(
            reader: self,
            chapters: chaptersWithNavigation,
            navigation: projectedNavigation,
            allSentences: allSentences,
            sentencesByChapterID: sentencesByChapterID
        )
    }

    private func projectBlock(
        _ block: ReadioDocumentBlock,
        indexedSentences: [ReadioIndexedSentence],
        localIndexByReaderIndex: [Int: Int],
        assets: [ReadioDocumentAsset],
        assetMediaTypes: [String: String]
    ) -> ContentBlock {
        let richSentences = indexedSentences.compactMap { sentence -> RichSentence? in
            guard let localIndex = localIndexByReaderIndex[Int(sentence.index)] else {
                return nil
            }
            return RichSentence(
                index: localIndex,
                text: sentence.text,
                runs: projectRuns(
                    text: block.text,
                    sentenceRange: sentence.range,
                    spans: block.inlineSpans
                )
            )
        }

        switch block.kind.type {
        case .heading:
            return .heading(
                level: Int(block.kind.level ?? 1),
                sentences: richSentences,
                anchorId: block.id
            )
        case .paragraph:
            return .paragraph(sentences: richSentences, anchorId: block.id)
        case .quote:
            return .blockquote(sentences: richSentences, anchorId: block.id)
        case .code:
            return .code(text: block.text, language: nil)
        case .image:
            let assetID = block.kind.assetID
            let asset = assets.first { $0.id == assetID }
            let remoteSource = asset.flatMap { asset -> String? in
                guard let scheme = URL(string: asset.source)?.scheme?.lowercased(),
                      scheme == "http" || scheme == "https" else { return nil }
                return asset.source
            }
            return .image(
                src: remoteSource,
                assetId: remoteSource == nil ? assetID : nil,
                mediaType: assetID.flatMap { assetMediaTypes[$0] },
                alt: asset?.alt.nilIfEmpty ?? block.text.nilIfEmpty
            )
        }
    }

    private func projectRuns(
        text: String,
        sentenceRange: ReadioScalarRange,
        spans: [ReadioDocumentInlineSpan]
    ) -> [StyledRun] {
        let scalarCount = UInt64(text.unicodeScalars.count)
        let sentenceStart = min(sentenceRange.start, scalarCount)
        let sentenceEnd = min(max(sentenceStart, sentenceRange.end), scalarCount)
        var boundaries = Set([sentenceStart, sentenceEnd])
        for span in spans {
            let start = max(sentenceStart, span.range.start)
            let end = min(sentenceEnd, span.range.end)
            if start < end {
                boundaries.insert(start)
                boundaries.insert(end)
            }
        }
        let ordered = boundaries.sorted()
        var runs: [StyledRun] = []
        for (start, end) in zip(ordered, ordered.dropFirst()) where start < end {
            let covering = spans.filter {
                $0.range.start <= start && end <= $0.range.end
            }
            let run = StyledRun(
                text: scalarSlice(text, from: start, to: end),
                bold: covering.contains(where: \.bold),
                italic: covering.contains(where: \.italic)
            )
            if let previous = runs.last,
               previous.bold == run.bold,
               previous.italic == run.italic,
               previous.href == run.href {
                runs[runs.count - 1] = StyledRun(
                    text: previous.text + run.text,
                    bold: run.bold,
                    italic: run.italic,
                    href: run.href
                )
            } else {
                runs.append(run)
            }
        }
        return runs.isEmpty
            ? [StyledRun(text: scalarSlice(text, from: sentenceStart, to: sentenceEnd))]
            : runs
    }

    private func scalarSlice(_ value: String, from start: UInt64, to end: UInt64) -> String {
        let scalars = value.unicodeScalars
        let lower = scalars.index(scalars.startIndex, offsetBy: Int(start))
        let upper = scalars.index(scalars.startIndex, offsetBy: Int(end))
        return String(scalars[lower..<upper])
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
